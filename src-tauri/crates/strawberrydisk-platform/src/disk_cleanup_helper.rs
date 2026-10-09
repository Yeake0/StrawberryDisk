use std::{
    ffi::{OsStr, OsString},
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream},
    os::windows::ffi::OsStrExt,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT},
    Storage::FileSystem::SYNCHRONIZE,
    System::Threading::{
        CreateEventW, GetExitCodeProcess, OpenEventW, OpenProcess, SetEvent, WaitForSingleObject,
    },
};

use crate::{
    PlatformCancellation, PlatformError, PlatformErrorCode, PlatformResult,
    WindowsDiskCleanupAvailability, WindowsDiskCleanupEstimate, WindowsDiskCleanupExecution,
    WindowsDiskCleanupExecutionStatus, WindowsDiskCleanupKind,
};

pub(crate) const HELPER_FLAG: &str = "--strawberrydisk-disk-cleanup-helper-v2";
const PROTOCOL: &str = "strawberrydisk-disk-cleanup-helper-v2";
const MAX_RESPONSE_BYTES: usize = 16 * 1024;
const ESTIMATE_RESPONSE_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const EXECUTION_WAIT_LOG_INTERVAL: Duration = Duration::from_secs(5 * 60);
const HELPER_SUCCESS_EXIT_CODE: i32 = 0;
const HELPER_FAILURE_EXIT_CODE: i32 = 70;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HelperAction {
    Estimate,
    Execute,
}

impl HelperAction {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Estimate => "estimate",
            Self::Execute => "execute",
        }
    }

    /// Read-only estimation must remain bounded, while destructive execution must follow the
    /// elevated process lifetime. Windows can spend an unbounded amount of time waiting for the
    /// native confirmation dialog and completing cleanup, so a fixed execution timeout would turn
    /// a still-running system operation into a false failure.
    const fn response_timeout(self) -> Option<Duration> {
        match self {
            Self::Estimate => Some(ESTIMATE_RESPONSE_TIMEOUT),
            Self::Execute => None,
        }
    }

    fn parse(value: &OsStr) -> Option<Self> {
        match value.to_str()? {
            "estimate" => Some(Self::Estimate),
            "execute" => Some(Self::Execute),
            _ => None,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum HelperPayload {
    Estimate {
        availability: WireAvailability,
        bytes: u64,
        item_count: u64,
        elapsed_ms: u64,
    },
    Execution {
        status: WireExecutionStatus,
        bytes_expected: u64,
        released_bytes: u64,
        affected_item_count: u64,
        failed_item_count: u64,
    },
    Error {
        code: WireErrorCode,
    },
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HelperResponse {
    protocol: String,
    token: String,
    payload: HelperPayload,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
enum WireAvailability {
    Ready,
    NotApplicable,
    Limited,
    ElevationRequired,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
enum WireExecutionStatus {
    Completed,
    Partial,
    VerificationFailed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
enum WireErrorCode {
    InvalidData,
    OperationFailed,
}

/// Owns a Win32 synchronization handle without exposing it outside this narrow helper boundary.
///
/// Storing the pointer-sized handle as an integer lets the cancellation probe remain `Send + Sync`.
/// Waiting and signalling Windows event/process handles are thread-safe operations, and this type
/// closes the handle exactly once when the owning request or callback is dropped.
struct OwnedHandle(usize);

impl OwnedHandle {
    fn from_raw(handle: HANDLE, operation: &'static str) -> PlatformResult<Self> {
        if handle.is_null() {
            return Err(PlatformError::operation_failed(operation));
        }
        Ok(Self(handle as usize))
    }

    fn raw(&self) -> HANDLE {
        self.0 as HANDLE
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.raw()) };
    }
}

/// Runs the fixed Windows disk-cleanup helper before Tauri initializes.
///
/// The helper accepts only estimate or execute operations for the compiled-in Previous
/// Installations handler. It never accepts a path, registry key, handler name, or executable from
/// the desktop process, which keeps this elevation boundary intentionally narrow.
pub fn run_disk_cleanup_helper_mode<I>(arguments: I) -> Option<i32>
where
    I: IntoIterator<Item = OsString>,
{
    let arguments = arguments.into_iter().collect::<Vec<_>>();
    if arguments.get(1).and_then(|value| value.to_str()) != Some(HELPER_FLAG) {
        return None;
    }
    let result =
        helper_arguments(&arguments).and_then(|(action, port, token, parent_process_id)| {
            let response = execute_helper_action(action, &token, parent_process_id);
            send_response(port, &response)
        });
    Some(if result.is_ok() {
        HELPER_SUCCESS_EXIT_CODE
    } else {
        HELPER_FAILURE_EXIT_CODE
    })
}

pub(crate) fn estimate_previous_installations_with_privileges(
) -> PlatformResult<WindowsDiskCleanupEstimate> {
    let cancellation = PlatformCancellation::new(|| false);
    let estimate = match request(HelperAction::Estimate, &cancellation)? {
        HelperPayload::Estimate {
            availability,
            bytes,
            item_count,
            elapsed_ms,
        } => Ok(WindowsDiskCleanupEstimate {
            kind: WindowsDiskCleanupKind::PreviousInstallations,
            availability: availability.into(),
            bytes,
            item_count,
            elapsed_ms,
        }),
        HelperPayload::Error { code } => Err(wire_error(code)),
        _ => Err(PlatformError::new(
            PlatformErrorCode::InvalidData,
            "disk cleanup helper returned an unexpected estimate response",
        )),
    }?;
    log::info!(
        "windows_disk_cleanup_privileged_scan_finished kind={} availability={:?} bytes={} item_count={} native_elapsed_ms={}",
        estimate.kind.stable_id(),
        estimate.availability,
        estimate.bytes,
        estimate.item_count,
        estimate.elapsed_ms
    );
    Ok(estimate)
}

pub(crate) fn execute_previous_installations_with_privileges(
    cancellation: &PlatformCancellation,
) -> PlatformResult<WindowsDiskCleanupExecution> {
    let execution = match request(HelperAction::Execute, cancellation)? {
        HelperPayload::Execution {
            status,
            bytes_expected,
            released_bytes,
            affected_item_count,
            failed_item_count,
        } => Ok(WindowsDiskCleanupExecution {
            kind: WindowsDiskCleanupKind::PreviousInstallations,
            status: status.into(),
            bytes_expected,
            released_bytes,
            affected_item_count,
            failed_item_count,
        }),
        HelperPayload::Error { code } => Err(wire_error(code)),
        _ => Err(PlatformError::new(
            PlatformErrorCode::InvalidData,
            "disk cleanup helper returned an unexpected execution response",
        )
        .with_possible_side_effects()),
    }?;
    log::info!(
        "windows_disk_cleanup_privileged_execution_finished kind={} status={:?} bytes_expected={} released_bytes={} affected_item_count={} failed_item_count={}",
        execution.kind.stable_id(),
        execution.status,
        execution.bytes_expected,
        execution.released_bytes,
        execution.affected_item_count,
        execution.failed_item_count
    );
    Ok(execution)
}

fn request(
    action: HelperAction,
    cancellation: &PlatformCancellation,
) -> PlatformResult<HelperPayload> {
    let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))
        .map_err(|error| PlatformError::io("bind disk cleanup helper response channel", &error))?;
    listener.set_nonblocking(true).map_err(|error| {
        PlatformError::io("configure disk cleanup helper response channel", &error)
    })?;
    let port = listener
        .local_addr()
        .map_err(|error| PlatformError::io("resolve disk cleanup helper response channel", &error))?
        .port();
    let token = unique_token()?;
    let cancellation_event = create_cancellation_event(&token)?;
    log::info!(
        "windows_disk_cleanup_elevation_requested action={}",
        action.as_str()
    );
    let process = launch_elevated(action, port, &token, std::process::id())?;
    let started = Instant::now();
    let response_timeout = action.response_timeout();
    let mut next_wait_log_at = EXECUTION_WAIT_LOG_INTERVAL;
    let mut rejected_connection_count = 0_u32;
    let mut cancellation_signalled = false;
    let result = loop {
        match listener.accept() {
            Ok((stream, _)) => match read_response(stream, &token) {
                Ok(response) => break Ok(response.payload),
                Err(_) => {
                    rejected_connection_count = rejected_connection_count.saturating_add(1);
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => {
                break Err(PlatformError::io(
                    "accept disk cleanup helper response",
                    &error,
                ));
            }
        }
        let elapsed = started.elapsed();
        if response_timeout.is_some_and(|timeout| elapsed >= timeout) {
            if let Err(error) = signal_cancellation(&cancellation_event) {
                break Err(error);
            }
            break Err(PlatformError::operation_failed(
                "disk cleanup helper response timed out",
            ));
        }
        if !cancellation_signalled && cancellation.is_cancelled() {
            if let Err(error) = signal_cancellation(&cancellation_event) {
                break Err(error);
            }
            cancellation_signalled = true;
            log::info!(
                "windows_disk_cleanup_elevation_cancellation_requested action={} elapsed_ms={}",
                action.as_str(),
                elapsed.as_millis()
            );
        }
        if action == HelperAction::Execute && elapsed >= next_wait_log_at {
            log::info!(
                "windows_disk_cleanup_elevation_waiting action={} helper_running=true elapsed_ms={}",
                action.as_str(),
                elapsed.as_millis()
            );
            next_wait_log_at = next_wait_log_at.saturating_add(EXECUTION_WAIT_LOG_INTERVAL);
        }
        let wait = unsafe { WaitForSingleObject(process, 100) };
        if wait == WAIT_TIMEOUT {
            continue;
        }
        if wait != WAIT_OBJECT_0 {
            break Err(PlatformError::operation_failed(
                "disk cleanup helper process wait failed",
            ));
        }
        // A completed helper can leave one response in the listener backlog. Poll once more before
        // interpreting the process exit code as a protocol failure.
        match listener.accept() {
            Ok((stream, _)) => {
                break read_response(stream, &token).map(|response| response.payload)
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                break Err(PlatformError::operation_failed(
                    "disk cleanup helper exited without a response",
                ));
            }
            Err(error) => {
                break Err(PlatformError::io(
                    "accept completed disk cleanup helper response",
                    &error,
                ));
            }
        }
    };
    let mut exit_code = HELPER_FAILURE_EXIT_CODE as u32;
    let exit_read = unsafe { GetExitCodeProcess(process, &mut exit_code) };
    unsafe { CloseHandle(process) };
    let response_ok =
        matches!(&result, Ok(payload) if !matches!(payload, HelperPayload::Error { .. }));
    log::info!(
        "windows_disk_cleanup_elevation_finished action={} response_ok={} cancellation_signalled={} helper_exit_code={} rejected_connection_count={} elapsed_ms={}",
        action.as_str(),
        response_ok,
        cancellation_signalled,
        if exit_read == 0 { u32::MAX } else { exit_code },
        rejected_connection_count,
        started.elapsed().as_millis()
    );
    preserve_request_failure_semantics(action, result)
}

fn execute_helper_action(
    action: HelperAction,
    token: &str,
    parent_process_id: u32,
) -> HelperResponse {
    let cancellation = helper_cancellation(token, parent_process_id);
    let payload = match crate::windows::disk_cleanup_helper_is_elevated() {
        Ok(true) => match cancellation {
            Ok(cancellation) => execute_elevated_action(action, &cancellation),
            Err(error) => HelperPayload::Error {
                code: wire_error_code(error.code()),
            },
        },
        Ok(false) | Err(_) => HelperPayload::Error {
            code: WireErrorCode::OperationFailed,
        },
    };
    HelperResponse {
        protocol: PROTOCOL.to_string(),
        token: token.to_string(),
        payload,
    }
}

/// Executes only after the helper has verified its elevated token. This prevents a launch or
/// policy anomaly from being reported as a successful zero-byte system-cleanup result.
fn execute_elevated_action(
    action: HelperAction,
    cancellation: &PlatformCancellation,
) -> HelperPayload {
    match action {
        HelperAction::Estimate => crate::windows::fresh_windows_disk_cleanup_estimates(
            &[WindowsDiskCleanupKind::PreviousInstallations],
            cancellation,
        )
        .into_iter()
        .next()
        .map(HelperPayload::from)
        .unwrap_or(HelperPayload::Error {
            code: WireErrorCode::OperationFailed,
        }),
        HelperAction::Execute => HelperPayload::from(crate::windows::execute_windows_disk_cleanup(
            WindowsDiskCleanupKind::PreviousInstallations,
            cancellation,
        )),
    }
}

fn helper_arguments(arguments: &[OsString]) -> PlatformResult<(HelperAction, u16, String, u32)> {
    if arguments.len() != 6 {
        return Err(PlatformError::new(
            PlatformErrorCode::InvalidData,
            "disk cleanup helper argument count is invalid",
        ));
    }
    let action = HelperAction::parse(&arguments[2]).ok_or_else(|| {
        PlatformError::new(
            PlatformErrorCode::InvalidData,
            "disk cleanup helper action is invalid",
        )
    })?;
    let port = arguments[3]
        .to_str()
        .and_then(|value| value.parse::<u16>().ok())
        .filter(|value| *value != 0)
        .ok_or_else(|| {
            PlatformError::new(
                PlatformErrorCode::InvalidData,
                "disk cleanup helper response port is invalid",
            )
        })?;
    let token = arguments[4]
        .to_str()
        .filter(|value| valid_token(value))
        .ok_or_else(|| {
            PlatformError::new(
                PlatformErrorCode::InvalidData,
                "disk cleanup helper response token is invalid",
            )
        })?;
    let parent_process_id = arguments[5]
        .to_str()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|value| *value != 0)
        .ok_or_else(|| {
            PlatformError::new(
                PlatformErrorCode::InvalidData,
                "disk cleanup helper parent process identifier is invalid",
            )
        })?;
    Ok((action, port, token.to_string(), parent_process_id))
}

fn launch_elevated(
    action: HelperAction,
    port: u16,
    token: &str,
    parent_process_id: u32,
) -> PlatformResult<HANDLE> {
    crate::elevation::launch_platform(crate::elevation::LaunchRequest::DiskCleanup {
        execute: action == HelperAction::Execute,
        port,
        token: token.to_owned(),
        parent_pid: parent_process_id,
    })
}

fn cancellation_event_name(token: &str) -> String {
    format!("Local\\StrawberryDisk-DiskCleanup-{token}")
}

fn create_cancellation_event(token: &str) -> PlatformResult<OwnedHandle> {
    let name = wide(OsStr::new(&cancellation_event_name(token)));
    let handle = unsafe { CreateEventW(std::ptr::null(), 1, 0, name.as_ptr()) };
    OwnedHandle::from_raw(
        handle,
        "create disk cleanup helper cancellation event failed",
    )
}

fn signal_cancellation(event: &OwnedHandle) -> PlatformResult<()> {
    if unsafe { SetEvent(event.raw()) } == 0 {
        return Err(PlatformError::operation_failed(
            "signal disk cleanup helper cancellation failed",
        ));
    }
    Ok(())
}

fn helper_cancellation(
    token: &str,
    parent_process_id: u32,
) -> PlatformResult<PlatformCancellation> {
    let name = wide(OsStr::new(&cancellation_event_name(token)));
    let cancellation_event = OwnedHandle::from_raw(
        unsafe { OpenEventW(SYNCHRONIZE, 0, name.as_ptr()) },
        "open disk cleanup helper cancellation event failed",
    )?;
    let parent_process = OwnedHandle::from_raw(
        unsafe { OpenProcess(SYNCHRONIZE, 0, parent_process_id) },
        "open disk cleanup helper parent process failed",
    )?;
    Ok(PlatformCancellation::new(move || {
        handle_is_signalled_or_invalid(&cancellation_event)
            || handle_is_signalled_or_invalid(&parent_process)
    }))
}

fn handle_is_signalled_or_invalid(handle: &OwnedHandle) -> bool {
    matches!(
        unsafe { WaitForSingleObject(handle.raw(), 0) },
        WAIT_OBJECT_0 | WAIT_FAILED
    )
}

fn send_response(port: u16, response: &HelperResponse) -> PlatformResult<()> {
    let bytes = serde_json::to_vec(response).map_err(|_| {
        PlatformError::new(
            PlatformErrorCode::InvalidData,
            "serialize disk cleanup helper response failed",
        )
    })?;
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err(PlatformError::new(
            PlatformErrorCode::InvalidData,
            "disk cleanup helper response is too large",
        ));
    }
    let mut stream = TcpStream::connect_timeout(
        &SocketAddrV4::new(Ipv4Addr::LOCALHOST, port).into(),
        Duration::from_secs(5),
    )
    .map_err(|error| PlatformError::io("connect disk cleanup helper response channel", &error))?;
    stream
        .write_all(&bytes)
        .and_then(|()| stream.flush())
        .map_err(|error| PlatformError::io("write disk cleanup helper response", &error))
}

fn read_response(stream: TcpStream, expected_token: &str) -> PlatformResult<HelperResponse> {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|error| PlatformError::io("configure disk cleanup helper response", &error))?;
    let mut bytes = Vec::with_capacity(1024);
    stream
        .take((MAX_RESPONSE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| PlatformError::io("read disk cleanup helper response", &error))?;
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err(PlatformError::new(
            PlatformErrorCode::InvalidData,
            "disk cleanup helper response is too large",
        ));
    }
    let response = serde_json::from_slice::<HelperResponse>(&bytes).map_err(|_| {
        PlatformError::new(
            PlatformErrorCode::InvalidData,
            "disk cleanup helper response is invalid",
        )
    })?;
    if response.protocol != PROTOCOL || response.token != expected_token {
        return Err(PlatformError::new(
            PlatformErrorCode::InvalidData,
            "disk cleanup helper response correlation failed",
        ));
    }
    Ok(response)
}

fn unique_token() -> PlatformResult<String> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| {
        PlatformError::operation_failed("generate disk cleanup helper correlation token failed")
    })?;
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut token = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        token.push(char::from(HEX[(byte >> 4) as usize]));
        token.push(char::from(HEX[(byte & 0x0f) as usize]));
    }
    Ok(token)
}

fn valid_token(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn wide(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(std::iter::once(0)).collect()
}

fn wire_error(code: WireErrorCode) -> PlatformError {
    PlatformError::new(
        match code {
            WireErrorCode::InvalidData => PlatformErrorCode::InvalidData,
            WireErrorCode::OperationFailed => PlatformErrorCode::OperationFailed,
        },
        "disk cleanup helper failed",
    )
}

fn wire_error_code(code: PlatformErrorCode) -> WireErrorCode {
    match code {
        PlatformErrorCode::InvalidData | PlatformErrorCode::InvalidPath => {
            WireErrorCode::InvalidData
        }
        _ => WireErrorCode::OperationFailed,
    }
}

fn preserve_request_failure_semantics(
    action: HelperAction,
    result: PlatformResult<HelperPayload>,
) -> PlatformResult<HelperPayload> {
    if action == HelperAction::Execute {
        // Once the elevated execute process has launched, losing its response cannot prove that
        // the native Purge call never ran. Preserve that uncertainty so Core never tells the user
        // that no files changed after an IPC, process, or verification failure.
        result.map_err(PlatformError::with_possible_side_effects)
    } else {
        result
    }
}

impl From<WindowsDiskCleanupEstimate> for HelperPayload {
    fn from(value: WindowsDiskCleanupEstimate) -> Self {
        Self::Estimate {
            availability: value.availability.into(),
            bytes: value.bytes,
            item_count: value.item_count,
            elapsed_ms: value.elapsed_ms,
        }
    }
}

impl From<WindowsDiskCleanupExecution> for HelperPayload {
    fn from(value: WindowsDiskCleanupExecution) -> Self {
        Self::Execution {
            status: value.status.into(),
            bytes_expected: value.bytes_expected,
            released_bytes: value.released_bytes,
            affected_item_count: value.affected_item_count,
            failed_item_count: value.failed_item_count,
        }
    }
}

impl From<WindowsDiskCleanupAvailability> for WireAvailability {
    fn from(value: WindowsDiskCleanupAvailability) -> Self {
        match value {
            WindowsDiskCleanupAvailability::Ready => Self::Ready,
            WindowsDiskCleanupAvailability::NotApplicable => Self::NotApplicable,
            WindowsDiskCleanupAvailability::Limited => Self::Limited,
            WindowsDiskCleanupAvailability::ElevationRequired => Self::ElevationRequired,
        }
    }
}

impl From<WireAvailability> for WindowsDiskCleanupAvailability {
    fn from(value: WireAvailability) -> Self {
        match value {
            WireAvailability::Ready => Self::Ready,
            WireAvailability::NotApplicable => Self::NotApplicable,
            WireAvailability::Limited => Self::Limited,
            WireAvailability::ElevationRequired => Self::ElevationRequired,
        }
    }
}

impl From<WindowsDiskCleanupExecutionStatus> for WireExecutionStatus {
    fn from(value: WindowsDiskCleanupExecutionStatus) -> Self {
        match value {
            WindowsDiskCleanupExecutionStatus::Completed => Self::Completed,
            WindowsDiskCleanupExecutionStatus::Partial => Self::Partial,
            WindowsDiskCleanupExecutionStatus::VerificationFailed => Self::VerificationFailed,
            WindowsDiskCleanupExecutionStatus::Failed => Self::Failed,
            WindowsDiskCleanupExecutionStatus::Cancelled => Self::Cancelled,
        }
    }
}

impl From<WireExecutionStatus> for WindowsDiskCleanupExecutionStatus {
    fn from(value: WireExecutionStatus) -> Self {
        match value {
            WireExecutionStatus::Completed => Self::Completed,
            WireExecutionStatus::Partial => Self::Partial,
            WireExecutionStatus::VerificationFailed => Self::VerificationFailed,
            WireExecutionStatus::Failed => Self::Failed,
            WireExecutionStatus::Cancelled => Self::Cancelled,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helper_mode_ignores_ordinary_desktop_arguments() {
        assert_eq!(
            run_disk_cleanup_helper_mode([OsString::from("StrawberryDisk")]),
            None
        );
    }

    #[test]
    fn helper_arguments_accept_only_the_fixed_protocol_shape() {
        let token = "a".repeat(64);
        let arguments = [
            OsString::from("StrawberryDisk"),
            OsString::from(HELPER_FLAG),
            OsString::from("estimate"),
            OsString::from("48152"),
            OsString::from(&token),
            OsString::from("1234"),
        ];
        assert_eq!(
            helper_arguments(&arguments).expect("valid helper arguments"),
            (HelperAction::Estimate, 48152, token, 1234)
        );

        let mut invalid = arguments;
        invalid[2] = OsString::from("arbitrary-command");
        assert!(helper_arguments(&invalid).is_err());
    }

    #[test]
    fn helper_timeout_is_bounded_only_for_read_only_estimation() {
        assert_eq!(
            HelperAction::Estimate.response_timeout(),
            Some(Duration::from_secs(15 * 60))
        );
        assert_eq!(HelperAction::Execute.response_timeout(), None);
    }

    #[test]
    fn execute_transport_failure_preserves_possible_side_effects() {
        let execution_error = preserve_request_failure_semantics(
            HelperAction::Execute,
            Err(PlatformError::operation_failed("response unavailable")),
        )
        .expect_err("an unavailable execute response must remain an error");
        assert_eq!(
            execution_error.mutation_state(),
            crate::PlatformMutationState::MayHaveChanged
        );

        let estimate_error = preserve_request_failure_semantics(
            HelperAction::Estimate,
            Err(PlatformError::operation_failed("response unavailable")),
        )
        .expect_err("an unavailable estimate response must remain an error");
        assert_eq!(
            estimate_error.mutation_state(),
            crate::PlatformMutationState::NotAttempted
        );
    }

    #[test]
    fn cancellation_event_reaches_the_elevated_probe() {
        let token = unique_token().expect("generate cancellation token");
        let event = create_cancellation_event(&token).expect("create cancellation event");
        let cancellation = helper_cancellation(&token, std::process::id())
            .expect("open cancellation event and parent process");

        assert!(!cancellation.is_cancelled());
        signal_cancellation(&event).expect("signal cancellation event");
        assert!(cancellation.is_cancelled());
    }

    #[test]
    fn response_rejects_the_wrong_correlation_token() {
        let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))
            .expect("bind response fixture");
        let address = listener.local_addr().expect("fixture address");
        let sender = std::thread::spawn(move || {
            let response = HelperResponse {
                protocol: PROTOCOL.to_string(),
                token: "b".repeat(64),
                payload: HelperPayload::Error {
                    code: WireErrorCode::OperationFailed,
                },
            };
            let mut stream = TcpStream::connect(address).expect("connect response fixture");
            stream
                .write_all(&serde_json::to_vec(&response).expect("serialize response"))
                .expect("write response");
        });
        let (stream, _) = listener.accept().expect("accept response fixture");
        assert!(read_response(stream, &"a".repeat(64)).is_err());
        sender.join().expect("join response fixture");
    }
}

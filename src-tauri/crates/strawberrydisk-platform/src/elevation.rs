//! Process-lifetime Windows authorization, shared by the platform's privileged adapters.
//!
//! Only this module asks Windows for elevation. The broker launches a finite set of
//! capabilities; domain helpers still validate their own requests and report their results.
//! It never accepts a caller-selected executable, shell command, or helper flag.

mod parent;
mod peer;
mod protocol;

use std::{
    ffi::{OsStr, OsString},
    io::{self, BufReader},
    net::{Ipv4Addr, TcpListener, TcpStream},
    os::windows::{ffi::OsStrExt, io::AsRawHandle},
    process::{Command, Stdio},
    ptr,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

use windows_sys::Win32::{
    Foundation::{
        CloseHandle, DuplicateHandle, GetLastError, ERROR_BROKEN_PIPE, ERROR_CANCELLED,
        ERROR_INVALID_DATA, ERROR_TIMEOUT, FILETIME, HANDLE, WAIT_TIMEOUT,
    },
    Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY},
    Storage::FileSystem::SYNCHRONIZE,
    System::Threading::{
        GetCurrentProcess, GetProcessId, GetProcessTimes, OpenProcessToken, WaitForSingleObject,
        PROCESS_QUERY_LIMITED_INFORMATION,
    },
    UI::{
        Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW},
        WindowsAndMessaging::SW_HIDE,
    },
};

use crate::{PlatformError, PlatformErrorCode, PlatformResult};
use protocol::{read_message, write_message, Request, Response, PROTOCOL};
pub(crate) use protocol::{LaunchRequest, RejectionReason, UninstallRequest};

const FLAG: &str = "--strawberrydisk-elevation-helper-v1";
const START_TIMEOUT: Duration = Duration::from_secs(120);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
static SESSION: OnceLock<Mutex<Option<Session>>> = OnceLock::new();

struct OwnedHandle(HANDLE);

// A process handle has no thread affinity. SESSION serializes access and Drop closes it once.
unsafe impl Send for OwnedHandle {}
// The handle is immutable; the Arc keeps it open while the lifetime watcher waits.
unsafe impl Sync for OwnedHandle {}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

struct Session {
    stream: BufReader<TcpStream>,
    process: OwnedHandle,
    label: String,
    next_request: u64,
}

#[derive(Debug)]
pub(crate) struct LaunchError {
    pub code: u32,
    pub may_have_started: bool,
    pub reason: RejectionReason,
}

impl LaunchError {
    fn before_start(code: u32) -> Self {
        Self {
            code,
            may_have_started: false,
            reason: RejectionReason::Native,
        }
    }

    pub(crate) fn into_platform(self) -> PlatformError {
        let error = PlatformError::new(
            if self.reason == RejectionReason::ItemChanged {
                PlatformErrorCode::ItemChanged
            } else if self.code == ERROR_CANCELLED {
                PlatformErrorCode::UserCancelled
            } else {
                PlatformErrorCode::OperationFailed
            },
            "Windows privileged process launch failed",
        );
        if self.may_have_started {
            error.with_possible_side_effects()
        } else {
            error
        }
    }
}

pub(crate) fn launch_platform(request: LaunchRequest) -> PlatformResult<HANDLE> {
    launch(request).map_err(LaunchError::into_platform)
}

/// The lock covers authentication and launch only, never the domain operation's lifetime.
/// A lost response is not retried: the child may already be modifying the machine.
pub(crate) fn launch(request: LaunchRequest) -> Result<HANDLE, LaunchError> {
    let started = Instant::now();
    let capability = request.capability();
    let mut slot = SESSION
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| LaunchError::before_start(ERROR_BROKEN_PIPE))?;
    if let Some(session) = slot.as_mut() {
        if session.ping().is_err() {
            log::info!(
                "windows_elevation_session_expired session_id={} reason=health_check",
                session.label
            );
            *slot = None;
        } else {
            log::info!(
                "windows_elevation_session_reused session_id={} capability={capability}",
                session.label
            );
        }
    }
    if slot.is_none() {
        *slot = Some(Session::start(capability)?);
    }
    let session = slot.as_mut().expect("authorization established a session");
    let request_id = session.next_request;
    session.next_request += 1;
    log::info!("windows_elevation_launch_requested session_id={} request_id={request_id} capability={capability}", session.label);
    let result = session.exchange(Request::Launch {
        request_id,
        request,
    });
    match result {
        Ok(Response::Launched {
            request_id: returned,
            handle,
            process_id,
        }) if returned == request_id => {
            let handle = handle as usize as HANDLE;
            if handle.is_null() || unsafe { GetProcessId(handle) } != process_id || process_id == 0
            {
                // Do not close an unvalidated integer supplied by an invalid response.
                log::warn!("windows_elevation_session_discarded session_id={} request_id={request_id} capability={capability} reason=invalid_handle", session.label);
                *slot = None;
                return Err(LaunchError {
                    code: ERROR_INVALID_DATA,
                    may_have_started: true,
                    reason: RejectionReason::Native,
                });
            }
            log::info!("windows_elevation_launch_finished session_id={} request_id={request_id} capability={capability} status=started child_pid={process_id} elapsed_ms={}", session.label, started.elapsed().as_millis());
            Ok(handle)
        }
        Ok(Response::Rejected {
            request_id: returned,
            code,
            reason,
        }) if returned == request_id => {
            log::warn!("windows_elevation_launch_finished session_id={} request_id={request_id} capability={capability} status=rejected reason={reason:?} native_code={code} elapsed_ms={}", session.label, started.elapsed().as_millis());
            Err(LaunchError {
                code,
                may_have_started: false,
                reason,
            })
        }
        result => {
            let code = result
                .err()
                .map_or(ERROR_INVALID_DATA, |error| native_code(&error));
            log::warn!("windows_elevation_session_discarded session_id={} request_id={request_id} capability={capability} reason=launch_response_lost native_code={code} may_have_started=true elapsed_ms={}", session.label, started.elapsed().as_millis());
            *slot = None;
            Err(LaunchError {
                code,
                may_have_started: true,
                reason: RejectionReason::Native,
            })
        }
    }
}

impl Session {
    fn exchange(&mut self, request: Request) -> io::Result<Response> {
        write_message(self.stream.get_mut(), &request)?;
        read_message(&mut self.stream)
    }

    fn ping(&mut self) -> io::Result<()> {
        if unsafe { WaitForSingleObject(self.process.0, 0) } != WAIT_TIMEOUT {
            return Err(io::Error::from_raw_os_error(ERROR_BROKEN_PIPE as i32));
        }
        match self.exchange(Request::Ping)? {
            Response::Pong => Ok(()),
            _ => Err(io::ErrorKind::InvalidData.into()),
        }
    }

    fn start(capability: &str) -> Result<Self, LaunchError> {
        let started = Instant::now();
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(before_io)?;
        listener.set_nonblocking(true).map_err(before_io)?;
        let port = listener.local_addr().map_err(before_io)?.port();
        let token = secure_token().map_err(before_io)?;
        let label = blake3::hash(token.as_bytes()).to_hex()[..12].to_string();
        log::info!(
            "windows_elevation_authorization_requested session_id={label} capability={capability}"
        );
        let process = authorize(port, &token).inspect_err(|error| {
            log::info!("windows_elevation_authorization_finished session_id={label} capability={capability} status={} native_code={} elapsed_ms={}",
                if error.code == ERROR_CANCELLED { "cancelled" } else { "failed" }, error.code, started.elapsed().as_millis());
        })?;
        // Human consent time is not a transport timeout. Start the handshake deadline
        // only after Windows returns the launched process handle.
        let handshake_started = Instant::now();
        loop {
            if handshake_started.elapsed() >= START_TIMEOUT
                || unsafe { WaitForSingleObject(process.0, 0) } != WAIT_TIMEOUT
            {
                log::warn!("windows_elevation_authorization_finished session_id={label} capability={capability} status=handshake_failed elapsed_ms={}", started.elapsed().as_millis());
                return Err(LaunchError::before_start(ERROR_TIMEOUT));
            }
            match listener.accept() {
                Ok((stream, _)) => {
                    if peer::verify_peer_process(&stream, unsafe { GetProcessId(process.0) })
                        .is_err()
                    {
                        log::warn!("windows_elevation_peer_rejected session_id={label} reason=process_identity");
                        continue;
                    }
                    configure_client_stream(&stream, Duration::from_secs(2)).map_err(before_io)?;
                    let mut stream = BufReader::new(stream);
                    match read_message::<Response>(&mut stream) {
                        Ok(Response::Hello {
                            protocol,
                            token: received,
                        }) if protocol == PROTOCOL && received == token => {}
                        Ok(Response::BootstrapFailed { code }) => {
                            // The OS peer check above binds this diagnostic to the exact
                            // process launched by consent, even before its Hello arrives.
                            log::warn!("windows_elevation_authorization_finished session_id={label} capability={capability} status=bootstrap_failed native_code={code} elapsed_ms={}", started.elapsed().as_millis());
                            return Err(LaunchError::before_start(code));
                        }
                        _ => continue,
                    }
                    stream
                        .get_mut()
                        .set_read_timeout(Some(REQUEST_TIMEOUT))
                        .map_err(before_io)?;
                    log::info!("windows_elevation_authorization_finished session_id={label} capability={capability} status=ready helper_pid={} elapsed_ms={}", unsafe { GetProcessId(process.0) }, started.elapsed().as_millis());
                    return Ok(Self {
                        stream,
                        process,
                        label,
                        next_request: 1,
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(50))
                }
                Err(error) => return Err(before_io(error)),
            }
        }
    }
}

fn configure_client_stream(stream: &TcpStream, read_timeout: Duration) -> io::Result<()> {
    // Winsock accepted sockets inherit FIONBIO from the polling listener. Timeouts
    // do not clear it: without this reset, the first delayed reply fails with 10035.
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(read_timeout))?;
    stream.set_write_timeout(Some(REQUEST_TIMEOUT))
}

/// Dispatch before constructing Tauri/WebView. Unsupported or malformed helper requests exit;
/// they must never fall through to an elevated desktop application.
pub fn run_elevation_helper_mode(arguments: impl IntoIterator<Item = OsString>) -> Option<i32> {
    let arguments = arguments.into_iter().collect::<Vec<_>>();
    if !arguments.get(1).is_some_and(|value| {
        value
            .to_string_lossy()
            .starts_with("--strawberrydisk-elevation-helper")
    }) {
        return None;
    }
    Some(
        match parse_bootstrap(&arguments)
            .and_then(|(port, token, parent, created)| serve(port, &token, parent, created))
        {
            Ok(()) => 0,
            Err(_) => 70,
        },
    )
}

fn parse_bootstrap(arguments: &[OsString]) -> io::Result<(u16, String, u32, u64)> {
    let invalid = || io::Error::from(io::ErrorKind::InvalidInput);
    if arguments.len() != 6 || arguments[1] != FLAG {
        return Err(invalid());
    }
    let port = arguments[2]
        .to_str()
        .and_then(|v| v.parse::<u16>().ok())
        .filter(|v| *v != 0)
        .ok_or_else(invalid)?;
    let token = arguments[3]
        .to_str()
        .filter(|v| v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or_else(invalid)?;
    let parent = arguments[4]
        .to_str()
        .and_then(|v| v.parse::<u32>().ok())
        .filter(|v| *v != 0)
        .ok_or_else(invalid)?;
    let created = arguments[5]
        .to_str()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v != 0)
        .ok_or_else(invalid)?;
    Ok((port, token.to_owned(), parent, created))
}

fn serve(port: u16, token: &str, parent_id: u32, parent_created: u64) -> io::Result<()> {
    if !is_elevated()? {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    // Retain the process object, not just a PID which Windows can recycle. The only
    // duplicated handles go to this original caller, with query/wait rights only.
    let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port))?;
    peer::verify_peer_process(&stream, parent_id)?;
    stream.set_write_timeout(Some(REQUEST_TIMEOUT))?;
    let parent = match parent::open_verified(parent_id, parent_created) {
        Ok(parent) => Arc::new(parent),
        Err(error) => {
            write_message(
                &mut stream,
                &Response::BootstrapFailed {
                    code: native_code(&error),
                },
            )?;
            // Keep the OS peer alive until the caller consumes this diagnostic and
            // disconnects; an immediate exit can erase its TCP ownership evidence.
            stream.set_read_timeout(Some(Duration::from_secs(2)))?;
            let _ = read_message::<Request>(&mut BufReader::new(&mut stream));
            return Err(error);
        }
    };
    // A client may stop responding while a worker or vendor process is still alive.
    // Bind the broker itself to the original process, independently of socket EOF.
    let lifetime_parent = Arc::clone(&parent);
    std::thread::Builder::new()
        .name("strawberrydisk-elevation-lifetime".into())
        .spawn(move || {
            unsafe { WaitForSingleObject(lifetime_parent.0, u32::MAX) };
            // This process only launches capabilities. Ending it cannot replay a
            // request or terminate a vendor process already handed to Windows.
            std::process::exit(0);
        })?;
    let mut stream = BufReader::new(stream);
    write_message(
        stream.get_mut(),
        &Response::Hello {
            protocol: PROTOCOL.to_owned(),
            token: token.to_owned(),
        },
    )?;
    // A disconnected/crashed parent closes the socket. Check the retained process
    // before every request as well, so a late connection cannot extend authorization.
    let mut last_request = 0;
    loop {
        let request: Request = match read_message(&mut stream) {
            Ok(request) => request,
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(error) => return Err(error),
        };
        if unsafe { WaitForSingleObject(parent.0, 0) } != WAIT_TIMEOUT {
            return Ok(());
        }
        let response = match request {
            Request::Ping => Response::Pong,
            Request::Launch {
                request_id,
                request,
            } => {
                if request_id <= last_request {
                    return Err(io::ErrorKind::InvalidData.into());
                }
                last_request = request_id;
                match launch_capability(request) {
                    Ok(Some(child)) => {
                        let mut handle = ptr::null_mut();
                        if unsafe {
                            DuplicateHandle(
                                GetCurrentProcess(),
                                child.handle(),
                                parent.0,
                                &mut handle,
                                PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE,
                                0,
                                0,
                            )
                        } == 0
                        {
                            // Launch already happened; close the transport instead of reporting
                            // a safe-to-retry rejection. The caller logs an uncertain outcome.
                            return Err(io::Error::last_os_error());
                        }
                        // If delivery fails, the limited handle is reclaimed at parent exit.
                        // Closing it remotely after a possibly delivered response could race
                        // the caller and close a recycled handle belonging to another task.
                        Response::Launched {
                            request_id,
                            handle: handle as usize as u64,
                            process_id: unsafe { GetProcessId(child.handle()) },
                        }
                    }
                    Ok(None) => return Err(io::Error::from_raw_os_error(ERROR_BROKEN_PIPE as i32)),
                    Err((code, reason)) => Response::Rejected {
                        request_id,
                        code,
                        reason,
                    },
                }
            }
        };
        write_message(stream.get_mut(), &response)?;
    }
}

enum LaunchedProcess {
    Helper(std::process::Child),
    Native(OwnedHandle),
}

impl LaunchedProcess {
    fn handle(&self) -> HANDLE {
        match self {
            Self::Helper(child) => child.as_raw_handle(),
            Self::Native(handle) => handle.0,
        }
    }
}

fn launch_capability(
    request: LaunchRequest,
) -> Result<Option<LaunchedProcess>, (u32, RejectionReason)> {
    if let LaunchRequest::Uninstall(request) = request {
        return crate::windows::launch_privileged_uninstaller(request)
            .map(|handle| handle.map(|handle| LaunchedProcess::Native(OwnedHandle(handle))))
            .map_err(|error| {
                (
                    error.native_code().unwrap_or(ERROR_INVALID_DATA),
                    if error == crate::ApplicationUninstallPlatformError::RegistrationChanged {
                        RejectionReason::ItemChanged
                    } else {
                        RejectionReason::Native
                    },
                )
            });
    }
    let message_directory = request.message_directory();
    let arguments = request
        .helper_arguments()
        .map_err(|code| (code, RejectionReason::Native))?;
    let mut command = Command::new(
        std::env::current_exe().map_err(|e| (native_code(&e), RejectionReason::Native))?,
    );
    crate::configure_background_process(&mut command);
    // Over-the-shoulder consent may run this broker as a different administrator.
    // These two helpers exchange nonce-bound files in the caller's temporary directory;
    // preserve that directory for this child only. Their domain validators still reject
    // wrong names, links, stale evidence and unsupported mutations.
    if let Some(directory) = message_directory {
        command.env("TEMP", &directory).env("TMP", directory);
    }
    command
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|child| Some(LaunchedProcess::Helper(child)))
        .map_err(|e| (native_code(&e), RejectionReason::Native))
}

fn authorize(port: u16, token: &str) -> Result<OwnedHandle, LaunchError> {
    let executable = helper_executable().map_err(before_io)?;
    let executable = wide(executable.as_os_str());
    let verb = wide(OsStr::new("runas"));
    let arguments = wide(OsStr::new(&format!(
        "{FLAG} {port} {token} {} {}",
        std::process::id(),
        process_created(unsafe { GetCurrentProcess() }).map_err(before_io)?
    )));
    let mut execution = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: verb.as_ptr(),
        lpFile: executable.as_ptr(),
        lpParameters: arguments.as_ptr(),
        nShow: SW_HIDE,
        ..Default::default()
    };
    if unsafe { ShellExecuteExW(&mut execution) } == 0 {
        return Err(LaunchError::before_start(unsafe { GetLastError() }));
    }
    if execution.hProcess.is_null() {
        return Err(LaunchError::before_start(ERROR_INVALID_DATA));
    }
    Ok(OwnedHandle(execution.hProcess))
}

fn helper_executable() -> io::Result<std::path::PathBuf> {
    #[cfg(test)]
    if let Some(path) = std::env::var_os("STRAWBERRYDISK_TEST_ELEVATION_HELPER_EXE") {
        return Ok(path.into());
    }
    std::env::current_exe()
}

fn is_elevated() -> io::Result<bool> {
    let mut token = ptr::null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let token = OwnedHandle(token);
    let mut elevation = TOKEN_ELEVATION::default();
    let mut bytes = 0;
    if unsafe {
        GetTokenInformation(
            token.0,
            TokenElevation,
            (&mut elevation as *mut TOKEN_ELEVATION).cast(),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut bytes,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(elevation.TokenIsElevated != 0)
}

fn secure_token() -> io::Result<String> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|_| io::Error::other("generate elevation session secret failed"))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn native_code(error: &io::Error) -> u32 {
    error
        .raw_os_error()
        .map_or(ERROR_BROKEN_PIPE, |code| code as u32)
}
fn before_io(error: io::Error) -> LaunchError {
    LaunchError::before_start(native_code(&error))
}
fn wide(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(Some(0)).collect()
}

fn process_created(handle: HANDLE) -> io::Result<u64> {
    let (mut created, mut exited, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    if unsafe { GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok((u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delayed_replies_on_a_polling_listener_do_not_fail_as_would_block() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        let server = loop {
            match listener.accept() {
                Ok((server, _)) => break server,
                Err(error)
                    if error.kind() == io::ErrorKind::WouldBlock && Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("the local test client should be accepted: {error}"),
            }
        };
        configure_client_stream(&server, Duration::from_secs(2)).unwrap();
        let writer = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            write_message(&mut client, &Response::Pong).unwrap();
        });
        assert!(matches!(
            read_message(&mut BufReader::new(server)),
            Ok(Response::Pong)
        ));
        writer.join().unwrap();
    }

    #[test]
    fn bootstrap_rejects_old_versions_extra_arguments_and_missing_process_identity() {
        assert_eq!(run_elevation_helper_mode(["StrawberryDisk".into()]), None);
        for arguments in [
            vec![
                "StrawberryDisk".into(),
                "--strawberrydisk-elevation-helper-v0".into(),
            ],
            vec![
                "StrawberryDisk".into(),
                FLAG.into(),
                "1234".into(),
                "a".repeat(64).into(),
                "123".into(),
            ],
            vec![
                "StrawberryDisk".into(),
                FLAG.into(),
                "1234".into(),
                "a".repeat(64).into(),
                "123".into(),
                "0".into(),
            ],
        ] {
            assert_eq!(run_elevation_helper_mode(arguments), Some(70));
        }
    }

    #[test]
    fn cancellation_and_uncertain_dispatch_preserve_mutation_semantics() {
        let cancelled = LaunchError::before_start(ERROR_CANCELLED).into_platform();
        assert_eq!(cancelled.code(), PlatformErrorCode::UserCancelled);
        assert_eq!(
            cancelled.mutation_state(),
            crate::PlatformMutationState::NotAttempted
        );
        let uncertain = LaunchError {
            code: ERROR_BROKEN_PIPE,
            may_have_started: true,
            reason: RejectionReason::Native,
        }
        .into_platform();
        assert_eq!(
            uncertain.mutation_state(),
            crate::PlatformMutationState::MayHaveChanged
        );
        let changed = LaunchError {
            code: ERROR_INVALID_DATA,
            may_have_started: false,
            reason: RejectionReason::ItemChanged,
        }
        .into_platform();
        assert_eq!(changed.code(), PlatformErrorCode::ItemChanged);
    }

    #[test]
    #[ignore = "requires a built StrawberryDisk executable and interactive Windows consent"]
    fn actual_disk_cleanup_estimates_reuse_one_authorization() {
        assert!(std::env::var_os("STRAWBERRYDISK_TEST_ELEVATION_HELPER_EXE").is_some());
        let first = crate::disk_cleanup_helper::estimate_previous_installations_with_privileges()
            .expect("the actual Windows estimate helper must return a result");
        let first_label = SESSION
            .get()
            .unwrap()
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .label
            .clone();
        let second = crate::disk_cleanup_helper::estimate_previous_installations_with_privileges()
            .expect("the second estimate must reuse the authorized broker");
        let slot = SESSION.get().unwrap().lock().unwrap();
        let session = slot.as_ref().unwrap();
        assert_eq!(session.label, first_label);
        assert_eq!(session.next_request, 3);
        // Estimation never requests deletion, even when a real Windows.old exists.
        println!(
            "first_availability={:?} second_availability={:?} shared_session=true",
            first.availability, second.availability
        );
    }
}

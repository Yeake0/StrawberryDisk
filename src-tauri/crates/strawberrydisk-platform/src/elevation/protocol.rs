use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    ffi::OsString,
    io::{self, BufRead, Write},
    path::PathBuf,
};
use windows_sys::Win32::Foundation::ERROR_INVALID_PARAMETER;

pub(super) const PROTOCOL: &str = "mangodisk-elevation-v1";
const MAX_MESSAGE_BYTES: usize = 64 * 1024;

/// Capability arguments are data, never an executable/command line selected by a client.
/// Native uninstall evidence is resolved again by that domain inside the elevated process.
#[derive(Serialize, Deserialize)]
#[serde(tag = "capability", deny_unknown_fields, rename_all = "camelCase")]
pub(crate) enum LaunchRequest {
    Maintenance {
        port: u16,
        token: String,
    },
    Settings {
        request: PathBuf,
        response: PathBuf,
        digest: String,
    },
    Startup {
        request: PathBuf,
        response: PathBuf,
    },
    DiskCleanup {
        execute: bool,
        port: u16,
        token: String,
        parent_pid: u32,
    },
    RegistrationRemoval {
        id: String,
        digest: String,
    },
    Uninstall(UninstallRequest),
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields, rename_all = "camelCase")]
pub(crate) enum UninstallRequest {
    Msi {
        product_code: String,
    },
    Registered {
        key_name: String,
        registry32: bool,
        digest: String,
    },
    Chocolatey {
        package_name: String,
        install_root: PathBuf,
        marker_digest: String,
        executable_digest: String,
    },
}

impl LaunchRequest {
    pub(super) fn capability(&self) -> &'static str {
        match self {
            Self::Maintenance { .. } => "maintenance",
            Self::Settings { .. } => "system_settings",
            Self::Startup { .. } => "startup",
            Self::DiskCleanup { .. } => "disk_cleanup",
            Self::RegistrationRemoval { .. } => "registration_removal",
            Self::Uninstall(_) => "application_uninstall",
        }
    }

    pub(super) fn message_directory(&self) -> Option<PathBuf> {
        match self {
            Self::Settings { request, .. } | Self::Startup { request, .. } => {
                request.parent().map(ToOwned::to_owned)
            }
            _ => None,
        }
    }

    pub(super) fn helper_arguments(self) -> Result<Vec<OsString>, u32> {
        let arguments: Vec<OsString> = match self {
            Self::Maintenance { port, token } if port != 0 && valid_digest(&token) => vec![
                crate::system_maintenance_helper::HELPER_FLAG.into(),
                port.to_string().into(),
                token.into(),
            ],
            Self::Settings {
                request,
                response,
                digest,
            } if valid_paths(&request, &response) && valid_digest(&digest) => vec![
                crate::system_settings_helper::HELPER_FLAG.into(),
                request.into(),
                response.into(),
                digest.into(),
            ],
            Self::Startup { request, response } if valid_paths(&request, &response) => vec![
                crate::startup_helper::HELPER_FLAG.into(),
                request.into(),
                response.into(),
            ],
            Self::DiskCleanup {
                execute,
                port,
                token,
                parent_pid,
            } if port != 0 && valid_digest(&token) && parent_pid != 0 => vec![
                crate::disk_cleanup_helper::HELPER_FLAG.into(),
                if execute { "execute" } else { "estimate" }.into(),
                port.to_string().into(),
                token.into(),
                parent_pid.to_string().into(),
            ],
            Self::RegistrationRemoval { id, digest }
                if id
                    .strip_prefix("application-")
                    .is_some_and(|v| v.len() == 24 && v.bytes().all(|b| b.is_ascii_hexdigit()))
                    && valid_digest(&digest) =>
            {
                vec![
                    crate::windows::APPLICATION_RECORD_HELPER_FLAG.into(),
                    id.into(),
                    digest.into(),
                ]
            }
            _ => return Err(ERROR_INVALID_PARAMETER),
        };
        Ok(arguments)
    }
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
fn valid_paths(request: &std::path::Path, response: &std::path::Path) -> bool {
    request.is_absolute()
        && response.is_absolute()
        && request != response
        && request.parent() == response.parent()
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields, rename_all = "camelCase")]
pub(super) enum Request {
    Ping,
    Launch {
        request_id: u64,
        request: LaunchRequest,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum RejectionReason {
    Native,
    ItemChanged,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields, rename_all = "camelCase")]
pub(super) enum Response {
    BootstrapFailed {
        code: u32,
    },
    Hello {
        protocol: String,
        token: String,
    },
    Pong,
    Launched {
        request_id: u64,
        handle: u64,
        process_id: u32,
    },
    Rejected {
        request_id: u64,
        code: u32,
        reason: RejectionReason,
    },
}

pub(super) fn write_message<T: Serialize>(writer: &mut impl Write, message: &T) -> io::Result<()> {
    let mut bytes = serde_json::to_vec(message)?;
    if bytes.len() >= MAX_MESSAGE_BYTES {
        return Err(io::ErrorKind::InvalidData.into());
    }
    bytes.push(b'\n');
    writer.write_all(&bytes)?;
    writer.flush()
}

/// Bounded framing also rejects EOF halfway through a message. A partially written launch
/// must never be interpreted as a valid request or cause unbounded allocation in the helper.
pub(super) fn read_message<T: DeserializeOwned>(reader: &mut impl BufRead) -> io::Result<T> {
    let mut bytes = Vec::new();
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        let length = available
            .iter()
            .position(|b| *b == b'\n')
            .map_or(available.len(), |index| index + 1);
        if bytes.len() + length > MAX_MESSAGE_BYTES {
            return Err(io::ErrorKind::InvalidData.into());
        }
        bytes.extend_from_slice(&available[..length]);
        reader.consume(length);
        if bytes.last() == Some(&b'\n') {
            return serde_json::from_slice(&bytes).map_err(Into::into);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_capability_round_trips_with_a_fixed_schema() {
        let requests = [
            LaunchRequest::Maintenance {
                port: 1234,
                token: "a".repeat(64),
            },
            LaunchRequest::Settings {
                request: r"C:\Temp\request.json".into(),
                response: r"C:\Temp\response.json".into(),
                digest: "a".repeat(64),
            },
            LaunchRequest::Startup {
                request: r"C:\Temp\request.json".into(),
                response: r"C:\Temp\response.json".into(),
            },
            LaunchRequest::DiskCleanup {
                execute: false,
                port: 1234,
                token: "a".repeat(64),
                parent_pid: 123,
            },
            LaunchRequest::RegistrationRemoval {
                id: format!("application-{}", "a".repeat(24)),
                digest: "b".repeat(64),
            },
            LaunchRequest::Uninstall(UninstallRequest::Msi {
                product_code: "{9627E855-337D-45EC-A2D9-CBB92B447399}".into(),
            }),
            LaunchRequest::Uninstall(UninstallRequest::Registered {
                key_name: "fixture".into(),
                registry32: false,
                digest: "a".repeat(64),
            }),
        ];
        for request in requests {
            let encoded = serde_json::to_vec(&request).unwrap();
            let decoded: LaunchRequest = serde_json::from_slice(&encoded).unwrap();
            assert_eq!(serde_json::to_vec(&decoded).unwrap(), encoded);
        }
    }

    #[test]
    fn protocol_rejects_arbitrary_commands_and_unknown_fields() {
        for json in [
            r#"{"capability":"command","executable":"cmd.exe"}"#,
            r#"{"capability":"maintenance","port":1234,"token":"a","command":"whoami"}"#,
            r#"{"capability":"uninstall","kind":"registered","key_name":"test","registry32":false,"digest":"a","scope":"currentUser"}"#,
        ] {
            assert!(serde_json::from_str::<LaunchRequest>(json).is_err());
        }
    }

    #[test]
    fn helper_launch_rejects_relative_paths_and_invalid_tokens() {
        assert!(LaunchRequest::Startup {
            request: "request.json".into(),
            response: "response.json".into()
        }
        .helper_arguments()
        .is_err());
        assert!(LaunchRequest::Maintenance {
            port: 0,
            token: "a".repeat(64)
        }
        .helper_arguments()
        .is_err());
        assert!(LaunchRequest::Maintenance {
            port: 1234,
            token: "a;whoami".into()
        }
        .helper_arguments()
        .is_err());
    }

    #[test]
    fn framing_rejects_oversized_and_truncated_messages() {
        assert!(read_message::<Request>(&mut &vec![b'x'; MAX_MESSAGE_BYTES + 1][..]).is_err());
        assert!(read_message::<Request>(&mut &b"{\"type\":\"ping\"}"[..]).is_err());
        assert!(matches!(
            read_message::<Request>(&mut &b"{\"type\":\"ping\"}\n"[..]),
            Ok(Request::Ping)
        ));
    }
}

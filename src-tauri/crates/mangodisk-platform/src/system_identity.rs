//! Basic operating-system identity without collecting host or user identifiers.

use sysinfo::System;

pub struct SystemIdentity {
    pub operating_system: &'static str,
    pub version: Option<String>,
    pub architecture: String,
}

pub fn current() -> SystemIdentity {
    SystemIdentity {
        operating_system: match std::env::consts::OS {
            "macos" => "macOS",
            "windows" => "Windows",
            "linux" => "Linux",
            other => other,
        },
        version: System::os_version(),
        architecture: System::cpu_arch(),
    }
}

/// Product version and kernel/build distinguish systems sharing a WebView release.
pub fn log_current() {
    let identity = current();
    let kernel_version = System::kernel_version();
    let system_name = System::name();
    log::info!(
        "system_environment operating_system={} system_name={} os_version={} kernel_version={} architecture={}",
        crate::diagnostics::text(identity.operating_system),
        crate::diagnostics::text(system_name.as_deref().unwrap_or("unknown")),
        crate::diagnostics::text(identity.version.as_deref().unwrap_or("unknown")),
        crate::diagnostics::text(kernel_version.as_deref().unwrap_or("unknown")),
        crate::diagnostics::text(&identity.architecture)
    );
}

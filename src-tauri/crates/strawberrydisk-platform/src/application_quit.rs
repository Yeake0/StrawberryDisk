//! Application-level quit requests; never escalate to process termination.
use std::path::Path;

use crate::PlatformResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplicationQuitOutcome {
    Requested,
    Unavailable,
    Unsupported,
}

#[cfg(target_os = "macos")]
pub fn request(path: &Path) -> PlatformResult<ApplicationQuitOutcome> {
    use objc2::rc::autoreleasepool;
    use objc2_app_kit::{NSApplicationActivationPolicy, NSWorkspace};

    autoreleasepool(|_| {
        let mut matched = false;
        let mut requested = false;
        let target = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let applications = NSWorkspace::sharedWorkspace().runningApplications();
        for index in 0..applications.count() {
            let application = applications.objectAtIndex(index);
            let Some(bundle) = application.bundleURL().and_then(|url| url.path()) else {
                continue;
            };
            let bundle = std::path::PathBuf::from(bundle.to_string());
            let bundle = bundle.canonicalize().unwrap_or(bundle);
            if bundle != target || application.isTerminated() {
                continue;
            }
            matched = true;
            // Match the application bundle exactly, not its embedded helpers.
            // A normal quit must preserve the application's save/cancel workflow.
            if application.processIdentifier() == std::process::id() as i32
                || application.activationPolicy() != NSApplicationActivationPolicy::Regular
            {
                continue;
            }
            requested |= application.terminate();
        }
        Ok(outcome(matched, requested))
    })
}

#[cfg(windows)]
pub fn request(path: &Path) -> PlatformResult<ApplicationQuitOutcome> {
    use crate::{
        current_platform, ApplicationProcessCloseMode, ApplicationProcessTarget, Platform,
    };

    // The shared Windows adapter rechecks image identity, excludes this process,
    // and sends WM_CLOSE. Its graceful mode never falls back to TerminateProcess.
    let result = current_platform().close_application_processes(
        &ApplicationProcessTarget {
            executable_names: Vec::new(),
            executable_paths: vec![path.to_path_buf()],
        },
        ApplicationProcessCloseMode::Graceful,
    )?;
    Ok(outcome(
        result.matched_process_count > 0,
        result.requested_process_count > 0,
    ))
}

#[cfg(not(any(target_os = "macos", windows)))]
pub fn request(_path: &Path) -> PlatformResult<ApplicationQuitOutcome> {
    Ok(ApplicationQuitOutcome::Unsupported)
}

#[cfg(any(target_os = "macos", windows, test))]
fn outcome(matched: bool, requested: bool) -> ApplicationQuitOutcome {
    if requested {
        ApplicationQuitOutcome::Requested
    } else if matched {
        ApplicationQuitOutcome::Unsupported
    } else {
        ApplicationQuitOutcome::Unavailable
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quit_request_is_not_reported_as_completed_termination() {
        assert_eq!(outcome(true, true), ApplicationQuitOutcome::Requested);
        assert_eq!(outcome(true, false), ApplicationQuitOutcome::Unsupported);
        assert_eq!(outcome(false, false), ApplicationQuitOutcome::Unavailable);
    }
}

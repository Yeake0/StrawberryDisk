//! Preserve failure stages and correlatable diagnostics with readable native causes.
use std::{error::Error, fmt};

#[derive(Debug)]
pub struct Failure {
    stage: &'static str,
}
impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "resident operation failed at {}", self.stage)
    }
}
impl Error for Failure {}

impl Failure {
    pub fn record(stage: &'static str, error: &(dyn Error + 'static)) -> Self {
        let io = error.downcast_ref::<std::io::Error>().or_else(|| {
            error
                .downcast_ref::<tauri_plugin_store::Error>()
                .and_then(|error| {
                    if let tauri_plugin_store::Error::Io(io) = error {
                        Some(io)
                    } else {
                        None
                    }
                })
        });
        let diagnostic = strawberrydisk_platform::diagnostics::text(error);
        log::warn!(
            "resident_operation_failed stage={stage} io_kind={:?} os_code={:?} error={}",
            io.map(std::io::Error::kind),
            io.and_then(std::io::Error::raw_os_error),
            diagnostic
        );
        Self { stage }
    }
    pub fn state(stage: &'static str) -> Self {
        log::warn!("resident_operation_failed stage={stage} reason=invalid_state");
        Self { stage }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transport_diagnostics_do_not_include_private_native_content() {
        let error = Failure::record(
            "preferences_save",
            &std::io::Error::other("/private/user/settings"),
        );
        assert_eq!(
            error.to_string(),
            "resident operation failed at preferences_save"
        );
    }
}

use strawberrydisk_core::{DirectorySelectionOutcome, DirectorySelectionService};

use super::error::{run_blocking_value, CommandError, CommandResult};

const OPERATION: &str = "filter_directory_paths";
const MAX_DIRECTORY_ENTRIES_PER_REQUEST: usize = 64;

fn validate_request_count(count: usize) -> CommandResult<()> {
    if count <= MAX_DIRECTORY_ENTRIES_PER_REQUEST {
        return Ok(());
    }
    log::info!(
        "directory_entries_rejected reason=request_limit_exceeded requested_count={count} maximum_count={MAX_DIRECTORY_ENTRIES_PER_REQUEST}"
    );
    Err(CommandError::invalid_input(OPERATION))
}

#[tauri::command]
pub async fn filter_directory_paths(
    paths: Vec<String>,
) -> CommandResult<DirectorySelectionOutcome> {
    validate_request_count(paths.len())?;
    run_blocking_value(OPERATION, move || {
        let started = std::time::Instant::now();
        let requested_count = paths.len();
        let outcome = DirectorySelectionService::resolve(paths);
        log::info!(
            "directory_entries_resolved requested_count={} directory_count={} rejected_count={} redirected_count={} rejection_reasons={:?} error_details={:?} elapsed_ms={}",
            requested_count,
            outcome.directories.len(),
            outcome.rejected_count,
            outcome.redirected_count,
            outcome.rejection_reasons(),
            outcome.error_details(),
            started.elapsed().as_millis()
        );
        outcome
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directory_request_limit_rejects_oversized_batches() {
        assert!(validate_request_count(MAX_DIRECTORY_ENTRIES_PER_REQUEST).is_ok());
        let error = validate_request_count(MAX_DIRECTORY_ENTRIES_PER_REQUEST + 1).unwrap_err();
        let value = serde_json::to_value(error).unwrap();
        assert_eq!(value["code"], "invalidInput");
        assert_eq!(value["retryable"], false);
    }
}

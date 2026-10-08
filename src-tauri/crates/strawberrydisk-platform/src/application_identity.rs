use std::path::Path;

use crate::ApplicationIdentityMetadata;

/// Call only with a catalog-resolved bundle or executable, never an uninstall command or user path.
pub fn read_application_identity(
    path: &Path,
    expected_bundle_identifier: Option<&str>,
) -> Option<ApplicationIdentityMetadata> {
    #[cfg(target_os = "macos")]
    return crate::macos::application_identity::read(path, expected_bundle_identifier?);
    #[cfg(windows)]
    {
        let _ = expected_bundle_identifier;
        if !path.is_absolute() || path.to_string_lossy().starts_with(r"\\") {
            return None;
        }
        let metadata = crate::windows::file_version::file_version_metadata(path)?;
        let bounded = |value: Option<String>| {
            value.filter(|value| !value.trim().is_empty() && value.len() <= 1024)
        };
        Some(ApplicationIdentityMetadata::Windows {
            product_name: bounded(metadata.product_name),
            file_description: bounded(metadata.description),
            company_name: bounded(metadata.company_name),
            package_identity: None,
        })
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = (path, expected_bundle_identifier);
        None
    }
}

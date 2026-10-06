use std::{path::Path, time::Instant};

use strawberrydisk_platform::{read_application_identity, ApplicationIdentityMetadata};

use super::models::{
    ApplicationUninstallIdentity, ApplicationUninstallInventorySource,
    APPLICATION_UNINSTALL_IDENTITY_SCHEMA_VERSION,
};
use super::{
    ApplicationUninstallCandidate, ApplicationUninstallPlatform, ApplicationUninstallService,
};

impl ApplicationUninstallService {
    /// Only trusted catalog identities select native metadata. This does not inspect user data or authorize uninstall.
    pub fn describe_identity(
        candidate: &ApplicationUninstallCandidate,
    ) -> ApplicationUninstallIdentity {
        let started = Instant::now();
        let metadata = match candidate.platform {
            ApplicationUninstallPlatform::MacosBundle => {
                candidate.application_path.as_deref().and_then(|path| {
                    read_application_identity(Path::new(path), Some(&candidate.primary_identifier))
                })
            }
            ApplicationUninstallPlatform::WindowsRegistry => {
                // Registry executable hints can come from DisplayIcon and may name an uninstaller.
                // Only package manifests supply a main executable with known identity provenance.
                let package = candidate
                    .source_identities
                    .iter()
                    .find(|source| {
                        source.source == ApplicationUninstallInventorySource::WindowsAppx
                    })
                    .filter(|source| {
                        !source.identifier.trim().is_empty() && source.identifier.len() <= 1024
                    });
                #[cfg(windows)]
                let native = package
                    .and(candidate.package_executable_path.as_deref())
                    .and_then(|path| read_application_identity(path, None));
                #[cfg(not(windows))]
                let native: Option<ApplicationIdentityMetadata> = None;
                let package_identity = package.map(|source| source.identifier.clone());
                match native {
                    Some(ApplicationIdentityMetadata::Windows {
                        product_name,
                        file_description,
                        company_name,
                        ..
                    }) => Some(ApplicationIdentityMetadata::Windows {
                        product_name,
                        file_description,
                        company_name,
                        package_identity,
                    }),
                    _ => Some(ApplicationIdentityMetadata::Windows {
                        product_name: None,
                        file_description: None,
                        company_name: None,
                        package_identity,
                    }),
                }
            }
            ApplicationUninstallPlatform::LinuxPackage => None,
        };
        log::info!("application_identity_described application_id={} application_name={} metadata_available={} elapsed_ms={}", candidate.application_id, strawberrydisk_platform::diagnostics::text(&candidate.name), metadata.is_some(), started.elapsed().as_millis());
        ApplicationUninstallIdentity {
            schema_version: APPLICATION_UNINSTALL_IDENTITY_SCHEMA_VERSION,
            metadata,
        }
    }
}

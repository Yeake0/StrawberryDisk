use std::path::Path;

use strawberrydisk_platform::{
    PlatformStartupArtifact, PlatformStartupControlCapability, PlatformStartupDiagnosticCode,
    PlatformStartupSourceKind,
};

/// Returns whether the provider can delete this exact startup configuration without deleting the
/// target application. Providers still re-read the item and enforce their source boundary before
/// every mutation.
pub(super) fn supports_removal(artifact: &PlatformStartupArtifact) -> bool {
    match artifact.source_kind {
        PlatformStartupSourceKind::LaunchAgent | PlatformStartupSourceKind::LaunchDaemon => {
            has_extension(artifact, "plist")
                && matches!(
                    artifact.control_capability,
                    PlatformStartupControlCapability::Toggleable
                        | PlatformStartupControlCapability::ElevationRequired
                )
        }
        PlatformStartupSourceKind::RegistryRun => matches!(
            artifact.control_capability,
            PlatformStartupControlCapability::Toggleable
                | PlatformStartupControlCapability::ElevationRequired
                | PlatformStartupControlCapability::RemoveOnly
        ),
        PlatformStartupSourceKind::StartupFolder => {
            has_extension(artifact, "lnk")
                && matches!(
                    artifact.control_capability,
                    PlatformStartupControlCapability::Toggleable
                        | PlatformStartupControlCapability::ElevationRequired
                )
        }
        PlatformStartupSourceKind::ScheduledTask => matches!(
            artifact.control_capability,
            PlatformStartupControlCapability::Toggleable
                | PlatformStartupControlCapability::ElevationRequired
        ),
        PlatformStartupSourceKind::BackgroundTask => {
            artifact.control_capability == PlatformStartupControlCapability::RemoveOnly
                && artifact
                    .diagnostics
                    .contains(&PlatformStartupDiagnosticCode::MissingTarget)
        }
        PlatformStartupSourceKind::AdvancedAutoRun => {
            artifact.control_capability == PlatformStartupControlCapability::RemoveOnly
        }
        _ => false,
    }
}

/// Returns whether a removable startup configuration points to a missing target. This narrower
/// signal powers the leftover filter and bulk cleanup; it does not limit an explicit row deletion.
pub(super) fn is_removable_orphan(artifact: &PlatformStartupArtifact) -> bool {
    if !artifact
        .diagnostics
        .contains(&PlatformStartupDiagnosticCode::MissingTarget)
    {
        return false;
    }
    supports_removal(artifact)
}

fn has_extension(artifact: &PlatformStartupArtifact, expected: &str) -> bool {
    artifact
        .configuration_path
        .as_deref()
        .and_then(Path::extension)
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case(expected))
}

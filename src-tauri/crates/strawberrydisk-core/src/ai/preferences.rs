use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{configuration_file, AiError};

/// The feature preference is independent of provider credentials and consent.
/// Deleting or repairing a provider configuration must never re-enable AI.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AiPreferences {
    pub schema_version: u8,
    pub enabled: bool,
}

impl Default for AiPreferences {
    fn default() -> Self {
        Self {
            schema_version: 1,
            enabled: true,
        }
    }
}

impl AiPreferences {
    pub fn load() -> Result<Self, AiError> {
        Self::read_from(&path()?)
    }

    pub fn save(enabled: bool) -> Result<Self, AiError> {
        Self::write_to(&path()?, enabled)
    }

    fn read_from(path: &Path) -> Result<Self, AiError> {
        // Only an absent document inherits the previous release's enabled
        // behavior. Invalid or unreadable preferences must fail closed.
        let Some(raw) = configuration_file::read_from(path)? else {
            return Ok(Self::default());
        };
        let value: Self = serde_json::from_str(&raw).map_err(|_| AiError::InvalidConfiguration)?;
        if value.schema_version != 1 {
            return Err(AiError::InvalidConfiguration);
        }
        Ok(value)
    }

    fn write_to(path: &Path, enabled: bool) -> Result<Self, AiError> {
        let value = Self {
            enabled,
            ..Self::default()
        };
        let raw = serde_json::to_string(&value).map_err(|_| AiError::InvalidConfiguration)?;
        configuration_file::write_to(path, &raw)?;
        Ok(value)
    }
}

fn path() -> Result<PathBuf, AiError> {
    crate::shared::application_paths()
        .map(|paths| paths.data_directory().join("ai-preferences.json"))
        .map_err(|_| AiError::ConfigurationUnavailable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_preferences_preserve_existing_behavior_and_round_trip_independently() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ai-preferences.json");
        let provider = directory.path().join("ai.json");
        std::fs::write(&provider, "provider-fixture").unwrap();
        assert!(AiPreferences::read_from(&path).unwrap().enabled);
        AiPreferences::write_to(&path, false).unwrap();
        assert!(!AiPreferences::read_from(&path).unwrap().enabled);
        assert_eq!(
            std::fs::read_to_string(&provider).unwrap(),
            "provider-fixture"
        );
        std::fs::remove_file(provider).unwrap();
        assert!(!AiPreferences::read_from(&path).unwrap().enabled);
        let enabled = AiPreferences::write_to(&path, true).unwrap();
        assert_eq!(
            serde_json::to_value(enabled).unwrap(),
            serde_json::json!({"schemaVersion": 1, "enabled": true})
        );
    }

    #[test]
    fn invalid_preferences_never_fall_back_to_enabled() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ai-preferences.json");
        for raw in [
            "{broken",
            r#"{"schemaVersion":2,"enabled":false}"#,
            r#"{"schemaVersion":1}"#,
            r#"{"schemaVersion":1,"enabled":"false"}"#,
        ] {
            std::fs::write(&path, raw).unwrap();
            assert_eq!(
                AiPreferences::read_from(&path),
                Err(AiError::InvalidConfiguration)
            );
            assert_eq!(std::fs::read_to_string(&path).unwrap(), raw);
        }
        assert_eq!(
            AiPreferences::read_from(directory.path()),
            Err(AiError::ConfigurationUnavailable)
        );
    }
}

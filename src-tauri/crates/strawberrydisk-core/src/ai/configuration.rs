use std::collections::HashSet;

use reqwest::header::{HeaderName, HeaderValue};
use reqwest::Url;
use serde::{Deserialize, Serialize};

use super::{configuration_file, AiError};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AiServiceMode {
    Free,
    Local,
    // Missing fields belong to schema 1 custom configurations, never opt users into a new recipient.
    #[default]
    Custom,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ReasoningMode {
    #[default]
    Default,
    Disabled,
}

#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AiCustomHeader {
    pub name: String,
    pub value: String,
}

// Never derive Debug: this structure contains the user's API key.
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AiConfiguration {
    pub schema_version: u8,
    #[serde(default)]
    pub mode: AiServiceMode,
    // Retained for schema 2 compatibility; explanation requests no longer require this flag.
    #[serde(default)]
    pub free_consent: bool,
    pub endpoint: String,
    pub model: String,
    pub api_key: String,
    pub reasoning: ReasoningMode,
    /// Missing optional controls retain provider defaults in schema 1/2 files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom_headers: Vec<AiCustomHeader>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AiConfigurationUpdate {
    pub mode: AiServiceMode,
    pub free_consent: bool,
    pub endpoint: String,
    pub model: String,
    /// None retains the stored key; the editor sends the complete custom configuration.
    pub api_key: Option<String>,
    pub reasoning: ReasoningMode,
    #[serde(default)]
    pub temperature: Option<f64>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub custom_headers: Option<Vec<AiCustomHeader>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSettings {
    pub schema_version: u8,
    pub mode: AiServiceMode,
    pub free_consent: bool,
    pub free_available: bool,
    pub endpoint: String,
    pub model: String,
    pub has_key: bool,
    pub reasoning: ReasoningMode,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
}

impl AiConfiguration {
    pub fn initial() -> Self {
        Self {
            schema_version: 2,
            mode: AiServiceMode::Local,
            free_consent: false,
            endpoint: String::new(),
            model: String::new(),
            api_key: String::new(),
            reasoning: ReasoningMode::Default,
            temperature: None,
            max_tokens: None,
            custom_headers: Vec::new(),
        }
    }

    pub fn load() -> Result<Option<Self>, AiError> {
        configuration_file::read()?
            .map(|raw| {
                let mut value: Self =
                    serde_json::from_str(&raw).map_err(|_| AiError::InvalidConfiguration)?;
                value.validate()?;
                // Legacy free settings now select local inference without
                // discarding the saved custom-provider configuration.
                if value.mode == AiServiceMode::Free {
                    value.mode = AiServiceMode::Local;
                }
                value.schema_version = 2;
                Ok(value)
            })
            .transpose()
    }

    pub fn save(update: AiConfigurationUpdate) -> Result<AiSettings, AiError> {
        let value = if matches!(update.mode, AiServiceMode::Free | AiServiceMode::Local) {
            // Switching to local mode must not validate or persist an unfinished
            // custom draft. Preserve the last saved settings, or initial defaults.
            let mut stored = Self::load()?.unwrap_or_else(Self::initial);
            stored.mode = AiServiceMode::Local;
            stored.free_consent = update.free_consent;
            stored
        } else {
            let previous = if update.api_key.is_none() || update.custom_headers.is_none() {
                Self::load()?.unwrap_or_else(Self::initial)
            } else {
                Self::initial()
            };
            Self {
                schema_version: 2,
                mode: update.mode,
                free_consent: update.free_consent,
                endpoint: normalize_endpoint(&update.endpoint)?,
                model: update.model.trim().to_owned(),
                api_key: update
                    .api_key
                    .map_or(previous.api_key, |key| key.trim().to_owned()),
                reasoning: update.reasoning,
                temperature: update.temperature,
                max_tokens: update.max_tokens,
                custom_headers: update.custom_headers.unwrap_or(previous.custom_headers),
            }
        };
        value.validate()?;
        let raw =
            serde_json::to_string_pretty(&value).map_err(|_| AiError::InvalidConfiguration)?;
        // Atomically replace the document so all custom fields are saved together.
        configuration_file::write(&raw)?;
        Ok(value.settings())
    }

    pub fn delete() -> Result<(), AiError> {
        configuration_file::delete()
    }

    pub fn settings(&self) -> AiSettings {
        AiSettings {
            schema_version: 2,
            mode: self.mode,
            free_consent: self.free_consent,
            free_available: false,
            endpoint: self.endpoint.clone(),
            model: self.model.clone(),
            has_key: !self.api_key.is_empty(),
            reasoning: self.reasoning,
            temperature: self.temperature,
            max_tokens: self.max_tokens,
        }
    }

    pub fn validate(&self) -> Result<(), AiError> {
        if !matches!(self.schema_version, 1 | 2)
            || self
                .temperature
                .is_some_and(|value| !value.is_finite() || !(0.0..=2.0).contains(&value))
            || self.max_tokens == Some(0)
            || !valid_custom_headers(&self.custom_headers)
            || self.model.len() > 128
            || self.model.chars().any(char::is_control)
            || self.api_key.len() > 512
            || self
                .api_key
                .chars()
                .any(|c| c.is_control() || !c.is_ascii())
            || (!self.endpoint.is_empty() && normalize_endpoint(&self.endpoint)? != self.endpoint)
        {
            return Err(AiError::InvalidConfiguration);
        }
        if matches!(self.mode, AiServiceMode::Free | AiServiceMode::Local) {
            return Ok(());
        }
        if self.model.is_empty() || self.endpoint.is_empty() {
            return Err(AiError::InvalidConfiguration);
        }
        let url = Url::parse(&self.endpoint).map_err(|_| AiError::InvalidConfiguration)?;
        if self.api_key.is_empty() && self.custom_headers.is_empty() && !is_loopback(&url) {
            return Err(AiError::InvalidConfiguration);
        }
        Ok(())
    }
}

fn valid_custom_headers(headers: &[AiCustomHeader]) -> bool {
    if headers.len() > 8 {
        return false;
    }
    let mut names = HashSet::new();
    for header in headers {
        // UUIDs expand from 8 to 36 bytes; Unix seconds need at most 20 digits
        // for u64. Bound the expanded value before persisting it.
        let maximum_expanded_bytes = header.value.len()
            + header.value.matches("{{uuid}}").count() * 28
            + header.value.matches("{{timestamp}}").count() * 7;
        if header.name.is_empty()
            || header.name.len() > 64
            || header.value.is_empty()
            || header.value.len() > 1024
            || maximum_expanded_bytes > 1024
            || !header
                .value
                .bytes()
                .all(|byte| (0x20..=0x7e).contains(&byte))
        {
            return false;
        }
        let Ok(name) = HeaderName::from_bytes(header.name.as_bytes()) else {
            return false;
        };
        if matches!(
            name.as_str(),
            "host"
                | "content-type"
                | "content-length"
                | "transfer-encoding"
                | "connection"
                | "upgrade"
                | "proxy-authorization"
                | "x-request-id"
        ) || !names.insert(name)
            || HeaderValue::from_str(&header.value).is_err()
        {
            return false;
        }
    }
    true
}

fn is_loopback(url: &Url) -> bool {
    matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))
}

pub(super) fn normalize_endpoint(input: &str) -> Result<String, AiError> {
    let trimmed = input.trim().trim_end_matches('/');
    let url = Url::parse(trimmed).map_err(|_| AiError::InvalidConfiguration)?;
    if trimmed.len() > 512
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(url.scheme(), "http" | "https")
        || url.path().ends_with("/chat/completions")
    {
        return Err(AiError::InvalidConfiguration);
    }
    Ok(url.as_str().trim_end_matches('/').to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advanced_controls_are_optional_and_validated() {
        let mut config = AiConfiguration::initial();
        for temperature in [-0.1, 2.1, f64::NAN, f64::INFINITY] {
            config.temperature = Some(temperature);
            assert!(config.validate().is_err());
        }
        for temperature in [0.0, 0.7, 2.0] {
            config.temperature = Some(temperature);
            assert!(config.validate().is_ok());
        }
        config.max_tokens = Some(0);
        assert!(config.validate().is_err());
        config.max_tokens = Some(4096);
        let roundtrip: AiConfiguration =
            serde_json::from_value(serde_json::to_value(&config).unwrap()).unwrap();
        assert_eq!(roundtrip.temperature, Some(2.0));
        assert_eq!(roundtrip.max_tokens, Some(4096));
        assert!(roundtrip.validate().is_ok());
    }

    #[test]
    fn endpoint_accepts_http_and_https_but_rejects_embedded_credentials() {
        for input in [
            "https://secret@example.com/v1",
            "https://example.com/v1?key=x",
            "file:///tmp",
            "https://example.com/v1/chat/completions",
        ] {
            assert!(normalize_endpoint(input).is_err());
        }
        assert_eq!(
            normalize_endpoint(" https://example.com/v1/ ").unwrap(),
            "https://example.com/v1"
        );
        assert!(normalize_endpoint("http://127.0.0.1:1234/v1").is_ok());
        assert!(normalize_endpoint("http://example.com/v1").is_ok());
    }

    #[test]
    fn custom_headers_reject_duplicates_control_bytes_and_transport_fields() {
        let mut config = AiConfiguration::initial();
        config.mode = AiServiceMode::Custom;
        config.endpoint = "https://example.com/v1".into();
        config.model = "fixture".into();
        config.custom_headers = vec![AiCustomHeader {
            name: "X-Api-Key".into(),
            value: "synthetic-secret".into(),
        }];
        assert!(config.validate().is_ok());
        for (name, value) in [
            ("Host", "example.com"),
            ("Content-Type", "text/plain"),
            ("Content-Length", "3"),
            ("X-Request-ID", "override"),
            ("Bad Name", "value"),
            ("X-Api-Key", "line\r\nInjected: yes"),
            ("X-Api-Key", "\u{503c}"),
            ("X-Api-Key", ""),
        ] {
            config.custom_headers[0] = AiCustomHeader {
                name: name.into(),
                value: value.into(),
            };
            assert!(matches!(
                config.validate(),
                Err(AiError::InvalidConfiguration)
            ));
        }
        config.custom_headers = vec![
            AiCustomHeader {
                name: "X-Api-Key".into(),
                value: "first".into(),
            },
            AiCustomHeader {
                name: "x-api-key".into(),
                value: "second".into(),
            },
        ];
        assert!(matches!(
            config.validate(),
            Err(AiError::InvalidConfiguration)
        ));
        config.custom_headers = (0..9)
            .map(|index| AiCustomHeader {
                name: format!("X-Fixture-{index}"),
                value: "value".into(),
            })
            .collect();
        assert!(matches!(
            config.validate(),
            Err(AiError::InvalidConfiguration)
        ));
    }

    #[test]
    fn custom_header_limits_account_for_all_placeholder_expansions() {
        for (template, maximum_length) in [
            ("{{uuid}}", 36),
            ("{{timestamp}}", 20),
            ("{{uuid}}/{{uuid}}/{{timestamp}}", 94),
        ] {
            let mut header = AiCustomHeader {
                name: "X-Fixture".into(),
                value: format!("{template}{}", "x".repeat(1024 - maximum_length)),
            };
            assert!(valid_custom_headers(std::slice::from_ref(&header)));
            header.value.push('x');
            assert!(!valid_custom_headers(&[header]));
        }
    }

    #[test]
    fn configuration_file_saves_custom_fields_together_and_rejects_corrupt_documents() {
        struct Cleanup;
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = AiConfiguration::delete();
            }
        }
        let _cleanup = Cleanup;
        let update = |endpoint: &str, key: Option<&str>| AiConfigurationUpdate {
            mode: AiServiceMode::Custom,
            free_consent: false,
            endpoint: endpoint.into(),
            model: "fixture-model".into(),
            api_key: key.map(str::to_owned),
            reasoning: ReasoningMode::Default,
            temperature: None,
            max_tokens: None,
            custom_headers: key.map(|_| Vec::new()),
        };
        let mut initial = update("https://example.com/v1", Some("synthetic-key"));
        initial.custom_headers = Some(vec![AiCustomHeader {
            name: "X-Api-Key".into(),
            value: "synthetic-header-secret".into(),
        }]);
        AiConfiguration::save(initial).unwrap();
        let settings = AiConfiguration::save(update("https://example.com/v1/", None)).unwrap();
        assert!(settings.has_key);
        assert!(!serde_json::to_string(&settings)
            .unwrap()
            .contains("synthetic-key"));
        assert!(!serde_json::to_string(&settings)
            .unwrap()
            .contains("synthetic-header-secret"));
        let retained = AiConfiguration::load().unwrap().unwrap();
        assert_eq!(retained.custom_headers.len(), 1);
        assert_eq!(retained.custom_headers[0].value, "synthetic-header-secret");
        let mut different = update("https://different.example/v1", Some("new-key"));
        different.custom_headers = None;
        AiConfiguration::save(different).unwrap();
        let retained = AiConfiguration::load().unwrap().unwrap();
        assert_eq!(retained.endpoint, "https://different.example/v1");
        assert_eq!(retained.api_key, "new-key");
        assert_eq!(retained.custom_headers[0].value, "synthetic-header-secret");
        AiConfiguration::save(update("https://example.com/v1", None)).unwrap();
        let retained = AiConfiguration::load().unwrap().unwrap();
        assert_eq!(retained.endpoint, "https://example.com/v1");
        assert_eq!(retained.api_key, "new-key");
        // Exercise the worst JSON expansion, not just eight short header values.
        let mut maximum = update("https://example.com/v1", Some("synthetic-key"));
        maximum.temperature = Some(0.7);
        maximum.max_tokens = Some(4096);
        maximum.reasoning = ReasoningMode::Disabled;
        maximum.custom_headers = Some(
            (0..8)
                .map(|index| AiCustomHeader {
                    name: format!("X-{index}-{}", "a".repeat(60)),
                    value: "\\\"".repeat(512),
                })
                .collect(),
        );
        AiConfiguration::save(maximum).unwrap();
        let maximum = AiConfiguration::load().unwrap().unwrap();
        assert_eq!(maximum.custom_headers.len(), 8);
        assert_eq!(maximum.custom_headers[0].value.len(), 1024);
        assert!(configuration_file::read().unwrap().unwrap().len() > 16 * 1024);
        let mut unfinished = update("not a URL", Some("unfinished-key"));
        unfinished.mode = AiServiceMode::Free;
        unfinished.model = "unfinished-model".into();
        unfinished.temperature = Some(3.0);
        unfinished.max_tokens = Some(0);
        unfinished.custom_headers = Some(vec![AiCustomHeader {
            name: String::new(),
            value: String::new(),
        }]);
        AiConfiguration::save(unfinished).unwrap();
        let free = AiConfiguration::load().unwrap().unwrap();
        assert_eq!(free.mode, AiServiceMode::Local);
        assert_eq!(free.endpoint, maximum.endpoint);
        assert_eq!(free.model, maximum.model);
        assert_eq!(free.api_key, maximum.api_key);
        assert_eq!(free.temperature, maximum.temperature);
        assert_eq!(free.max_tokens, maximum.max_tokens);
        assert!(matches!(free.reasoning, ReasoningMode::Disabled));
        assert!(free.custom_headers == maximum.custom_headers);
        // Schema 1 retains its custom provider and defaults the legacy flag to false.
        configuration_file::write(r#"{"schemaVersion":1,"endpoint":"https://example.com/v1","model":"fixture-model","apiKey":"synthetic-key","reasoning":"default"}"#).unwrap();
        let migrated = AiConfiguration::load().unwrap().unwrap();
        assert_eq!(migrated.schema_version, 2);
        assert_eq!(migrated.mode, AiServiceMode::Custom);
        assert!(!migrated.free_consent);
        assert_eq!(migrated.temperature, None);
        assert_eq!(migrated.max_tokens, None);
        assert!(migrated.custom_headers.is_empty());
        let mut free = update("https://example.com/v1", None);
        free.mode = AiServiceMode::Free;
        free.free_consent = true;
        AiConfiguration::save(free).unwrap();
        let free = AiConfiguration::load().unwrap().unwrap();
        assert_eq!(free.mode, AiServiceMode::Local);
        assert_eq!(free.api_key, "synthetic-key");
        assert!(free.free_consent);
        AiConfiguration::save(update("https://example.com/v1", None)).unwrap();
        assert_eq!(
            AiConfiguration::load().unwrap().unwrap().mode,
            AiServiceMode::Custom
        );
        configuration_file::write("{broken").unwrap();
        assert!(matches!(
            AiConfiguration::load(),
            Err(AiError::InvalidConfiguration)
        ));
        assert_eq!(
            configuration_file::read().unwrap().as_deref(),
            Some("{broken")
        );
        AiConfiguration::save(update("https://example.com/v1", Some("replacement"))).unwrap();
        AiConfiguration::delete().unwrap();
        assert!(AiConfiguration::load().unwrap().is_none());
        let mut first_free = update("unfinished", None);
        first_free.mode = AiServiceMode::Free;
        first_free.custom_headers = Some(vec![AiCustomHeader {
            name: String::new(),
            value: String::new(),
        }]);
        AiConfiguration::save(first_free).unwrap();
        let initial = AiConfiguration::load().unwrap().unwrap();
        assert_eq!(initial.mode, AiServiceMode::Local);
        assert!(initial.endpoint.is_empty());
        assert!(initial.api_key.is_empty());
        assert!(initial.custom_headers.is_empty());
        assert_eq!(AiConfiguration::initial().mode, AiServiceMode::Local);
        assert!(!AiConfiguration::initial().free_consent);
    }
}

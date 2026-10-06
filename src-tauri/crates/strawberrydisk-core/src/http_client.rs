//! Shared HTTP identity for Core requests and desktop integrations.
//! Callers retain ownership of timeouts, redirects, retries and authentication.

use std::sync::LazyLock;

use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use strawberrydisk_platform::system_identity::{self, SystemIdentity};

// System identity is stable during a process lifetime. Cache it so constructing
// an HTTP client never repeats OS queries after the first request.
static APPLICATION_USER_AGENT: LazyLock<HeaderValue> = LazyLock::new(|| {
    let identity = system_identity::current();
    if identity.version.is_none() {
        log::warn!("http_user_agent_system_version_unavailable outcome=unknown");
    }
    HeaderValue::from_str(&format_user_agent(&identity))
        .expect("the formatted user agent contains only printable ASCII")
});

fn format_user_agent(identity: &SystemIdentity) -> String {
    let architecture = match identity.architecture.as_str() {
        "aarch64" | "arm64" => "arm64",
        "amd64" | "x86_64" => "x86_64",
        other => other,
    };
    format!(
        "StrawberryDisk/{} ({} {}; {})",
        env!("CARGO_PKG_VERSION"),
        comment_field(identity.operating_system),
        comment_field(identity.version.as_deref().unwrap_or("unknown")),
        comment_field(architecture),
    )
}

fn comment_field(value: &str) -> String {
    // Preserve native version labels such as Windows "11 (26100)" while
    // keeping each field inside one bounded HTTP comment.
    let value: String = value
        .chars()
        .take(96)
        .map(|character| match character {
            '(' => '[',
            ')' => ']',
            '\\' | ';' => '_',
            character if character.is_ascii_graphic() || character == ' ' => character,
            _ => ' ',
        })
        .collect();
    let value = value.trim();
    if value.is_empty() {
        "unknown".into()
    } else {
        value.into()
    }
}

/// Third-party clients, including the updater, use the same application headers.
pub fn default_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, APPLICATION_USER_AGENT.clone());
    headers
}

/// Request-specific headers override these defaults without affecting other clients.
pub fn builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder().default_headers(default_headers())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_agent_formats_system_versions_and_architectures() {
        for (operating_system, version, architecture, expected) in [
            ("macOS", "15.7.7", "aarch64", "macOS 15.7.7; arm64"),
            (
                "Windows",
                "11 (26100)",
                "x86_64",
                "Windows 11 [26100]; x86_64",
            ),
            ("Linux", "24.04", "aarch64", "Linux 24.04; arm64"),
        ] {
            let identity = SystemIdentity {
                operating_system,
                version: Some(version.into()),
                architecture: architecture.into(),
            };
            assert_eq!(
                format_user_agent(&identity),
                format!("StrawberryDisk/{} ({expected})", env!("CARGO_PKG_VERSION")),
            );
        }
    }

    #[test]
    fn missing_or_unusual_system_versions_cannot_break_requests() {
        let mut identity = SystemIdentity {
            operating_system: "Linux",
            version: None,
            architecture: "x86_64".into(),
        };
        assert!(format_user_agent(&identity).contains("Linux unknown; x86_64"));
        identity.version = Some("release\r\nInjected: value; (\\)\u{65e5}".repeat(100));
        let value = format_user_agent(&identity);
        assert!(HeaderValue::from_str(&value).is_ok());
        assert!(value.len() < 160);
        assert_eq!(value.matches('(').count(), 1);
        assert_eq!(value.matches(')').count(), 1);
        assert_eq!(value.matches(';').count(), 1);
        assert!(!value.contains('\\'));
    }
}

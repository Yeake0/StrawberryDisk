//! Native surfaces reuse the same locale resources as Vue.
use serde_json::Value;
use std::sync::OnceLock;
use tauri_plugin_store::StoreExt;

pub struct NativeLabels {
    pub locale: String,
    messages: &'static Value,
}
impl NativeLabels {
    pub fn load(app: &tauri::AppHandle) -> Self {
        let locale = app
            .store_builder("settings.json")
            .disable_auto_save()
            .build()
            .ok()
            .and_then(|store| store.get("settings"))
            .and_then(|value| {
                value
                    .get("language")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .or_else(tauri_plugin_os::locale)
            .unwrap_or_else(|| "en-US".into());
        Self::for_locale(&locale)
    }
    pub fn for_locale(locale: &str) -> Self {
        let locale = supported_locale(locale);
        static MESSAGES: OnceLock<[Value; 2]> = OnceLock::new();
        let values = MESSAGES.get_or_init(|| {
            [
                include_str!("../../../src/locales/en-US.json"),
                include_str!("../../../src/locales/pt-BR.json"),
            ]
            .map(|text| serde_json::from_str(text).expect("validated locale resource"))
        });
        let index = match locale {
            "pt-BR" => 1,
            _ => 0,
        };
        Self {
            locale: locale.into(),
            messages: &values[index],
        }
    }
    pub fn text(&self, key: &str) -> &str {
        self.messages
            .pointer(key)
            .and_then(Value::as_str)
            .unwrap_or("—")
    }
}

/// Match the frontend language-prefix policy for installations that have not
/// persisted a language yet.
fn supported_locale(locale: &str) -> &'static str {
    let locale = locale.trim().to_ascii_lowercase();
    for (prefix, supported) in [("pt", "pt-BR"), ("en", "en-US")] {
        if locale == prefix || locale.starts_with(&format!("{prefix}-")) {
            return supported;
        }
    }
    "en-US"
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn runtime_prompts_are_complete_in_every_supported_locale() {
        for locale in ["en-US", "pt-BR"] {
            let labels = NativeLabels::for_locale(locale);
            for key in ["updateRequired", "update", "exit", "openFailed"] {
                let value = labels.text(&format!("/webviewRuntime/{key}"));
                assert!(!value.is_empty() && value != "—", "{locale}: {key}");
            }
            assert!(labels.text("/webviewRuntime/openFailed").contains("{url}"));
        }
    }

    #[test]
    fn native_defaults_match_supported_browser_language_prefixes() {
        for (input, expected) in [
            ("zh-CN", "en-US"),
            ("ja", "en-US"),
            ("ru-RU", "en-US"),
            ("en-GB", "en-US"),
            ("tr", "en-US"),
            ("pt-BR", "pt-BR"),
            ("pt-PT", "pt-BR"),
            ("pt", "pt-BR"),
            ("tricky", "en-US"),
            ("de-DE", "en-US"),
        ] {
            assert_eq!(NativeLabels::for_locale(input).locale, expected);
        }
    }
}

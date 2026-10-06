//! Readable, bounded values for native diagnostic logs.
use std::fmt::Display;

const MAX_DIAGNOSTIC_CHARS: usize = 8192;

/// Retains object names, paths and native error details in one quoted log field.
/// Escape control characters so filenames or process output cannot forge log records.
/// Callers must exclude credentials, authorization tokens and document contents before
/// calling this formatter; arbitrary text cannot be reliably classified as a secret.
pub fn text(value: &(impl Display + ?Sized)) -> String {
    format!("{:?}", bounded_message(value, MAX_DIAGNOSTIC_CHARS))
}

/// Bounds native error text before IPC serialization. A batch must remain within
/// its message budget even when every item fails with a long diagnostic.
pub fn bounded_message(value: &(impl Display + ?Sized), max_chars: usize) -> String {
    let value = value.to_string();
    let mut chars = value.chars();
    let mut retained: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        retained.push_str("…[truncated]");
    }
    retained
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_preserves_native_reason_and_escapes_record_boundaries() {
        let value =
            text(&"C:\\Users\\Tester\\cache: access denied (os error 5)\nforged_record=true");
        assert!(value.contains("access denied (os error 5)"));
        assert!(value.contains("Tester"));
        assert!(value.contains("\\nforged_record=true"));
        assert!(!value.contains('\n'));
    }

    #[test]
    fn oversized_unicode_output_is_bounded_with_explicit_truncation() {
        let value = text(&"🦀".repeat(MAX_DIAGNOSTIC_CHARS + 1));
        assert_eq!(value.matches('🦀').count(), MAX_DIAGNOSTIC_CHARS);
        assert!(value.ends_with("…[truncated]\""));
    }
}

use std::{collections::BTreeMap, fs, path::Path};

use serde_json::Value;

const MAX_PROFILE_METADATA_BYTES: u64 = 8 * 1024 * 1024;
const MAX_PROFILE_NAME_CHARS: usize = 128;

/// Resolves Chromium's user-facing profile label without exposing other preference values.
/// The bounded read prevents an unexpectedly large or replaced metadata file from becoming part
/// of an ordinary privacy scan. Invalid metadata safely falls back to the stable directory name.
pub(crate) fn chromium_display_name(profile_root: &Path, fallback: &str) -> String {
    let Some(bytes) = read_bounded_file(&profile_root.join("Preferences")) else {
        return fallback.into();
    };
    let name = serde_json::from_slice::<Value>(&bytes)
        .ok()
        .and_then(|value| {
            value
                .get("profile")?
                .get("name")?
                .as_str()
                .map(str::to_owned)
        });
    sanitized_name(name.as_deref()).unwrap_or_else(|| fallback.into())
}

/// Maps Firefox profile directory names to the labels declared in `profiles.ini`. Matching only
/// the final path component keeps the parser independent of platform separators and prevents
/// arbitrary paths from entering the public scan protocol.
pub(crate) fn firefox_display_names(profiles_root: &Path) -> BTreeMap<String, String> {
    let Some(bytes) = profiles_root
        .parent()
        .and_then(|parent| read_bounded_file(&parent.join("profiles.ini")))
    else {
        return BTreeMap::new();
    };
    let Ok(contents) = std::str::from_utf8(&bytes) else {
        return BTreeMap::new();
    };

    let mut names = BTreeMap::new();
    let mut current_name: Option<String> = None;
    let mut current_path: Option<String> = None;
    for line in contents.lines().map(str::trim) {
        if line.starts_with('[') && line.ends_with(']') {
            insert_firefox_profile(&mut names, current_path.take(), current_name.take());
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "Name" => current_name = sanitized_name(Some(value.trim())),
            "Path" => current_path = Some(value.trim().replace('\\', "/")),
            _ => {}
        }
    }
    insert_firefox_profile(&mut names, current_path, current_name);
    names
}

fn insert_firefox_profile(
    names: &mut BTreeMap<String, String>,
    path: Option<String>,
    name: Option<String>,
) {
    let Some((path, name)) = path.zip(name) else {
        return;
    };
    let Some(directory_name) = path.rsplit('/').find(|component| !component.is_empty()) else {
        return;
    };
    names.entry(directory_name.into()).or_insert(name);
}

fn sanitized_name(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    if value.is_empty()
        || value.chars().count() > MAX_PROFILE_NAME_CHARS
        || value.chars().any(char::is_control)
    {
        return None;
    }
    Some(value.into())
}

fn read_bounded_file(path: &Path) -> Option<Vec<u8>> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > MAX_PROFILE_METADATA_BYTES
    {
        return None;
    }
    fs::read(path).ok()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

    struct Fixture(std::path::PathBuf);

    impl Fixture {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "strawberrydisk-browser-profile-{name}-{}-{}",
                std::process::id(),
                NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).expect("profile fixture directory must be created");
            Self(path)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn chromium_name_uses_preferences_and_rejects_control_characters() {
        let fixture = Fixture::new("chromium");
        fs::write(
            fixture.0.join("Preferences"),
            br#"{"profile":{"name":"Work"},"unrelated":"private"}"#,
        )
        .unwrap();
        assert_eq!(chromium_display_name(&fixture.0, "Profile 1"), "Work");

        fs::write(
            fixture.0.join("Preferences"),
            br#"{"profile":{"name":"invalid\nname"}}"#,
        )
        .unwrap();
        assert_eq!(chromium_display_name(&fixture.0, "Profile 1"), "Profile 1");
    }

    #[test]
    fn firefox_names_match_profile_directory_components() {
        let fixture = Fixture::new("firefox");
        let profiles = fixture.0.join("Profiles");
        fs::create_dir_all(&profiles).unwrap();
        fs::write(
            fixture.0.join("profiles.ini"),
            "[Profile0]\nName=Personal\nIsRelative=1\nPath=Profiles/abc.default-release\n\n[Install]\nDefault=Profiles/abc.default-release\n",
        )
        .unwrap();

        assert_eq!(
            firefox_display_names(&profiles).get("abc.default-release"),
            Some(&"Personal".into())
        );
    }
}

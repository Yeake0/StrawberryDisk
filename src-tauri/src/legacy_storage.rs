//! Preserve user data when the application identifier changes with the rebrand.

use std::{fs, io, path::Path};

const LEGACY_IDENTIFIER: &str = "app.mangodisk.desktop";
#[cfg(windows)]
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

pub(crate) fn migrate(local_data: &Path, config_data: &Path) {
    for (destination, entries) in [
        (local_data, &["data"][..]),
        (
            config_data,
            &[
                ".window-state.json",
                "installation.json",
                "memory-release.json",
                "resident.json",
                "settings.json",
            ][..],
        ),
    ] {
        let Some(parent) = destination.parent() else {
            continue;
        };
        let source = parent.join(LEGACY_IDENTIFIER);
        if !fs::symlink_metadata(&source)
            .is_ok_and(|metadata| metadata.is_dir() && !is_link(&metadata))
        {
            continue;
        }
        for entry in entries {
            let from = source.join(entry);
            let to = destination.join(entry);
            if let Err(error) = copy_missing(&from, &to) {
                log::warn!(
                    "legacy_storage_migration_failed source={} destination={} error={error}",
                    from.display(),
                    to.display()
                );
            }
        }
    }
}

fn copy_missing(source: &Path, destination: &Path) -> io::Result<()> {
    let metadata = match fs::symlink_metadata(source) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if is_link(&metadata) {
        return Ok(());
    }
    if let Ok(existing) = fs::symlink_metadata(destination) {
        if is_link(&existing) {
            return Ok(());
        }
    }
    if metadata.is_dir() {
        if destination.exists() && !destination.is_dir() {
            return Ok(());
        }
        fs::create_dir_all(destination)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_missing(&entry.path(), &destination.join(entry.file_name()))?;
        }
    } else if metadata.is_file() {
        if destination.exists() {
            return Ok(());
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        let temp = destination.with_extension(format!("migration-{}.tmp", uuid::Uuid::new_v4()));
        fs::copy(source, &temp)?;
        let install = fs::hard_link(&temp, destination);
        let cleanup = fs::remove_file(&temp);
        install?;
        cleanup?;
    }
    Ok(())
}

fn is_link(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_legacy_data_without_overwriting_current_settings() {
        let root =
            std::env::temp_dir().join(format!("strawberrydisk-migration-{}", uuid::Uuid::new_v4()));
        let old = root.join(LEGACY_IDENTIFIER);
        let new = root.join("app.strawberrydisk.desktop");
        fs::create_dir_all(old.join("data")).unwrap();
        fs::create_dir_all(new.join("data")).unwrap();
        fs::write(old.join("settings.json"), b"legacy").unwrap();
        fs::write(old.join("data/history.db"), b"history").unwrap();
        fs::write(new.join("settings.json"), b"current").unwrap();
        migrate(&new, &new);
        assert_eq!(fs::read(new.join("settings.json")).unwrap(), b"current");
        assert_eq!(fs::read(new.join("data/history.db")).unwrap(), b"history");
        fs::remove_dir_all(root).unwrap();
    }
}

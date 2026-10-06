use std::{
    fs,
    os::windows::fs::MetadataExt,
    path::{Path, PathBuf},
};

use crate::{Platform, PlatformError, PlatformResult};

use super::{
    is_remote_placeholder_attributes, WindowsPlatform, FILE_ATTRIBUTE_REPARSE_POINT_VALUE,
};

/// Windows Known Folders may reach a share through a local directory symlink
/// or junction. Resolve only this entry point and keep every downstream scan
/// and mutation bound to the verified target. Cloud placeholders and unknown
/// reparse providers remain unsupported rather than being opened for recall.
pub(super) fn resolve(path: &Path) -> PlatformResult<PathBuf> {
    if !path.is_absolute() {
        return Err(PlatformError::invalid_path(
            "directory entry must be absolute",
        ));
    }
    validate_entry_components(path)?;
    let canonical = fs::canonicalize(path)
        .map_err(|error| PlatformError::io("resolve directory entry", &error))?;
    let target = WindowsPlatform.canonicalize_no_links(&canonical)?;
    if !fs::symlink_metadata(&target)
        .map_err(|error| PlatformError::io("validate directory target", &error))?
        .is_dir()
    {
        return Err(PlatformError::invalid_path(
            "directory entry target is not a directory",
        ));
    }
    validate_entry_components(path)?;
    let verified = fs::canonicalize(path)
        .map_err(|error| PlatformError::io("revalidate directory entry", &error))?;
    if !WindowsPlatform.paths_equal(&target, &verified) {
        return Err(PlatformError::item_changed(
            "directory entry changed during resolution",
        ));
    }
    Ok(target)
}

fn validate_entry_components(path: &Path) -> PlatformResult<()> {
    for component in path.ancestors().collect::<Vec<_>>().into_iter().rev() {
        if component.as_os_str().is_empty() {
            continue;
        }
        let metadata = fs::symlink_metadata(component)
            .map_err(|error| PlatformError::io("inspect directory entry", &error))?;
        let attributes = metadata.file_attributes();
        if is_remote_placeholder_attributes(attributes) {
            return Err(PlatformError::invalid_path(
                "directory entry requires remote recall",
            ));
        }
        if attributes & FILE_ATTRIBUTE_REPARSE_POINT_VALUE != 0 {
            // read_link accepts Windows symbolic links and mount-point junctions,
            // but rejects opaque reparse tags such as cloud-provider placeholders.
            fs::read_link(component)
                .map_err(|error| PlatformError::io("inspect directory redirection", &error))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };

    struct Sandbox(PathBuf);

    impl Drop for Sandbox {
        fn drop(&mut self) {
            // Remove only fixture-owned junctions before removing real targets.
            for name in ["entry", "chain", "broken", "target/nested"] {
                let _ = fs::remove_dir(self.0.join(name));
            }
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn junction(link: &Path, target: &Path) {
        let output = Command::new("cmd.exe")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .output()
            .expect("create the junction fixture");
        assert!(output.status.success(), "junction creation should succeed");
    }

    #[test]
    fn explicit_junctions_resolve_without_relaxing_traversal() {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("StrawberryDisk-Directory-Entry-{id}"));
        let _sandbox = Sandbox(root.clone());
        let target = root.join("target");
        fs::create_dir_all(target.join("space and unicode-\u{00e9}")).unwrap();
        fs::create_dir_all(root.join("outside")).unwrap();
        fs::write(target.join("file.txt"), "fixture").unwrap();
        junction(&root.join("entry"), &target);
        junction(&root.join("chain"), &root.join("entry"));
        junction(&root.join("broken"), &root.join("missing"));
        junction(&target.join("nested"), &root.join("outside"));

        for name in ["entry", "chain", "target"] {
            assert_eq!(
                resolve(&root.join(name)).unwrap(),
                fs::canonicalize(&target).unwrap()
            );
        }
        assert_eq!(
            resolve(&root.join("entry/space and unicode-\u{00e9}")).unwrap(),
            fs::canonicalize(target.join("space and unicode-\u{00e9}")).unwrap()
        );
        assert!(WindowsPlatform
            .canonicalize_no_links(&root.join("entry"))
            .is_err());
        assert!(WindowsPlatform
            .canonicalize_no_links(&target.join("nested"))
            .is_err());
        assert!(resolve(&root.join("broken")).is_err());
        assert!(resolve(&target.join("file.txt")).is_err());
        assert!(resolve(Path::new("relative")).is_err());
        assert!(resolve(Path::new(r"C:relative")).is_err());
    }
}

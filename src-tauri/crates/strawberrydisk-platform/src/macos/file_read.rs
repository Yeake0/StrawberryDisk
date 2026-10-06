use std::{io, path::Path};

/// EACCES represents BSD/ACL denial; EPERM alone can also come from sandbox or
/// security software. Offer privacy guidance only for EPERM in known protected
/// app-data locations, without probing them or triggering another TCC prompt.
pub(crate) fn is_privacy_restricted(path: &Path, error: &io::Error) -> bool {
    if error.raw_os_error() != Some(libc::EPERM) {
        return false;
    }
    let Some(home) = dirs::home_dir() else {
        return false;
    };
    let library = home.join("Library");
    [
        "Containers",
        "Group Containers",
        "Mail",
        "Messages",
        "Safari",
    ]
    .iter()
    .any(|directory| path.starts_with(library.join(directory)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn privacy_guidance_requires_both_protected_path_and_native_denial() {
        let home = dirs::home_dir().expect("home must be available");
        let protected = home.join("Library/Containers/example/Data/Library/Caches");
        assert!(is_privacy_restricted(
            &protected,
            &io::Error::from_raw_os_error(libc::EPERM)
        ));
        assert!(!is_privacy_restricted(
            &protected,
            &io::Error::from_raw_os_error(libc::EACCES)
        ));
        assert!(!is_privacy_restricted(
            &home.join("Library/Caches/example"),
            &io::Error::from_raw_os_error(libc::EPERM)
        ));
        assert!(!is_privacy_restricted(
            &home.join("Library/Containers-backup/example"),
            &io::Error::from_raw_os_error(libc::EPERM)
        ));
    }
}

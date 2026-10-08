use std::{fs, io::Read, path::Path};

use core_foundation::{
    array::CFArray,
    base::{CFType, CFTypeRef, TCFType},
    dictionary::CFDictionary,
    number::CFNumber,
    string::{CFString, CFStringRef},
    url::CFURL,
};

use crate::{ApplicationIdentityMetadata, ApplicationSigningKind, ApplicationSigningMetadata};

#[link(name = "Security", kind = "framework")]
unsafe extern "C" {
    fn SecStaticCodeCreateWithPath(path: CFTypeRef, flags: u32, code: *mut CFTypeRef) -> i32;
    fn SecCodeCopySigningInformation(code: CFTypeRef, flags: u32, info: *mut CFTypeRef) -> i32;
    fn SecCertificateCopySubjectSummary(certificate: CFTypeRef) -> CFStringRef;
    static kSecCodeInfoIdentifier: CFStringRef;
    static kSecCodeInfoTeamIdentifier: CFStringRef;
    static kSecCodeInfoCertificates: CFStringRef;
    static kSecCodeInfoFlags: CFStringRef;
}

const MAX_PLIST_BYTES: u64 = 1024 * 1024;

fn text(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty() && value.len() <= 1024).then(|| value.to_owned())
}

pub(crate) fn read(path: &Path, expected_identifier: &str) -> Option<ApplicationIdentityMetadata> {
    let plist_path = path.join("Contents/Info.plist");
    let metadata = fs::symlink_metadata(&plist_path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_PLIST_BYTES {
        return None;
    }
    let mut bytes = Vec::new();
    fs::File::open(&plist_path)
        .ok()?
        .take(MAX_PLIST_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_PLIST_BYTES {
        return None;
    }
    let plist = plist::Value::from_reader(std::io::Cursor::new(bytes))
        .ok()?
        .into_dictionary()?;
    let value = |key: &str| {
        plist
            .get(key)
            .and_then(plist::Value::as_string)
            .and_then(text)
    };
    let identifier = value("CFBundleIdentifier")?;
    if identifier != expected_identifier {
        log::warn!("application_identity_unavailable path={} stage=bundle_identity reason=identity_changed", crate::diagnostics::text(&path.to_string_lossy()));
        return None;
    }
    Some(ApplicationIdentityMetadata::Macos {
        bundle_identifier: Some(identifier),
        product_name: value("CFBundleDisplayName").or_else(|| value("CFBundleName")),
        category: value("LSApplicationCategoryType"),
        signing: signing_metadata(path),
    })
}

fn unavailable() -> ApplicationSigningMetadata {
    ApplicationSigningMetadata {
        kind: ApplicationSigningKind::Unavailable,
        certificate_subject: None,
        team_identifier: None,
    }
}

fn signing_metadata(path: &Path) -> ApplicationSigningMetadata {
    let Some(url) = CFURL::from_path(path, true) else {
        return unavailable();
    };
    let mut code = std::ptr::null();
    // Create/Copy results are owned; CF wrappers release them on every return path.
    let status = unsafe { SecStaticCodeCreateWithPath(url.as_CFTypeRef(), 0, &mut code) };
    if status != 0 || code.is_null() {
        log::debug!("application_identity_signing_unavailable path={} stage=create_code native_code={status}", crate::diagnostics::text(&path.to_string_lossy()));
        return unavailable();
    }
    let code = unsafe { CFType::wrap_under_create_rule(code) };
    let mut info = std::ptr::null();
    // This reads metadata without checking validity, revocation or notarization.
    let status = unsafe { SecCodeCopySigningInformation(code.as_CFTypeRef(), 1 << 1, &mut info) };
    if status != 0 || info.is_null() {
        log::debug!(
            "application_identity_signing_unavailable path={} stage=copy_info native_code={status}",
            crate::diagnostics::text(&path.to_string_lossy())
        );
        return unavailable();
    }
    let info: CFDictionary<CFString, CFType> =
        unsafe { CFDictionary::wrap_under_create_rule(info.cast()) };
    let key = |reference| unsafe { CFString::wrap_under_get_rule(reference) };
    if info.find(key(unsafe { kSecCodeInfoIdentifier })).is_none() {
        return ApplicationSigningMetadata {
            kind: ApplicationSigningKind::Unsigned,
            ..unavailable()
        };
    }
    let flags = info
        .find(key(unsafe { kSecCodeInfoFlags }))
        .and_then(|value| value.downcast::<CFNumber>())
        .and_then(|value| value.to_i64())
        .unwrap_or(0);
    let team = info
        .find(key(unsafe { kSecCodeInfoTeamIdentifier }))
        .and_then(|value| value.downcast::<CFString>())
        .and_then(|value| text(&value.to_string()));
    let certificate = info
        .find(key(unsafe { kSecCodeInfoCertificates }))
        .and_then(|value| value.downcast::<CFArray>())
        .and_then(|certificates| {
            let certificate = certificates.get(0)?;
            let subject = unsafe { SecCertificateCopySubjectSummary(*certificate) };
            (!subject.is_null()).then(|| unsafe { CFString::wrap_under_create_rule(subject) })
        })
        .and_then(|value| text(&value.to_string()));
    ApplicationSigningMetadata {
        kind: if flags & 2 != 0 {
            ApplicationSigningKind::AdHoc
        } else if certificate.is_some() {
            ApplicationSigningKind::Certificate
        } else {
            ApplicationSigningKind::Unavailable
        },
        certificate_subject: certificate,
        team_identifier: team,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_metadata_is_bounded_and_must_match_the_catalog_identity() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("Contents")).unwrap();
        let mut plist = plist::Dictionary::new();
        for (key, value) in [
            ("CFBundleIdentifier", "com.example.game"),
            ("CFBundleName", "Example Game"),
            ("LSApplicationCategoryType", "public.app-category.games"),
        ] {
            plist.insert(key.into(), value.into());
        }
        plist::Value::Dictionary(plist)
            .to_file_xml(root.path().join("Contents/Info.plist"))
            .unwrap();
        let value = read(root.path(), "com.example.game").unwrap();
        let json = serde_json::to_value(value).unwrap();
        assert_eq!(json["category"], "public.app-category.games");
        assert_eq!(json["productName"], "Example Game");
        assert!(read(root.path(), "com.other.game").is_none());
        assert!(text(&"x".repeat(1025)).is_none());
    }

    #[test]
    fn missing_oversized_or_linked_plists_do_not_supply_identity() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("Contents")).unwrap();
        let path = root.path().join("Contents/Info.plist");
        assert!(read(root.path(), "com.example.game").is_none());
        fs::write(&path, vec![b'x'; MAX_PLIST_BYTES as usize + 1]).unwrap();
        assert!(read(root.path(), "com.example.game").is_none());
        fs::remove_file(&path).unwrap();
        let target = root.path().join("outside.plist");
        fs::write(&target, b"not a plist").unwrap();
        std::os::unix::fs::symlink(&target, &path).unwrap();
        assert!(read(root.path(), "com.example.game").is_none());
    }

    #[test]
    #[ignore = "reads an explicitly selected installed application without executing it"]
    fn actual_application_identity_metadata() {
        let path = std::env::var("STRAWBERRYDISK_IDENTITY_PATH").unwrap();
        let identifier = std::env::var("STRAWBERRYDISK_IDENTITY_BUNDLE_ID").unwrap();
        let metadata = read(Path::new(&path), &identifier).expect("matching installed bundle");
        let output = std::env::var("STRAWBERRYDISK_IDENTITY_OUTPUT").unwrap();
        fs::write(output, serde_json::to_vec_pretty(&metadata).unwrap()).unwrap();
    }
}

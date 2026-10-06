use winreg::{enums::KEY_READ, RegKey};

use super::{estimated_bytes_from_kib, normalize_product_code, string_value, RegistryScope};

/// Keeps the original estimate until every merged view agrees on the MSI alias identity.
/// Size evidence never authorizes native uninstall execution.
#[derive(Clone, Debug)]
pub(super) struct RegistrySizeEstimate {
    original_bytes: u64,
    alias: Option<MsiAliasSizeEvidence>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MsiAliasIdentity {
    scope: RegistryScope,
    product_code: String,
    display_name: String,
    display_version: String,
    publisher: String,
    raw_size: u32,
}

#[derive(Clone, Debug)]
struct MsiAliasSizeEvidence {
    identity: MsiAliasIdentity,
    corrected_bytes: Option<u64>,
}

impl RegistrySizeEstimate {
    pub(super) fn read(
        uninstall: &RegKey,
        entry: &RegKey,
        key_name: &str,
        estimated_size_kib: Option<u32>,
        scope: RegistryScope,
        registry_view: u32,
    ) -> Self {
        Self {
            original_bytes: estimated_bytes_from_kib(estimated_size_kib),
            alias: msi_alias_size_evidence(
                uninstall,
                entry,
                key_name,
                estimated_size_kib,
                scope,
                registry_view,
            ),
        }
    }

    pub(super) fn bytes(&self) -> u64 {
        self.alias
            .as_ref()
            .and_then(|alias| alias.corrected_bytes)
            .unwrap_or(self.original_bytes)
    }

    pub(super) fn with_fallback(mut self, bytes: u64) -> Self {
        self.original_bytes = self.original_bytes.max(bytes);
        self
    }

    pub(super) fn merge(&mut self, other: &Self) {
        self.original_bytes = self.original_bytes.max(other.original_bytes);
        // An uncorroborated duplicate can retain a correction only when its complete
        // alias identity agrees. Conflicts permanently restore the original max policy.
        match (&mut self.alias, &other.alias) {
            (Some(left), Some(right)) if left.identity == right.identity => {
                match (left.corrected_bytes, right.corrected_bytes) {
                    (Some(left_bytes), Some(right_bytes)) if left_bytes != right_bytes => {
                        self.alias = None;
                    }
                    (None, Some(bytes)) => left.corrected_bytes = Some(bytes),
                    _ => {}
                }
            }
            _ => self.alias = None,
        }
    }
}

fn msi_alias_size_evidence(
    uninstall: &RegKey,
    entry: &RegKey,
    key_name: &str,
    estimated_size_kib: Option<u32>,
    scope: RegistryScope,
    registry_view: u32,
) -> Option<MsiAliasSizeEvidence> {
    let raw_size = estimated_size_kib.filter(|size| *size > 0)?;
    if normalize_product_code(key_name).is_some()
        || entry.get_value::<u32, _>("WindowsInstaller").ok() != Some(1)
    {
        return None;
    }
    let command = string_value(entry, "UninstallString")?;
    let product_code = msi_command_product_code(&command)?;
    let identity_field = |field| {
        string_value(entry, field)
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty())
    };
    let mut evidence = MsiAliasSizeEvidence {
        identity: MsiAliasIdentity {
            scope,
            product_code,
            display_name: identity_field("DisplayName")?,
            display_version: identity_field("DisplayVersion")?,
            publisher: identity_field("Publisher")?,
            raw_size,
        },
        corrected_bytes: None,
    };
    // Read corroboration only inside this scope and view. A missing product leaves an
    // alias identity for duplicate merging; unreadable or contradictory evidence does not.
    let product = match uninstall
        .open_subkey_with_flags(&evidence.identity.product_code, KEY_READ | registry_view)
    {
        Ok(product) => product,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Some(evidence),
        Err(_) => return None,
    };
    if product.get_value::<u32, _>("WindowsInstaller").ok() != Some(1) {
        return None;
    }
    for (field, alias_value) in [
        ("DisplayName", &evidence.identity.display_name),
        ("DisplayVersion", &evidence.identity.display_version),
        ("Publisher", &evidence.identity.publisher),
    ] {
        let product_value = string_value(&product, field)?;
        if !alias_value.eq_ignore_ascii_case(product_value.trim()) {
            return None;
        }
    }
    let product_size_kib = product.get_value::<u32, _>("EstimatedSize").ok()?;
    let corrected_bytes = byte_valued_alias_estimate(raw_size, product_size_kib)?;
    log::info!(
        "windows_application_size_correction_evidence registry_key={} product_code={} stage=registry_estimate source=msi_product raw_estimated_size={} product_estimated_size_kib={} estimated_bytes={} outcome=byte_unit_mismatch",
        crate::diagnostics::text(key_name), evidence.identity.product_code, raw_size, product_size_kib, corrected_bytes
    );
    evidence.corrected_bytes = Some(corrected_bytes);
    Some(evidence)
}

fn byte_valued_alias_estimate(raw_size: u32, product_size_kib: u32) -> Option<u64> {
    let product_bytes = estimated_bytes_from_kib(Some(product_size_kib));
    // MSI's estimate can include small installer overhead beyond the payload. Require
    // agreement within 1% in bytes, rather than guessing from an application's size.
    (raw_size > 0
        && product_bytes > 0
        && u64::from(raw_size).abs_diff(product_bytes) * 100 <= product_bytes)
        .then_some(product_bytes)
}

fn msi_command_product_code(command: &str) -> Option<String> {
    let command = command.trim();
    let (executable, arguments) = if let Some(quoted) = command.strip_prefix('"') {
        let closing_quote = quoted.find('"')?;
        (&quoted[..closing_quote], &quoted[closing_quote + 1..])
    } else {
        command.split_once(char::is_whitespace)?
    };
    let host = std::path::Path::new(executable).file_name()?.to_str()?;
    if !host.eq_ignore_ascii_case("msiexec.exe") && !host.eq_ignore_ascii_case("msiexec") {
        return None;
    }
    let arguments = arguments.trim();
    let operation = arguments.get(..2)?;
    if !operation.eq_ignore_ascii_case("/x") && !operation.eq_ignore_ascii_case("/i") {
        return None;
    }
    // Accept exactly one product identifier. Extra flags, multiple products, and shell
    // expressions are deliberately left to the original estimate without interpretation.
    normalize_product_code(arguments[2..].trim())
}

#[cfg(test)]
mod tests {
    use super::{byte_valued_alias_estimate, msi_command_product_code, RegistrySizeEstimate};
    use winreg::{
        enums::{HKEY_CURRENT_USER, KEY_WOW64_64KEY},
        RegKey,
    };

    const PRODUCT_CODE: &str = "{01234567-89AB-CDEF-0123-456789ABCDEF}";

    fn corrected_bytes(
        uninstall: &RegKey,
        entry: &RegKey,
        key_name: &str,
        size: Option<u32>,
        view: u32,
    ) -> Option<u64> {
        RegistrySizeEstimate::read(
            uninstall,
            entry,
            key_name,
            size,
            super::RegistryScope::Machine,
            view,
        )
        .alias
        .and_then(|alias| alias.corrected_bytes)
    }

    #[test]
    fn corrects_only_the_evidenced_byte_unit_mismatch() {
        assert_eq!(
            byte_valued_alias_estimate(74_748_942, 73_236),
            Some(74_993_664)
        );
        for (alias, product) in [
            (73_236, 73_236),
            (74_748_942, 74_748_942),
            (74_748_942, 1_024),
            (74_000_000, 73_236),
            (0, 73_236),
            (74_748_942, 0),
            (u32::MAX, u32::MAX),
        ] {
            assert_eq!(byte_valued_alias_estimate(alias, product), None);
        }
        assert_eq!(
            byte_valued_alias_estimate(1_034_240, 1_000),
            Some(1_024_000)
        );
        assert_eq!(byte_valued_alias_estimate(1_034_241, 1_000), None);
    }

    #[test]
    fn display_product_identity_requires_an_exact_msi_command() {
        for command in [
            format!("MsiExec.exe /X {PRODUCT_CODE}"),
            format!("msiexec /i{PRODUCT_CODE}"),
            format!(r#""C:\Windows\System32\msiexec.exe" /x{PRODUCT_CODE}"#),
        ] {
            assert_eq!(
                msi_command_product_code(&command),
                Some(PRODUCT_CODE.to_string())
            );
        }
        for command in [
            format!("installer.exe /X {PRODUCT_CODE}"),
            format!("msiexec.exe /X {PRODUCT_CODE} /qn"),
            format!("msiexec.exe /X {PRODUCT_CODE} & other.exe"),
            format!("msiexec.exe /X {PRODUCT_CODE} {PRODUCT_CODE}"),
            format!("msiexec.exe /package {PRODUCT_CODE}"),
            "msiexec.exe /x invalid".to_string(),
            "msiexec.exe /".to_string(),
            "msiexec.exe /\u{754c}".to_string(),
        ] {
            assert_eq!(msi_command_product_code(&command), None);
        }
    }

    #[test]
    fn registry_correction_requires_matching_msi_identity_in_the_same_view() {
        let fixture = RegistryFixture::new();
        let (_view_root, uninstall) = fixture.view("single");
        let (alias, _) = uninstall
            .create_subkey("ExampleApplication")
            .expect("alias should be created");
        let (product, _) = uninstall
            .create_subkey(PRODUCT_CODE)
            .expect("product should be created");
        for entry in [&alias, &product] {
            entry
                .set_value("WindowsInstaller", &1_u32)
                .expect("MSI marker should be written");
            for (field, value) in [
                ("DisplayName", "Example"),
                ("DisplayVersion", "1.2.3"),
                ("Publisher", "Example Vendor"),
            ] {
                entry
                    .set_value(field, &value)
                    .expect("identity should be written");
            }
        }
        alias
            .set_value("UninstallString", &format!("MsiExec.exe /X {PRODUCT_CODE}"))
            .expect("command should be written");
        product
            .set_value("EstimatedSize", &73_236_u32)
            .expect("product estimate should be written");
        let estimate = || {
            corrected_bytes(
                &uninstall,
                &alias,
                "ExampleApplication",
                Some(74_748_942),
                KEY_WOW64_64KEY,
            )
        };
        assert_eq!(estimate(), Some(74_993_664));
        assert_eq!(
            corrected_bytes(
                &uninstall,
                &alias,
                PRODUCT_CODE,
                Some(74_748_942),
                KEY_WOW64_64KEY
            ),
            None
        );
        assert_eq!(
            corrected_bytes(
                &uninstall,
                &alias,
                "ExampleApplication",
                None,
                KEY_WOW64_64KEY
            ),
            None
        );
        for field in ["DisplayName", "DisplayVersion", "Publisher"] {
            let original: String = product
                .get_value(field)
                .expect("original identity should exist");
            product
                .set_value(field, &"Other")
                .expect("mismatch should be written");
            assert_eq!(
                estimate(),
                None,
                "mismatched {field} must retain the original estimate"
            );
            product
                .set_value(field, &original)
                .expect("identity should be restored");
        }
        alias
            .set_value("WindowsInstaller", &0_u32)
            .expect("non-MSI marker should be written");
        assert_eq!(estimate(), None);
        alias
            .set_value("WindowsInstaller", &1_u32)
            .expect("MSI marker should be restored");
        product
            .set_value("WindowsInstaller", &0_u32)
            .expect("non-MSI marker should be written");
        assert_eq!(estimate(), None);
        product
            .set_value("WindowsInstaller", &1_u32)
            .expect("MSI marker should be restored");
        product
            .set_value("EstimatedSize", &74_748_942_u32)
            .expect("valid large estimate should be written");
        assert_eq!(estimate(), None);
        uninstall
            .delete_subkey_all(PRODUCT_CODE)
            .expect("product fixture should be removed");
        assert_eq!(estimate(), None);
    }

    struct RegistryFixture {
        _identity: tempfile::TempDir,
        root: RegKey,
        path: String,
    }

    impl RegistryFixture {
        fn new() -> Self {
            let identity = tempfile::tempdir().expect("fixture identity should be unique");
            let path = format!(
                r"Software\StrawberryDiskTests\RegistrySize\{}",
                identity
                    .path()
                    .file_name()
                    .expect("fixture should have a name")
                    .to_string_lossy()
            );
            let root = RegKey::predef(HKEY_CURRENT_USER);
            root.create_subkey(&path)
                .expect("fixture root should be created");
            Self {
                _identity: identity,
                root,
                path,
            }
        }

        fn view(&self, name: &str) -> (RegKey, RegKey) {
            // Separate fixture roots model independent views without modifying installed apps.
            let (root, _) = self
                .root
                .create_subkey(format!(r"{}\{name}", self.path))
                .expect("fixture view should be created");
            let (uninstall, _) = root
                .create_subkey(super::super::UNINSTALL_PATH)
                .expect("fixture uninstall path should be created");
            (root, uninstall)
        }
    }

    impl Drop for RegistryFixture {
        fn drop(&mut self) {
            let _ = self.root.delete_subkey_all(&self.path);
        }
    }

    fn fixture_entry(uninstall: &RegKey, name: &str, size: u32) -> RegKey {
        let (entry, _) = uninstall
            .create_subkey(name)
            .expect("fixture entry should be created");
        for (field, value) in [
            ("DisplayName", "Example"),
            ("DisplayVersion", "1.2.3"),
            ("Publisher", "Example Vendor"),
        ] {
            entry
                .set_value(field, &value)
                .expect("identity should be written");
        }
        entry
            .set_value("WindowsInstaller", &1_u32)
            .expect("MSI marker should be written");
        entry
            .set_value("EstimatedSize", &size)
            .expect("estimate should be written");
        entry
            .set_value("UninstallString", &format!("MsiExec.exe /X {PRODUCT_CODE}"))
            .expect("command should be written");
        entry
    }

    fn merged_inventory_bytes(roots: [&RegKey; 2]) -> u64 {
        use super::super::{read_uninstall_view, RegistryScope};
        use std::collections::{HashMap, HashSet};
        use winreg::enums::KEY_WOW64_32KEY;

        let mut applications = HashMap::new();
        let mut conflicts = HashSet::new();
        let mut sizes = HashMap::new();
        for (root, view) in roots.into_iter().zip([KEY_WOW64_64KEY, KEY_WOW64_32KEY]) {
            assert_eq!(
                read_uninstall_view(
                    root,
                    view,
                    RegistryScope::Machine,
                    &mut applications,
                    &mut conflicts,
                    &mut sizes
                ),
                (true, true)
            );
        }
        let application = applications
            .get("machine:exampleapplication")
            .expect("duplicate aliases should be merged into one application");
        assert!(
            application.uninstall_registration.is_none(),
            "display correction must not authorize MSI alias execution"
        );
        application.estimated_bytes
    }

    #[test]
    fn inventory_merge_preserves_correction_in_both_view_orders() {
        let fixture = RegistryFixture::new();
        let (first_root, first) = fixture.view("first");
        let (second_root, second) = fixture.view("second");
        let _first_alias = fixture_entry(&first, "ExampleApplication", 74_748_942);
        let second_alias = fixture_entry(&second, "ExampleApplication", 74_748_942);
        let _product = fixture_entry(&first, PRODUCT_CODE, 73_236);
        second_alias
            .set_value("DisplayName", &" EXAMPLE ")
            .expect("normalized identity should be written");
        for roots in [[&first_root, &second_root], [&second_root, &first_root]] {
            assert_eq!(merged_inventory_bytes(roots), 74_993_664);
        }
        first
            .delete_subkey_all(PRODUCT_CODE)
            .expect("product fixture should be removed");
        assert_eq!(
            merged_inventory_bytes([&first_root, &second_root]),
            76_542_916_608,
            "aliases without any corroboration must retain their original estimate"
        );
    }

    #[test]
    fn inventory_merge_retains_original_max_for_identity_and_product_conflicts() {
        let fixture = RegistryFixture::new();
        let (first_root, first) = fixture.view("first");
        let (second_root, second) = fixture.view("second");
        let _first_alias = fixture_entry(&first, "ExampleApplication", 74_748_942);
        let second_alias = fixture_entry(&second, "ExampleApplication", 74_748_942);
        let _product = fixture_entry(&first, PRODUCT_CODE, 73_236);
        for field in [
            "DisplayName",
            "DisplayVersion",
            "Publisher",
            "UninstallString",
        ] {
            let original: String = second_alias
                .get_value(field)
                .expect("original identity should exist");
            second_alias
                .set_value(field, &"Other")
                .expect("conflicting identity should be written");
            for roots in [[&first_root, &second_root], [&second_root, &first_root]] {
                assert_eq!(
                    merged_inventory_bytes(roots),
                    76_542_916_608,
                    "conflicting {field} must disable correction"
                );
            }
            second_alias
                .set_value(field, &original)
                .expect("identity should be restored");
        }
        second_alias
            .set_value("EstimatedSize", &73_236_u32)
            .expect("healthy estimate should be written");
        for roots in [[&first_root, &second_root], [&second_root, &first_root]] {
            assert_eq!(
                merged_inventory_bytes(roots),
                76_542_916_608,
                "different raw units must not inherit correction"
            );
        }
        second_alias
            .set_value("EstimatedSize", &74_748_942_u32)
            .expect("original estimate should be restored");
        let second_product = fixture_entry(&second, PRODUCT_CODE, 73_235);
        for roots in [[&first_root, &second_root], [&second_root, &first_root]] {
            assert_eq!(
                merged_inventory_bytes(roots),
                76_542_916_608,
                "conflicting corroborated sizes must disable correction"
            );
        }
        second_product
            .set_value("Publisher", &"Other")
            .expect("contradictory product identity should be written");
        assert_eq!(
            merged_inventory_bytes([&first_root, &second_root]),
            76_542_916_608
        );
        second_product
            .set_value("Publisher", &"Example Vendor")
            .expect("product identity should be restored");
        let first_alias = first
            .open_subkey_with_flags("ExampleApplication", winreg::enums::KEY_ALL_ACCESS)
            .expect("first alias should exist");
        let first_product = first
            .open_subkey_with_flags(PRODUCT_CODE, winreg::enums::KEY_ALL_ACCESS)
            .expect("first product should exist");
        for size in [73_236_u32, 74_748_942_u32] {
            for entry in [&first_alias, &second_alias, &first_product, &second_product] {
                entry
                    .set_value("EstimatedSize", &size)
                    .expect("healthy estimate should be written");
            }
            for roots in [[&first_root, &second_root], [&second_root, &first_root]] {
                assert_eq!(
                    merged_inventory_bytes(roots),
                    u64::from(size) * 1024,
                    "consistent KB estimates, including large apps, must retain their size"
                );
            }
        }
    }

    #[test]
    fn size_merge_does_not_restore_conflicts_or_cross_registry_scopes() {
        use super::{MsiAliasIdentity, MsiAliasSizeEvidence, RegistryScope};
        let verified = RegistrySizeEstimate {
            original_bytes: 76_542_916_608,
            alias: Some(MsiAliasSizeEvidence {
                identity: MsiAliasIdentity {
                    scope: RegistryScope::Machine,
                    product_code: PRODUCT_CODE.to_string(),
                    display_name: "example".to_string(),
                    display_version: "1.2.3".to_string(),
                    publisher: "example vendor".to_string(),
                    raw_size: 74_748_942,
                },
                corrected_bytes: Some(74_993_664),
            }),
        };
        let mut other_scope = verified.clone();
        other_scope
            .alias
            .as_mut()
            .expect("alias should exist")
            .identity
            .scope = RegistryScope::CurrentUser;
        for (mut left, right) in [
            (verified.clone(), other_scope.clone()),
            (other_scope, verified.clone()),
        ] {
            left.merge(&right);
            left.merge(&verified);
            assert_eq!(
                left.bytes(),
                76_542_916_608,
                "a later observation must not restore conflicting evidence"
            );
        }
        let mut ordinary = RegistrySizeEstimate {
            original_bytes: 80_000_000_000,
            alias: None,
        };
        ordinary.merge(&verified);
        assert_eq!(
            ordinary.bytes(),
            80_000_000_000,
            "ordinary large applications retain the maximum policy"
        );
        let fallback = RegistrySizeEstimate {
            original_bytes: 0,
            alias: None,
        }
        .with_fallback(1234);
        assert_eq!(
            fallback.bytes(),
            1234,
            "directory fallback must remain available"
        );
    }
}

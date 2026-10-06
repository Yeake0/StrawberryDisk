use objc2::{rc::autoreleasepool, runtime::AnyObject};
use objc2_foundation::{NSArray, NSData, NSString, NSURLPathKey, NSURLResourceKey, NSURL};

use super::*;

/// Monterey stores login bookmarks in a per-user v2 archive instead of the v13 database.
/// Read only the reachable login items; never rewrite the system archive to remove a record.
pub(super) fn read_records(
    cancellation: &PlatformCancellation,
) -> Result<(Vec<BackgroundAppRecord>, Option<u64>), ParseError> {
    let path = dirs::home_dir().ok_or(ParseError::Unsupported)?.join(
        "Library/Application Support/com.apple.backgroundtaskmanagementagent/backgrounditems.btm",
    );
    let metadata = fs::symlink_metadata(&path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            ParseError::Unsupported
        } else {
            io_parse_error(error)
        }
    })?;
    if !metadata.is_file() || metadata.len() > MAX_DATABASE_BYTES {
        return Err(ParseError::InvalidData);
    }
    let bytes = fs::read(&path).map_err(io_parse_error)?;
    let archive = Value::from_reader(Cursor::new(bytes)).map_err(|_| ParseError::InvalidData)?;
    let records = parse(&archive, cancellation, bookmark_path)?;
    log::debug!(
        "startup_legacy_login_archive_read version=2 item_count={} path={}",
        records.len(),
        crate::diagnostics::text(&path.to_string_lossy())
    );
    Ok((records, metadata.modified().ok().and_then(system_time_ms)))
}

fn object<'a>(objects: &'a [Value], reference: &Value) -> Option<&'a Value> {
    objects.get(usize::try_from(reference.as_uid()?.get()).ok()?)
}

fn dictionary_value<'a>(
    objects: &'a [Value],
    dictionary: &Dictionary,
    key: &str,
) -> Option<&'a Value> {
    let keys = dictionary.get("NS.keys")?.as_array()?;
    let values = dictionary.get("NS.objects")?.as_array()?;
    if keys.len() != values.len() {
        return None;
    }
    keys.iter().zip(values).find_map(|(name, value)| {
        (object(objects, name)?.as_string()? == key)
            .then(|| object(objects, value))
            .flatten()
    })
}

fn is_class(objects: &[Value], dictionary: &Dictionary, name: &str) -> bool {
    referenced_value(objects, dictionary, "$class")
        .and_then(Value::as_dictionary)
        .and_then(|class| class.get("$classname"))
        .and_then(Value::as_string)
        == Some(name)
}

fn members<'a>(objects: &'a [Value], dictionary: &Dictionary, key: &str) -> Option<&'a [Value]> {
    referenced_value(objects, dictionary, key)?
        .as_dictionary()?
        .get("NS.objects")?
        .as_array()
        .map(Vec::as_slice)
}

fn parse(
    archive: &Value,
    cancellation: &PlatformCancellation,
    path_from_bookmark: impl Fn(&[u8]) -> Option<PathBuf>,
) -> Result<Vec<BackgroundAppRecord>, ParseError> {
    let root = archive.as_dictionary().ok_or(ParseError::InvalidData)?;
    if root.get("$archiver").and_then(Value::as_string) != Some("NSKeyedArchiver") {
        return Err(ParseError::InvalidData);
    }
    let objects = root
        .get("$objects")
        .and_then(Value::as_array)
        .ok_or(ParseError::InvalidData)?;
    let top = root
        .get("$top")
        .and_then(Value::as_dictionary)
        .and_then(|top| top.get("root"))
        .and_then(|value| object(objects, value))
        .and_then(Value::as_dictionary)
        .ok_or(ParseError::InvalidData)?;
    if dictionary_value(objects, top, "version").and_then(Value::as_unsigned_integer) != Some(2) {
        return Err(ParseError::Unsupported);
    }
    let background = dictionary_value(objects, top, "backgroundItems")
        .and_then(Value::as_dictionary)
        .ok_or(ParseError::InvalidData)?;
    let containers =
        members(objects, background, "allContainers").ok_or(ParseError::InvalidData)?;
    let mut records = Vec::new();
    let mut seen = BTreeSet::new();
    for reference in containers {
        let container = object(objects, reference)
            .and_then(Value::as_dictionary)
            .ok_or(ParseError::InvalidData)?;
        if !is_class(objects, container, "BackgroundItemContainer") {
            return Err(ParseError::InvalidData);
        }
        let items = members(objects, container, "internalItems").ok_or(ParseError::InvalidData)?;
        for reference in items {
            if cancellation.is_cancelled() {
                return Err(ParseError::Cancelled);
            }
            let item = object(objects, reference)
                .and_then(Value::as_dictionary)
                .ok_or(ParseError::InvalidData)?;
            if !is_class(objects, item, "BackgroundLoginItem") {
                continue;
            }
            let bookmark = referenced_value(objects, item, "bookmark")
                .and_then(Value::as_dictionary)
                .ok_or(ParseError::InvalidData)?;
            let identifier = referenced_value(objects, bookmark, "identifier")
                .and_then(Value::as_dictionary)
                .and_then(|value| value.get("NS.uuidbytes"))
                .and_then(Value::as_data)
                .filter(|bytes| bytes.len() == 16)
                .ok_or(ParseError::InvalidData)?;
            let identifier = identifier
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            if !seen.insert(identifier.clone()) {
                return Err(ParseError::InvalidData);
            }
            let data = referenced_value(objects, bookmark, "data")
                .and_then(Value::as_data)
                .ok_or(ParseError::InvalidData)?;
            let path = path_from_bookmark(data)
                .filter(|path| path.is_absolute())
                .ok_or(ParseError::InvalidData)?;
            if !path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("app"))
            {
                continue;
            }
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or(ParseError::InvalidData)?
                .to_owned();
            records.push(BackgroundAppRecord {
                identifier: format!("legacy:{identifier}"),
                // The bookmark UUID is not a v13 shared-list identity. Do not manufacture a
                // removable native ID from its prefix or match missing records by name alone.
                uuid: None,
                bundle_identifier: String::new(),
                name,
                developer_name: None,
                path,
                disposition: 0,
                modified_at: None,
            });
        }
    }
    Ok(records)
}

fn bookmark_path(bookmark: &[u8]) -> Option<PathBuf> {
    autoreleasepool(|_| {
        // Foundation reads the stored resource values without resolving targets or mounting
        // volumes, which also retains the original path after an application is uninstalled.
        let data =
            unsafe { NSData::dataWithBytes_length(bookmark.as_ptr().cast(), bookmark.len()) };
        let path_key = unsafe { NSURLPathKey };
        let keys = NSArray::<NSURLResourceKey>::from_slice(&[path_key]);
        NSURL::resourceValuesForKeys_fromBookmarkData(&keys, &data)
            .and_then(|values| values.objectForKey(path_key))
            .and_then(|value: objc2::rc::Retained<AnyObject>| {
                value
                    .downcast_ref::<NSString>()
                    .map(|path| PathBuf::from(path.to_string()))
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use plist::Uid;

    fn dictionary(values: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
        Value::Dictionary(
            values
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value))
                .collect(),
        )
    }

    fn insert(objects: &mut Vec<Value>, value: Value) -> Value {
        let reference = Value::Uid(Uid::new(objects.len() as u64));
        objects.push(value);
        reference
    }

    fn fixture(paths: &[&str], version: u64) -> Value {
        let mut objects = vec![Value::String("$null".into())];
        let container_class = insert(
            &mut objects,
            dictionary([(
                "$classname",
                Value::String("BackgroundItemContainer".into()),
            )]),
        );
        let item_class = insert(
            &mut objects,
            dictionary([("$classname", Value::String("BackgroundLoginItem".into()))]),
        );
        let mut containers = Vec::new();
        for (index, path) in paths.iter().enumerate() {
            let identity = insert(
                &mut objects,
                dictionary([("NS.uuidbytes", Value::Data(vec![index as u8 + 1; 16]))]),
            );
            let data = insert(&mut objects, Value::Data(path.as_bytes().to_vec()));
            let bookmark = insert(
                &mut objects,
                dictionary([("identifier", identity), ("data", data)]),
            );
            let item = insert(
                &mut objects,
                dictionary([("$class", item_class.clone()), ("bookmark", bookmark)]),
            );
            let items = insert(
                &mut objects,
                dictionary([("NS.objects", Value::Array(vec![item]))]),
            );
            let container = insert(
                &mut objects,
                dictionary([
                    ("$class", container_class.clone()),
                    ("internalItems", items),
                ]),
            );
            containers.push(container);
        }
        let containers = insert(
            &mut objects,
            dictionary([("NS.objects", Value::Array(containers))]),
        );
        let background = insert(&mut objects, dictionary([("allContainers", containers)]));
        let version_key = insert(&mut objects, Value::String("version".into()));
        let background_key = insert(&mut objects, Value::String("backgroundItems".into()));
        let version = insert(&mut objects, Value::Integer(version.into()));
        let root = insert(
            &mut objects,
            dictionary([
                ("NS.keys", Value::Array(vec![version_key, background_key])),
                ("NS.objects", Value::Array(vec![version, background])),
            ]),
        );
        dictionary([
            ("$archiver", Value::String("NSKeyedArchiver".into())),
            ("$top", dictionary([("root", root)])),
            ("$objects", Value::Array(objects)),
        ])
    }

    fn decoded_path(data: &[u8]) -> Option<PathBuf> {
        String::from_utf8(data.to_vec()).ok().map(PathBuf::from)
    }

    #[test]
    fn preserves_same_name_orphans_as_distinct_non_removable_records() {
        let archive = fixture(
            &["/missing/a/Exampleé & %.app", "/missing/b/Exampleé & %.app"],
            2,
        );
        let records = parse(&archive, &PlatformCancellation::new(|| false), decoded_path).unwrap();
        assert_eq!(records.len(), 2);
        assert_ne!(records[0].identifier, records[1].identifier);
        assert_eq!(records[0].name, records[1].name);
        assert!(records.iter().all(|record| record.uuid.is_none()));
        let artifacts = records
            .into_iter()
            .map(|record| artifact_from_record(record, None, Some(&BTreeSet::new())))
            .collect::<Vec<_>>();
        assert_ne!(
            artifacts[0].owner.identity_key,
            artifacts[1].owner.identity_key
        );
        assert!(artifacts.iter().all(|artifact| artifact.control_capability
            == PlatformStartupControlCapability::SystemManaged));
    }

    #[test]
    fn rejects_unknown_version_invalid_bookmarks_and_relative_paths() {
        let cancellation = PlatformCancellation::new(|| false);
        assert_eq!(
            parse(&fixture(&[], 3), &cancellation, decoded_path),
            Err(ParseError::Unsupported)
        );
        assert_eq!(
            parse(&fixture(&["/missing/App.app"], 2), &cancellation, |_| None),
            Err(ParseError::InvalidData)
        );
        assert_eq!(
            parse(
                &fixture(&["relative/App.app"], 2),
                &cancellation,
                decoded_path
            ),
            Err(ParseError::InvalidData)
        );
    }

    #[test]
    fn preserves_offline_volume_paths_without_authorizing_removal() {
        let records = parse(
            &fixture(&["/Volumes/Offline/App.app"], 2),
            &PlatformCancellation::new(|| false),
            decoded_path,
        )
        .unwrap();
        assert_eq!(records[0].path, PathBuf::from("/Volumes/Offline/App.app"));
        assert!(match_missing_item(&records[0], &records, &[]).is_none());
    }

    #[test]
    fn cancellation_stops_legacy_archive_traversal() {
        assert_eq!(
            parse(
                &fixture(&["/missing/App.app"], 2),
                &PlatformCancellation::new(|| true),
                decoded_path
            ),
            Err(ParseError::Cancelled)
        );
    }
}

use std::{
    fs,
    os::windows::fs::MetadataExt,
    path::{Path, PathBuf},
    time::Duration,
};

use rusqlite::{Connection, OpenFlags, TransactionBehavior};
use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

use crate::{
    vscode_history, PlatformError, PlatformErrorCode, PlatformPrivacyApplicationNativeTraceKind,
    PlatformPrivacyDetailEntry, PlatformResult,
};

const MAX_TEXT_HISTORY_BYTES: u64 = 8 * 1024 * 1024;
const MAX_HISTORY_ENTRIES: usize = 50_000;
const MEDIA_PLAYER_FILE_ITEM_TYPE: i64 = 5;

#[derive(Debug)]
pub(super) struct Snapshot {
    pub(super) item_count: u64,
    pub(super) revision: String,
}

#[derive(Debug)]
struct HistoryRecords {
    labels: Vec<String>,
    revision: String,
}

pub(super) fn snapshot(
    trace: PlatformPrivacyApplicationNativeTraceKind,
) -> Option<PlatformResult<Snapshot>> {
    let result = match trace {
        PlatformPrivacyApplicationNativeTraceKind::WindowsModernMediaPlayerRecentMedia => {
            media_player_database().and_then(|path| media_player_records(&path))
        }
        PlatformPrivacyApplicationNativeTraceKind::WindowsVlcRecentMedia => {
            roaming_file("vlc/vlc-qt-interface.ini").and_then(|path| vlc_records(&path))
        }
        PlatformPrivacyApplicationNativeTraceKind::WindowsPotPlayerRecentMedia => {
            potplayer_playlist_paths().and_then(|paths| potplayer_records(&paths))
        }
        _ => return None,
    };
    Some(result.map(|records| Snapshot {
        item_count: records.labels.len() as u64,
        revision: records.revision,
    }))
}

pub(super) fn details(
    trace: PlatformPrivacyApplicationNativeTraceKind,
    offset: u64,
    limit: u32,
) -> Option<PlatformResult<Vec<PlatformPrivacyDetailEntry>>> {
    let records = match trace {
        PlatformPrivacyApplicationNativeTraceKind::WindowsModernMediaPlayerRecentMedia => {
            media_player_database().and_then(|path| media_player_records(&path))
        }
        PlatformPrivacyApplicationNativeTraceKind::WindowsVlcRecentMedia => {
            roaming_file("vlc/vlc-qt-interface.ini").and_then(|path| vlc_records(&path))
        }
        PlatformPrivacyApplicationNativeTraceKind::WindowsPotPlayerRecentMedia => {
            potplayer_playlist_paths().and_then(|paths| potplayer_records(&paths))
        }
        _ => return None,
    };
    Some(records.map(|records| page(records.labels, offset, limit)))
}

pub(super) fn clear(
    trace: PlatformPrivacyApplicationNativeTraceKind,
) -> Option<PlatformResult<bool>> {
    match trace {
        PlatformPrivacyApplicationNativeTraceKind::WindowsModernMediaPlayerRecentMedia => {
            Some(media_player_database().and_then(|path| clear_media_player(&path)))
        }
        PlatformPrivacyApplicationNativeTraceKind::WindowsVlcRecentMedia => {
            Some(roaming_file("vlc/vlc-qt-interface.ini").and_then(|path| clear_vlc(&path)))
        }
        PlatformPrivacyApplicationNativeTraceKind::WindowsPotPlayerRecentMedia => {
            Some(potplayer_playlist_paths().and_then(|paths| clear_potplayer(&paths)))
        }
        _ => None,
    }
}

fn media_player_database() -> PlatformResult<PathBuf> {
    dirs::data_local_dir()
        .map(|root| {
            root.join("Packages/Microsoft.ZuneMusic_8wekyb3d8bbwe/LocalState/MediaPlayer.db")
        })
        .ok_or_else(|| PlatformError::invalid_path("local application data is unavailable"))
}

fn roaming_file(relative: &str) -> PlatformResult<PathBuf> {
    dirs::data_dir()
        .map(|root| root.join(relative))
        .ok_or_else(|| PlatformError::invalid_path("roaming application data is unavailable"))
}

fn potplayer_playlist_paths() -> PlatformResult<Vec<PathBuf>> {
    let roaming = dirs::data_dir()
        .ok_or_else(|| PlatformError::invalid_path("roaming application data is unavailable"))?;
    Ok([
        roaming.join("PotPlayerMini64/Playlist/PotPlayerMini64.dpl"),
        roaming.join("PotPlayerMini/Playlist/PotPlayerMini.dpl"),
    ]
    .into_iter()
    .collect())
}

/// The current Media Player package retains its home-page cards in `RecentlyPlayed`. Item type 5
/// is the local-file source verified against the package's `File` table. Restricting every query
/// and deletion to that type avoids treating the indexed music/video library as playback history.
fn media_player_records(path: &Path) -> PlatformResult<HistoryRecords> {
    let Some(_) = safe_file_metadata(path)? else {
        return Ok(empty_records(b"windows-media-player-v1"));
    };
    let connection = open_media_player_database(path, true)?;
    connection
        .busy_timeout(Duration::from_secs(1))
        .map_err(database_error)?;
    let mut statement = connection
        .prepare(
            "SELECT r.Id, r.ItemId, r.PlayedTime, f.Uri \
             FROM RecentlyPlayed r \
             INNER JOIN File f ON f.Id = r.ItemId \
             WHERE r.ItemType = ?1 \
             ORDER BY r.PlayedTime DESC, r.Id DESC",
        )
        .map_err(database_error)?;
    let rows = statement
        .query_map([MEDIA_PLAYER_FILE_ITEM_TYPE], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(database_error)?;
    let mut labels = Vec::new();
    let mut revision = blake3::Hasher::new();
    revision.update(b"mangodisk-windows-media-player-v1\0");
    for row in rows {
        let (id, item_id, played_time, uri) = row.map_err(database_error)?;
        if labels.len() >= MAX_HISTORY_ENTRIES {
            return Err(history_limit_error());
        }
        revision.update(&id.to_le_bytes());
        revision.update(&item_id.to_le_bytes());
        revision.update(&played_time.to_le_bytes());
        revision.update(uri.as_bytes());
        labels.push(vscode_history::resource_label(&uri));
    }
    Ok(HistoryRecords {
        labels,
        revision: revision.finalize().to_hex().to_string(),
    })
}

fn clear_media_player(path: &Path) -> PlatformResult<bool> {
    let Some(_) = safe_file_metadata(path)? else {
        return Ok(true);
    };
    let mut connection = open_media_player_database(path, false)?;
    connection
        .busy_timeout(Duration::from_secs(1))
        .map_err(database_error)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(database_error)?;
    let removed = transaction
        .execute(
            "DELETE FROM RecentlyPlayed WHERE ItemType = ?1",
            [MEDIA_PLAYER_FILE_ITEM_TYPE],
        )
        .map_err(|error| database_error(error).with_possible_side_effects())?;
    transaction
        .commit()
        .map_err(|error| database_error(error).with_possible_side_effects())?;
    let remaining = media_player_records(path)?.labels.len();
    log::info!(
        "windows_media_player_history_cleared removed_count={removed} remaining_count={remaining}"
    );
    Ok(remaining == 0)
}

fn open_media_player_database(path: &Path, read_only: bool) -> PlatformResult<Connection> {
    let flags = if read_only {
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX
    } else {
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX
    };
    Connection::open_with_flags(path, flags).map_err(database_error)
}

fn database_error(error: rusqlite::Error) -> PlatformError {
    PlatformError::new(
        PlatformErrorCode::InvalidData,
        format!("media history database operation failed: {error}"),
    )
}

fn vlc_records(path: &Path) -> PlatformResult<HistoryRecords> {
    let Some(bytes) = read_text_history(path)? else {
        return Ok(empty_records(b"windows-vlc-v1"));
    };
    let text = decode_text_history(&bytes, "VLC history")?;
    let mut labels = Vec::new();
    let mut in_recent_section = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_recent_section = line.eq_ignore_ascii_case("[RecentsMRL]");
            continue;
        }
        if !in_recent_section {
            continue;
        }
        let Some(value) = line.strip_prefix("list=") else {
            continue;
        };
        for item in value
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
        {
            if labels.len() >= MAX_HISTORY_ENTRIES {
                return Err(history_limit_error());
            }
            labels.push(vscode_history::resource_label(item.trim_matches('"')));
        }
    }
    Ok(records_from_bytes(b"windows-vlc-v1", labels, &bytes))
}

fn clear_vlc(path: &Path) -> PlatformResult<bool> {
    let Some(bytes) = read_text_history(path)? else {
        return Ok(true);
    };
    let text = decode_text_history(&bytes, "VLC history")?;
    let rewritten = rewrite_ini_section_values(text, "RecentsMRL", &["list", "times"]);
    if rewritten.as_bytes() != bytes_without_bom(&bytes) {
        write_text_history(path, &bytes, &rewritten)?;
    }
    let remaining = vlc_records(path)?.labels.len();
    log::info!("windows_vlc_history_cleared remaining_count={remaining}");
    Ok(remaining == 0)
}

fn potplayer_records(paths: &[PathBuf]) -> PlatformResult<HistoryRecords> {
    let mut labels = Vec::new();
    let mut revision = blake3::Hasher::new();
    revision.update(b"mangodisk-windows-potplayer-v1\0");
    for path in paths {
        let Some(bytes) = read_text_history(path)? else {
            continue;
        };
        revision.update(blake3::hash(&bytes).as_bytes());
        let text = decode_text_history(&bytes, "PotPlayer playlist")?;
        for line in text.lines() {
            let mut fields = line.splitn(3, '*');
            let index = fields.next().unwrap_or_default();
            let kind = fields.next().unwrap_or_default();
            let value = fields.next().unwrap_or_default().trim();
            if index.bytes().all(|byte| byte.is_ascii_digit())
                && !index.is_empty()
                && kind.eq_ignore_ascii_case("file")
                && !value.is_empty()
            {
                if labels.len() >= MAX_HISTORY_ENTRIES {
                    return Err(history_limit_error());
                }
                labels.push(vscode_history::resource_label(value));
            }
        }
    }
    Ok(HistoryRecords {
        labels,
        revision: revision.finalize().to_hex().to_string(),
    })
}

fn clear_potplayer(paths: &[PathBuf]) -> PlatformResult<bool> {
    let mut changed_file_count = 0_u64;
    for path in paths {
        let Some(bytes) = read_text_history(path)? else {
            continue;
        };
        let text = decode_text_history(&bytes, "PotPlayer playlist")?;
        let rewritten = rewrite_potplayer_playlist(text);
        if rewritten.as_bytes() != bytes_without_bom(&bytes) {
            write_text_history(path, &bytes, &rewritten)?;
            changed_file_count = changed_file_count.saturating_add(1);
        }
    }
    let remaining = potplayer_records(paths)?.labels.len();
    log::info!(
        "windows_potplayer_history_cleared changed_file_count={changed_file_count} remaining_count={remaining}"
    );
    Ok(remaining == 0)
}

fn read_text_history(path: &Path) -> PlatformResult<Option<Vec<u8>>> {
    let Some(metadata) = safe_file_metadata(path)? else {
        return Ok(None);
    };
    if metadata.len() > MAX_TEXT_HISTORY_BYTES {
        return Err(PlatformError::new(
            PlatformErrorCode::InvalidData,
            "media history file exceeds the supported size",
        ));
    }
    fs::read(path)
        .map(Some)
        .map_err(|error| PlatformError::io("read media history", &error))
}

fn safe_file_metadata(path: &Path) -> PlatformResult<Option<fs::Metadata>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(PlatformError::io("inspect media history", &error)),
    };
    if !metadata.is_file() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(PlatformError::invalid_path(
            "media history source is not a safe regular file",
        ));
    }
    Ok(Some(metadata))
}

fn decode_text_history<'a>(bytes: &'a [u8], source: &str) -> PlatformResult<&'a str> {
    std::str::from_utf8(bytes_without_bom(bytes)).map_err(|_| {
        PlatformError::new(
            PlatformErrorCode::InvalidData,
            format!("{source} is not valid UTF-8"),
        )
    })
}

fn bytes_without_bom(bytes: &[u8]) -> &[u8] {
    bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes)
}

fn write_text_history(path: &Path, original: &[u8], text: &str) -> PlatformResult<()> {
    let mut bytes = Vec::with_capacity(text.len() + 3);
    if original.starts_with(&[0xEF, 0xBB, 0xBF]) {
        bytes.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    }
    bytes.extend_from_slice(text.as_bytes());
    fs::write(path, bytes).map_err(|error| {
        PlatformError::io("write media history", &error).with_possible_side_effects()
    })
}

fn rewrite_ini_section_values(text: &str, section: &str, keys: &[&str]) -> String {
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let ended_with_newline = text.ends_with('\n');
    let mut in_target_section = false;
    let mut output = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_target_section = trimmed[1..trimmed.len() - 1].eq_ignore_ascii_case(section);
        }
        if in_target_section {
            if let Some((key, _)) = trimmed.split_once('=') {
                if keys
                    .iter()
                    .any(|candidate| key.eq_ignore_ascii_case(candidate))
                {
                    output.push(format!("{key}="));
                    continue;
                }
            }
        }
        output.push(line.to_owned());
    }
    let mut rewritten = output.join(newline);
    if ended_with_newline {
        rewritten.push_str(newline);
    }
    rewritten
}

fn rewrite_potplayer_playlist(text: &str) -> String {
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let ended_with_newline = text.ends_with('\n');
    let mut output = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        let numbered_record = trimmed.split_once('*').is_some_and(|(prefix, _)| {
            !prefix.is_empty() && prefix.bytes().all(|b| b.is_ascii_digit())
        });
        if numbered_record {
            continue;
        }
        if trimmed.starts_with("playname=") {
            output.push("playname=".to_owned());
        } else if trimmed.starts_with("playtime=") {
            output.push("playtime=0".to_owned());
        } else if trimmed.starts_with("topindex=") {
            output.push("topindex=0".to_owned());
        } else {
            output.push(line.to_owned());
        }
    }
    let mut rewritten = output.join(newline);
    if ended_with_newline {
        rewritten.push_str(newline);
    }
    rewritten
}

fn records_from_bytes(domain: &[u8], labels: Vec<String>, bytes: &[u8]) -> HistoryRecords {
    let mut revision = blake3::Hasher::new();
    revision.update(domain);
    revision.update(b"\0");
    revision.update(blake3::hash(bytes).as_bytes());
    HistoryRecords {
        labels,
        revision: revision.finalize().to_hex().to_string(),
    }
}

fn empty_records(domain: &[u8]) -> HistoryRecords {
    records_from_bytes(domain, Vec::new(), &[])
}

fn history_limit_error() -> PlatformError {
    PlatformError::new(
        PlatformErrorCode::InvalidData,
        "media history exceeds the supported entry count",
    )
}

fn page(labels: Vec<String>, offset: u64, limit: u32) -> Vec<PlatformPrivacyDetailEntry> {
    labels
        .into_iter()
        .skip(usize::try_from(offset).unwrap_or(usize::MAX))
        .take(limit as usize)
        .map(|label| PlatformPrivacyDetailEntry {
            label,
            item_count: 1,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeSet,
        sync::atomic::{AtomicU64, Ordering},
    };

    use super::*;

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

    fn fixture_root() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "strawberrydisk-media-history-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn vlc_counts_uri_records_and_preserves_unrelated_settings_when_cleared() {
        let root = fixture_root();
        let path = root.join("vlc-qt-interface.ini");
        fs::write(
            &path,
            "[MainWindow]\r\nplaylist-visible=true\r\n[RecentsMRL]\r\nlist=file:///C:/One%20File.mp4,file://server/share/Two.mp3\r\ntimes=1,2\r\n",
        )
        .unwrap();

        let records = vlc_records(&path).unwrap();
        assert_eq!(
            records.labels,
            [r"C:\One File.mp4", r"\\server\share\Two.mp3"]
        );
        assert!(clear_vlc(&path).unwrap());
        let rewritten = fs::read_to_string(&path).unwrap();
        assert!(rewritten.contains("playlist-visible=true"));
        assert!(rewritten.contains("list=\r\n"));
        assert!(rewritten.contains("times=\r\n"));
        assert!(vlc_records(&path).unwrap().labels.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn potplayer_lists_only_file_records_and_clears_the_default_playlist() {
        let root = fixture_root();
        let path = root.join("PotPlayerMini64.dpl");
        fs::write(
            &path,
            "\u{feff}DAUMPLAYLIST\r\nplayname=C:\\Video\\One.mp4\r\nplaytime=42\r\ntopindex=1\r\nsaveplaypos=0\r\n1*file*C:\\Video\\One.mp4\r\n1*duration2*123\r\n2*file*\\\\server\\Two.mkv\r\n",
        )
        .unwrap();

        let records = potplayer_records(std::slice::from_ref(&path)).unwrap();
        assert_eq!(records.labels, [r"C:\Video\One.mp4", r"\\server\Two.mkv"]);
        assert!(clear_potplayer(std::slice::from_ref(&path)).unwrap());
        let rewritten = fs::read_to_string(&path).unwrap();
        assert!(rewritten.contains("saveplaypos=0"));
        assert!(!rewritten.contains("*file*"));
        assert!(potplayer_records(&[path]).unwrap().labels.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn media_player_clears_recent_files_without_deleting_the_indexed_library() {
        let root = fixture_root();
        let path = root.join("MediaPlayer.db");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE File (Uri TEXT, Id INTEGER PRIMARY KEY, Version INTEGER); \
                 CREATE TABLE RecentlyPlayed (ItemId INTEGER, ItemType INTEGER, PlayedTime INTEGER, Id INTEGER PRIMARY KEY, Version INTEGER); \
                 INSERT INTO File VALUES ('C:\\Media\\One.mp4', 1, 0); \
                 INSERT INTO RecentlyPlayed VALUES (1, 5, 100, 1, 0); \
                 INSERT INTO RecentlyPlayed VALUES (99, 4, 101, 2, 0);",
            )
            .unwrap();
        drop(connection);

        let records = media_player_records(&path).unwrap();
        assert_eq!(records.labels, [r"C:\Media\One.mp4"]);
        assert!(clear_media_player(&path).unwrap());
        let connection = Connection::open(&path).unwrap();
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM File", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM RecentlyPlayed WHERE ItemType = 4",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            1
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn duplicate_labels_remain_distinct_history_records() {
        let entries = page(vec!["same".into(), "same".into()], 0, 10);
        let unique = entries
            .iter()
            .map(|entry| &entry.label)
            .collect::<BTreeSet<_>>();
        assert_eq!(entries.len(), 2);
        assert_eq!(unique.len(), 1);
    }
}

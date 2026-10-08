use std::{
    ffi::OsString,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::UNIX_EPOCH,
};

use rusqlite::{Connection, OpenFlags};
use strawberrydisk_platform::PlatformPrivacyBrowserKind;

use crate::{CoreError, CoreErrorReason, CoreResult};

use super::{PrivacyDataKind, PrivacyDetailEntry, PrivacyTimeRange};

const CHROMIUM_EPOCH_OFFSET_MS: i64 = 11_644_473_600_000;
const SAFARI_EPOCH_OFFSET_SECONDS: i64 = 978_307_200;
const MAX_SNAPSHOT_DATABASE_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_SNAPSHOT_COMPANION_BYTES: u64 = 512 * 1024 * 1024;
const MAX_FIREFOX_LOGIN_JSON_BYTES: u64 = 64 * 1024 * 1024;
const MAX_DETAIL_LABEL_CHARS: usize = 512;
const SNAPSHOT_ATTEMPTS: usize = 3;

static NEXT_SNAPSHOT: AtomicU64 = AtomicU64::new(1);

pub(super) fn count(
    path: &Path,
    browser: PlatformPrivacyBrowserKind,
    kind: PrivacyDataKind,
    range: PrivacyTimeRange,
    now_ms: u64,
) -> CoreResult<u64> {
    let database_path = safe_database_path(path)?;
    if browser == PlatformPrivacyBrowserKind::Firefox && kind == PrivacyDataKind::SavedPasswords {
        return count_firefox_saved_passwords(&database_path, range);
    }
    match count_at_path(&database_path, browser, kind, range, now_ms, false) {
        Err(error) if error.reason() == Some(CoreErrorReason::ResourceBusy) => {
            // Active browsers can hold an exclusive read lock even though the committed database
            // remains safe to inspect. A short-lived private snapshot lets scanning continue while
            // keeping every destructive operation gated behind browser shutdown and a fresh scan.
            log::debug!("privacy_database_snapshot_fallback reason=resource_busy");
            let snapshot = DatabaseSnapshot::capture(&database_path)?;
            count_at_path(snapshot.path(), browser, kind, range, now_ms, true)
        }
        result => result,
    }
}

pub(super) fn details(
    path: &Path,
    browser: PlatformPrivacyBrowserKind,
    kind: PrivacyDataKind,
    range: PrivacyTimeRange,
    now_ms: u64,
    offset: u64,
    limit: u32,
) -> CoreResult<Vec<PrivacyDetailEntry>> {
    let database_path = safe_database_path(path)?;
    if browser == PlatformPrivacyBrowserKind::Firefox && kind == PrivacyDataKind::SavedPasswords {
        return firefox_saved_password_details(&database_path, offset, limit);
    }
    match details_at_path(
        &database_path,
        browser,
        kind,
        range,
        now_ms,
        offset,
        limit,
        false,
    ) {
        Err(error) if error.reason() == Some(CoreErrorReason::ResourceBusy) => {
            // Detail reads use the same private snapshot fallback as aggregate counting. The copy
            // is deleted before this call returns and no private label is written to diagnostics.
            log::debug!("privacy_database_detail_snapshot_fallback reason=resource_busy");
            let snapshot = DatabaseSnapshot::capture(&database_path)?;
            details_at_path(
                snapshot.path(),
                browser,
                kind,
                range,
                now_ms,
                offset,
                limit,
                true,
            )
        }
        result => result,
    }
}

#[allow(clippy::too_many_arguments)]
fn details_at_path(
    database_path: &Path,
    browser: PlatformPrivacyBrowserKind,
    kind: PrivacyDataKind,
    range: PrivacyTimeRange,
    now_ms: u64,
    offset: u64,
    limit: u32,
    writable_snapshot: bool,
) -> CoreResult<Vec<PrivacyDetailEntry>> {
    let access_flag = if writable_snapshot {
        OpenFlags::SQLITE_OPEN_READ_WRITE
    } else {
        OpenFlags::SQLITE_OPEN_READ_ONLY
    };
    let connection = Connection::open_with_flags(
        database_path,
        access_flag | OpenFlags::SQLITE_OPEN_NO_MUTEX | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )
    .map_err(database_error)?;
    connection
        .busy_timeout(std::time::Duration::from_millis(250))
        .map_err(database_error)?;
    let cutoff = cutoff_for(range, browser, now_ms);
    let sql = detail_query(browser, kind)?;
    let mut statement = connection.prepare(sql).map_err(database_error)?;
    let limit = i64::from(limit);
    let offset = i64::try_from(offset)
        .map_err(|_| CoreError::invalid_input("privacy detail offset is too large"))?;
    let entries = statement
        .query_map(rusqlite::params![cutoff, limit, offset], |row| {
            let label = row.get::<_, Option<String>>(0)?.unwrap_or_default();
            let item_count = row.get::<_, i64>(1)?;
            Ok(PrivacyDetailEntry {
                label: sanitize_detail_label(&label),
                item_count: u64::try_from(item_count.max(0)).unwrap_or(0),
            })
        })
        .map_err(database_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(database_error)?;
    Ok(entries)
}

fn detail_query(
    browser: PlatformPrivacyBrowserKind,
    kind: PrivacyDataKind,
) -> CoreResult<&'static str> {
    let sql = match (browser, kind) {
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::BrowsingHistory) => {
            "SELECT urls.url, COUNT(*) FROM visits JOIN urls ON urls.id = visits.url WHERE (?1 IS NULL OR visits.visit_time >= ?1) GROUP BY urls.url ORDER BY MAX(visits.visit_time) DESC, urls.url LIMIT ?2 OFFSET ?3"
        }
        (PlatformPrivacyBrowserKind::Firefox, PrivacyDataKind::BrowsingHistory) => {
            "SELECT moz_places.url, COUNT(*) FROM moz_historyvisits JOIN moz_places ON moz_places.id = moz_historyvisits.place_id WHERE moz_historyvisits.visit_type <> 7 AND (?1 IS NULL OR moz_historyvisits.visit_date >= ?1) GROUP BY moz_places.url ORDER BY MAX(moz_historyvisits.visit_date) DESC, moz_places.url LIMIT ?2 OFFSET ?3"
        }
        (PlatformPrivacyBrowserKind::Safari, PrivacyDataKind::BrowsingHistory) => {
            "SELECT history_items.url, COUNT(*) FROM history_visits JOIN history_items ON history_items.id = history_visits.history_item WHERE (?1 IS NULL OR history_visits.visit_time >= ?1) GROUP BY history_items.url ORDER BY MAX(history_visits.visit_time) DESC, history_items.url LIMIT ?2 OFFSET ?3"
        }
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::DownloadHistory) => {
            "SELECT target_path, COUNT(*) FROM downloads WHERE (?1 IS NULL OR start_time >= ?1) GROUP BY target_path ORDER BY MAX(start_time) DESC, target_path LIMIT ?2 OFFSET ?3"
        }
        (PlatformPrivacyBrowserKind::Firefox, PrivacyDataKind::DownloadHistory) => {
            "SELECT moz_places.url, COUNT(*) FROM moz_historyvisits JOIN moz_places ON moz_places.id = moz_historyvisits.place_id WHERE moz_historyvisits.visit_type = 7 AND (?1 IS NULL OR moz_historyvisits.visit_date >= ?1) GROUP BY moz_places.url ORDER BY MAX(moz_historyvisits.visit_date) DESC, moz_places.url LIMIT ?2 OFFSET ?3"
        }
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::SearchHistory) => {
            "SELECT keyword_search_terms.term, COUNT(*) FROM keyword_search_terms JOIN urls ON urls.id = keyword_search_terms.url_id WHERE (?1 IS NULL OR urls.last_visit_time >= ?1) GROUP BY keyword_search_terms.term ORDER BY MAX(urls.last_visit_time) DESC, keyword_search_terms.term LIMIT ?2 OFFSET ?3"
        }
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::Cookies) => {
            "SELECT host_key, COUNT(*) FROM cookies WHERE (?1 IS NULL OR last_access_utc >= ?1) GROUP BY host_key ORDER BY COUNT(*) DESC, host_key LIMIT ?2 OFFSET ?3"
        }
        (PlatformPrivacyBrowserKind::Firefox, PrivacyDataKind::Cookies) => {
            "SELECT host, COUNT(*) FROM moz_cookies WHERE (?1 IS NULL OR lastAccessed >= ?1) GROUP BY host ORDER BY COUNT(*) DESC, host LIMIT ?2 OFFSET ?3"
        }
        (PlatformPrivacyBrowserKind::Firefox, PrivacyDataKind::SitePermissions) => {
            "SELECT CASE WHEN type IS NULL OR type = '' THEN origin ELSE origin || ' · ' || type END, COUNT(*) FROM moz_perms GROUP BY origin, type ORDER BY origin, type LIMIT ?2 OFFSET ?3"
        }
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::SavedPasswords) => {
            "SELECT origin_url, COUNT(*) FROM logins GROUP BY origin_url ORDER BY origin_url LIMIT ?2 OFFSET ?3"
        }
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::AutofillData) => {
            "SELECT name, COUNT(*) FROM autofill GROUP BY name ORDER BY name LIMIT ?2 OFFSET ?3"
        }
        (PlatformPrivacyBrowserKind::Firefox, PrivacyDataKind::AutofillData) => {
            "SELECT fieldname, COUNT(*) FROM moz_formhistory GROUP BY fieldname ORDER BY fieldname LIMIT ?2 OFFSET ?3"
        }
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::FrequentlyVisitedSites) => {
            "SELECT url, COUNT(*) FROM top_sites GROUP BY url ORDER BY MIN(url_rank), url LIMIT ?2 OFFSET ?3"
        }
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::AddressBarShortcuts) => {
            "SELECT text, COUNT(*) FROM omni_box_shortcuts WHERE (?1 IS NULL OR last_access_time >= ?1) GROUP BY text ORDER BY MAX(last_access_time) DESC, text LIMIT ?2 OFFSET ?3"
        }
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::WebsiteIcons) => {
            "SELECT page_url, COUNT(*) FROM icon_mapping GROUP BY page_url ORDER BY page_url LIMIT ?2 OFFSET ?3"
        }
        _ => {
            return Err(CoreError::operation_failed(
                "privacy database details are unsupported",
            ));
        }
    };
    Ok(sql)
}

fn firefox_saved_password_details(
    path: &Path,
    offset: u64,
    limit: u32,
) -> CoreResult<Vec<PrivacyDetailEntry>> {
    let metadata = fs::metadata(path).map_err(snapshot_io_error)?;
    if metadata.len() > MAX_FIREFOX_LOGIN_JSON_BYTES {
        return Err(CoreError::operation_failed(
            "Firefox saved-password source exceeds the scan limit",
        ));
    }
    let document: FirefoxLoginDetailDocument = serde_json::from_reader(
        fs::File::open(path).map_err(snapshot_io_error)?,
    )
    .map_err(|_| CoreError::operation_failed("Firefox saved-password schema is unsupported"))?;
    let start = usize::try_from(offset).unwrap_or(usize::MAX);
    Ok(document
        .logins
        .into_iter()
        .skip(start)
        .take(limit as usize)
        .map(|login| PrivacyDetailEntry {
            label: sanitize_detail_label(&login.hostname),
            item_count: 1,
        })
        .collect())
}

#[derive(serde::Deserialize)]
struct FirefoxLoginDetailDocument {
    logins: Vec<FirefoxLoginDetail>,
}

#[derive(serde::Deserialize)]
struct FirefoxLoginDetail {
    hostname: String,
}

fn sanitize_detail_label(value: &str) -> String {
    value
        .trim()
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .take(MAX_DETAIL_LABEL_CHARS)
        .collect()
}

fn count_firefox_saved_passwords(path: &Path, range: PrivacyTimeRange) -> CoreResult<u64> {
    if range != PrivacyTimeRange::AllTime {
        return Err(CoreError::operation_failed(
            "Firefox saved-password time range is unsupported",
        ));
    }
    let metadata = fs::metadata(path).map_err(snapshot_io_error)?;
    if metadata.len() > MAX_FIREFOX_LOGIN_JSON_BYTES {
        return Err(CoreError::operation_failed(
            "Firefox saved-password source exceeds the scan limit",
        ));
    }
    // Stream the top-level login entries as ignored values so encrypted passwords, hosts, and
    // usernames are never retained by Core, written to logs, or exposed across the protocol.
    let document: FirefoxLoginDocument = serde_json::from_reader(
        fs::File::open(path).map_err(snapshot_io_error)?,
    )
    .map_err(|_| CoreError::operation_failed("Firefox saved-password schema is unsupported"))?;
    Ok(document.login_count)
}

#[derive(serde::Deserialize)]
struct FirefoxLoginDocument {
    #[serde(rename = "logins", deserialize_with = "deserialize_entry_count")]
    login_count: u64,
}

fn deserialize_entry_count<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{IgnoredAny, SeqAccess, Visitor};

    struct EntryCountVisitor;

    impl<'de> Visitor<'de> for EntryCountVisitor {
        type Value = u64;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("an array of credential records")
        }

        fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut count = 0_u64;
            while sequence.next_element::<IgnoredAny>()?.is_some() {
                count = count
                    .checked_add(1)
                    .ok_or_else(|| serde::de::Error::custom("credential count overflowed"))?;
            }
            Ok(count)
        }
    }

    deserializer.deserialize_seq(EntryCountVisitor)
}

fn count_at_path(
    database_path: &Path,
    browser: PlatformPrivacyBrowserKind,
    kind: PrivacyDataKind,
    range: PrivacyTimeRange,
    now_ms: u64,
    writable_snapshot: bool,
) -> CoreResult<u64> {
    let access_flag = if writable_snapshot {
        OpenFlags::SQLITE_OPEN_READ_WRITE
    } else {
        OpenFlags::SQLITE_OPEN_READ_ONLY
    };
    let connection = Connection::open_with_flags(
        database_path,
        access_flag | OpenFlags::SQLITE_OPEN_NO_MUTEX | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )
    .map_err(database_error)?;
    // Process detection is advisory and can race a browser startup. Bound read lock waits so an
    // unrecognized or newly started browser degrades this source instead of freezing the page.
    connection
        .busy_timeout(std::time::Duration::from_millis(250))
        .map_err(database_error)?;
    validate_count_schema(&connection, browser, kind)?;
    let cutoff = cutoff_for(range, browser, now_ms);
    let count: i64 = match (browser, kind, cutoff) {
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::BrowsingHistory, Some(value)) => {
            connection.query_row(
                "SELECT COUNT(*) FROM visits WHERE visit_time >= ?1",
                [value],
                |row| row.get(0),
            )
        }
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::BrowsingHistory, None) => {
            connection.query_row("SELECT COUNT(*) FROM visits", [], |row| row.get(0))
        }
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::SearchHistory, Some(value)) => {
            connection.query_row(
                "SELECT COUNT(*) FROM keyword_search_terms AS terms JOIN urls ON urls.id = terms.url_id WHERE urls.last_visit_time >= ?1",
                [value],
                |row| row.get(0),
            )
        }
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::SearchHistory, None) => {
            connection.query_row("SELECT COUNT(*) FROM keyword_search_terms", [], |row| row.get(0))
        }
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::DownloadHistory, Some(value)) => {
            connection.query_row(
                "SELECT COUNT(*) FROM downloads WHERE start_time >= ?1",
                [value],
                |row| row.get(0),
            )
        }
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::DownloadHistory, None) => {
            connection.query_row("SELECT COUNT(*) FROM downloads", [], |row| row.get(0))
        }
        (PlatformPrivacyBrowserKind::Firefox, PrivacyDataKind::BrowsingHistory, Some(value)) => {
            connection.query_row(
                "SELECT COUNT(*) FROM moz_historyvisits WHERE visit_type <> 7 AND visit_date >= ?1",
                [value],
                |row| row.get(0),
            )
        }
        (PlatformPrivacyBrowserKind::Firefox, PrivacyDataKind::BrowsingHistory, None) => connection
            .query_row(
                "SELECT COUNT(*) FROM moz_historyvisits WHERE visit_type <> 7",
                [],
                |row| row.get(0),
            ),
        (PlatformPrivacyBrowserKind::Firefox, PrivacyDataKind::DownloadHistory, Some(value)) => {
            connection.query_row(
                "SELECT COUNT(*) FROM moz_historyvisits WHERE visit_type = 7 AND visit_date >= ?1",
                [value],
                |row| row.get(0),
            )
        }
        (PlatformPrivacyBrowserKind::Firefox, PrivacyDataKind::DownloadHistory, None) => connection
            .query_row(
                "SELECT COUNT(*) FROM moz_historyvisits WHERE visit_type = 7",
                [],
                |row| row.get(0),
            ),
        (PlatformPrivacyBrowserKind::Safari, PrivacyDataKind::BrowsingHistory, Some(value)) => {
            connection.query_row(
                "SELECT COUNT(*) FROM history_visits WHERE visit_time >= ?1",
                [value],
                |row| row.get(0),
            )
        }
        (PlatformPrivacyBrowserKind::Safari, PrivacyDataKind::BrowsingHistory, None) => {
            connection.query_row("SELECT COUNT(*) FROM history_visits", [], |row| row.get(0))
        }
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::Cookies, Some(value)) => connection
            .query_row(
                "SELECT COUNT(*) FROM cookies WHERE last_access_utc >= ?1",
                [value],
                |row| row.get(0),
            ),
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::Cookies, None) => {
            connection.query_row("SELECT COUNT(*) FROM cookies", [], |row| row.get(0))
        }
        (PlatformPrivacyBrowserKind::Firefox, PrivacyDataKind::Cookies, Some(value)) => connection
            .query_row(
                "SELECT COUNT(*) FROM moz_cookies WHERE lastAccessed >= ?1",
                [value],
                |row| row.get(0),
            ),
        (PlatformPrivacyBrowserKind::Firefox, PrivacyDataKind::Cookies, None) => {
            connection.query_row("SELECT COUNT(*) FROM moz_cookies", [], |row| row.get(0))
        }
        (PlatformPrivacyBrowserKind::Firefox, PrivacyDataKind::SitePermissions, None) => {
            connection.query_row("SELECT COUNT(*) FROM moz_perms", [], |row| row.get(0))
        }
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::SavedPasswords, None) => {
            connection.query_row("SELECT COUNT(*) FROM logins", [], |row| row.get(0))
        }
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::AutofillData, None) => {
            connection.query_row("SELECT COUNT(*) FROM autofill", [], |row| row.get(0))
        }
        (PlatformPrivacyBrowserKind::Firefox, PrivacyDataKind::AutofillData, None) => {
            connection.query_row("SELECT COUNT(*) FROM moz_formhistory", [], |row| row.get(0))
        }
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::FrequentlyVisitedSites, None) => {
            connection.query_row("SELECT COUNT(*) FROM top_sites", [], |row| row.get(0))
        }
        (
            PlatformPrivacyBrowserKind::Chromium,
            PrivacyDataKind::AddressBarShortcuts,
            Some(value),
        ) => connection.query_row(
            "SELECT COUNT(*) FROM omni_box_shortcuts WHERE last_access_time >= ?1",
            [value],
            |row| row.get(0),
        ),
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::AddressBarShortcuts, None) => {
            connection.query_row("SELECT COUNT(*) FROM omni_box_shortcuts", [], |row| {
                row.get(0)
            })
        }
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::WebsiteIcons, None) => {
            connection.query_row("SELECT COUNT(*) FROM icon_mapping", [], |row| row.get(0))
        }
        _ => {
            return Err(CoreError::operation_failed(
                "privacy database operation is unsupported",
            ))
        }
    }
    .map_err(database_error)?;
    Ok(count.max(0) as u64)
}

pub(super) fn clear(
    path: &Path,
    browser: PlatformPrivacyBrowserKind,
    kind: PrivacyDataKind,
    range: PrivacyTimeRange,
    now_ms: u64,
) -> CoreResult<u64> {
    let database_path = safe_database_path(path)?;
    if browser == PlatformPrivacyBrowserKind::Firefox && kind == PrivacyDataKind::SavedPasswords {
        return clear_firefox_saved_passwords(&database_path, range);
    }
    let before = count(path, browser, kind, range, now_ms)?;
    if before == 0 {
        return Ok(0);
    }
    let mut connection = Connection::open_with_flags(
        database_path,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )
    .map_err(database_error)?;
    connection
        .busy_timeout(std::time::Duration::from_millis(750))
        .map_err(database_error)?;
    validate_cleanup_schema(&connection, browser, kind)?;
    let transaction = connection.transaction().map_err(database_error)?;
    let cutoff = cutoff_for(range, browser, now_ms);
    match (browser, kind, cutoff) {
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::BrowsingHistory, cutoff) => {
            clear_chromium_history(&transaction, cutoff)?;
        }
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::SearchHistory, Some(value)) => {
            transaction
                .execute(
                    "DELETE FROM keyword_search_terms WHERE url_id IN (SELECT id FROM urls WHERE last_visit_time >= ?1)",
                    [value],
                )
                .map_err(database_error)?;
        }
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::SearchHistory, None) => {
            transaction
                .execute("DELETE FROM keyword_search_terms", [])
                .map_err(database_error)?;
        }
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::DownloadHistory, Some(value)) => {
            transaction.execute("DELETE FROM downloads_url_chains WHERE id IN (SELECT id FROM downloads WHERE start_time >= ?1)", [value]).map_err(database_error)?;
            transaction
                .execute("DELETE FROM downloads WHERE start_time >= ?1", [value])
                .map_err(database_error)?;
        }
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::DownloadHistory, None) => {
            transaction
                .execute("DELETE FROM downloads_url_chains", [])
                .map_err(database_error)?;
            transaction
                .execute("DELETE FROM downloads", [])
                .map_err(database_error)?;
        }
        (PlatformPrivacyBrowserKind::Firefox, PrivacyDataKind::BrowsingHistory, cutoff) => {
            delete_firefox_visits(&transaction, cutoff, false)?;
        }
        (PlatformPrivacyBrowserKind::Firefox, PrivacyDataKind::DownloadHistory, cutoff) => {
            delete_firefox_visits(&transaction, cutoff, true)?;
        }
        (PlatformPrivacyBrowserKind::Safari, PrivacyDataKind::BrowsingHistory, Some(value)) => {
            transaction
                .execute("DELETE FROM history_visits WHERE visit_time >= ?1", [value])
                .map_err(database_error)?;
            transaction.execute("DELETE FROM history_items WHERE NOT EXISTS (SELECT 1 FROM history_visits WHERE history_visits.history_item = history_items.id)", []).map_err(database_error)?;
        }
        (PlatformPrivacyBrowserKind::Safari, PrivacyDataKind::BrowsingHistory, None) => {
            transaction
                .execute("DELETE FROM history_visits", [])
                .map_err(database_error)?;
            transaction
                .execute("DELETE FROM history_items", [])
                .map_err(database_error)?;
        }
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::Cookies, Some(value)) => {
            transaction
                .execute("DELETE FROM cookies WHERE last_access_utc >= ?1", [value])
                .map_err(database_error)?;
        }
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::Cookies, None) => {
            transaction
                .execute("DELETE FROM cookies", [])
                .map_err(database_error)?;
        }
        (PlatformPrivacyBrowserKind::Firefox, PrivacyDataKind::Cookies, Some(value)) => {
            transaction
                .execute("DELETE FROM moz_cookies WHERE lastAccessed >= ?1", [value])
                .map_err(database_error)?;
        }
        (PlatformPrivacyBrowserKind::Firefox, PrivacyDataKind::Cookies, None) => {
            transaction
                .execute("DELETE FROM moz_cookies", [])
                .map_err(database_error)?;
        }
        (PlatformPrivacyBrowserKind::Firefox, PrivacyDataKind::SitePermissions, None) => {
            transaction
                .execute("DELETE FROM moz_perms", [])
                .map_err(database_error)?;
        }
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::SavedPasswords, None) => {
            // Chromium has accumulated several optional derivative tables across releases. Clear
            // them before the primary login rows so no password notes, health findings, or login
            // statistics survive the user-requested credential removal.
            for table in [
                "insecure_credentials",
                "password_notes",
                "compromised_credentials",
                "stats",
            ] {
                delete_from_optional_table(
                    &transaction,
                    table,
                    &[],
                    &format!("DELETE FROM {table}"),
                )?;
            }
            transaction
                .execute("DELETE FROM logins", [])
                .map_err(database_error)?;
        }
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::AutofillData, None) => {
            // The scanned `autofill` table is form-entry history. Address profiles, payment cards,
            // and other Web Data tables are deliberately outside this item and remain untouched.
            transaction
                .execute("DELETE FROM autofill", [])
                .map_err(database_error)?;
        }
        (PlatformPrivacyBrowserKind::Firefox, PrivacyDataKind::AutofillData, None) => {
            transaction
                .execute("DELETE FROM moz_formhistory", [])
                .map_err(database_error)?;
        }
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::FrequentlyVisitedSites, None) => {
            transaction
                .execute("DELETE FROM top_sites", [])
                .map_err(database_error)?;
        }
        (
            PlatformPrivacyBrowserKind::Chromium,
            PrivacyDataKind::AddressBarShortcuts,
            Some(value),
        ) => {
            transaction
                .execute(
                    "DELETE FROM omni_box_shortcuts WHERE last_access_time >= ?1",
                    [value],
                )
                .map_err(database_error)?;
        }
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::AddressBarShortcuts, None) => {
            transaction
                .execute("DELETE FROM omni_box_shortcuts", [])
                .map_err(database_error)?;
        }
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::WebsiteIcons, None) => {
            // Icon mappings and bitmap payloads are derived from visited pages. Clear dependent
            // rows first, preserve the schema and meta table, and let Chromium repopulate icons.
            transaction
                .execute("DELETE FROM icon_mapping", [])
                .map_err(database_error)?;
            transaction
                .execute("DELETE FROM favicon_bitmaps", [])
                .map_err(database_error)?;
            transaction
                .execute("DELETE FROM favicons", [])
                .map_err(database_error)?;
        }
        _ => {
            return Err(CoreError::operation_failed(
                "privacy database operation is unsupported",
            ))
        }
    }
    transaction.commit().map_err(database_error)?;
    let remaining = count(path, browser, kind, range, now_ms)?;
    if remaining != 0 {
        return Err(CoreError::operation_failed(
            "privacy database verification failed",
        ));
    }
    Ok(before)
}

fn clear_firefox_saved_passwords(path: &Path, range: PrivacyTimeRange) -> CoreResult<u64> {
    let before = count_firefox_saved_passwords(path, range)?;
    if before == 0 {
        return Ok(0);
    }
    let metadata = fs::metadata(path).map_err(snapshot_io_error)?;
    if metadata.len() > MAX_FIREFOX_LOGIN_JSON_BYTES {
        return Err(CoreError::operation_failed(
            "Firefox saved-password source exceeds the cleanup limit",
        ));
    }
    // Firefox keeps encrypted credentials in the `logins` array alongside profile metadata.
    // Replace only that array, preserve every unrelated top-level field, and never log or expose
    // the parsed document. The replacement is synced before an atomic same-directory file swap.
    let mut document: serde_json::Value = serde_json::from_reader(
        fs::File::open(path).map_err(snapshot_io_error)?,
    )
    .map_err(|_| CoreError::operation_failed("Firefox saved-password schema is unsupported"))?;
    let logins = document
        .as_object_mut()
        .and_then(|object| object.get_mut("logins"))
        .and_then(serde_json::Value::as_array_mut)
        .ok_or_else(|| {
            CoreError::operation_failed("Firefox saved-password schema is unsupported")
        })?;
    logins.clear();
    let replacement = serde_json::to_vec(&document).map_err(|_| {
        CoreError::operation_failed("Firefox saved-password cleanup serialization failed")
    })?;
    write_atomic_replacement(path, &replacement, metadata.permissions())?;
    if count_firefox_saved_passwords(path, range)? != 0 {
        return Err(CoreError::operation_failed(
            "Firefox saved-password verification failed",
        ));
    }
    Ok(before)
}

fn write_atomic_replacement(
    path: &Path,
    content: &[u8],
    permissions: fs::Permissions,
) -> CoreResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| CoreError::operation_failed("privacy source parent is unavailable"))?;
    let temporary = parent.join(format!(
        ".strawberrydisk-privacy-{}-{}.tmp",
        std::process::id(),
        NEXT_SNAPSHOT.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| -> std::io::Result<()> {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        file.write_all(content)?;
        file.sync_all()?;
        fs::set_permissions(&temporary, permissions)?;
        replace_file(&temporary, path)
    })();
    if let Err(error) = result {
        let _ = fs::remove_file(&temporary);
        return Err(CoreError::operation_failed(format!(
            "privacy source replacement failed kind={:?}",
            error.kind()
        )));
    }
    Ok(())
}

#[cfg(not(windows))]
fn replace_file(source: &Path, target: &Path) -> std::io::Result<()> {
    fs::rename(source, target)
}

#[cfg(windows)]
fn replace_file(source: &Path, target: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source = source
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let target = target
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let succeeded = unsafe {
        MoveFileExW(
            source.as_ptr(),
            target.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if succeeded == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn clear_chromium_history(
    transaction: &rusqlite::Transaction<'_>,
    cutoff: Option<i64>,
) -> CoreResult<()> {
    // Chromium stores derived privacy data in several version-dependent tables. Capture the
    // selected visit IDs before deleting the primary rows, then remove only matching derivatives.
    // Optional tables are skipped when absent but fail closed when present with an unknown schema.
    transaction
        .execute_batch(
            "CREATE TEMP TABLE strawberrydisk_privacy_visits(
                 id INTEGER PRIMARY KEY
             ) WITHOUT ROWID;",
        )
        .map_err(database_error)?;
    match cutoff {
        Some(value) => transaction.execute(
            "INSERT INTO strawberrydisk_privacy_visits SELECT id FROM visits WHERE visit_time >= ?1",
            [value],
        ),
        None => transaction.execute(
            "INSERT INTO strawberrydisk_privacy_visits SELECT id FROM visits",
            [],
        ),
    }
    .map_err(database_error)?;

    delete_from_optional_table(
        transaction,
        "visit_source",
        &["id"],
        "DELETE FROM visit_source WHERE id IN (SELECT id FROM strawberrydisk_privacy_visits)",
    )?;
    for table in ["content_annotations", "context_annotations"] {
        delete_from_optional_table(
            transaction,
            table,
            &["visit_id"],
            &format!(
                "DELETE FROM {table} WHERE visit_id IN (SELECT id FROM strawberrydisk_privacy_visits)"
            ),
        )?;
    }
    delete_from_optional_table(
        transaction,
        "clusters_and_visits",
        &["cluster_id", "visit_id"],
        "DELETE FROM clusters_and_visits WHERE visit_id IN (SELECT id FROM strawberrydisk_privacy_visits)",
    )?;
    delete_from_optional_table(
        transaction,
        "cluster_visit_duplicates",
        &["visit_id", "duplicate_visit_id"],
        "DELETE FROM cluster_visit_duplicates WHERE visit_id IN (SELECT id FROM strawberrydisk_privacy_visits) OR duplicate_visit_id IN (SELECT id FROM strawberrydisk_privacy_visits)",
    )?;

    // Remaining visits can contain navigation links to a removed visit. Reset those aggregate
    // identifiers so Chromium does not retain a dangling trace or reopen a deleted relationship.
    if table_has_columns(transaction, "visits", &["from_visit"])? {
        transaction
            .execute(
                "UPDATE visits SET from_visit = 0 WHERE from_visit IN (SELECT id FROM strawberrydisk_privacy_visits)",
                [],
            )
            .map_err(database_error)?;
    }
    if table_has_columns(transaction, "visits", &["opener_visit"])? {
        transaction
            .execute(
                "UPDATE visits SET opener_visit = 0 WHERE opener_visit IN (SELECT id FROM strawberrydisk_privacy_visits)",
                [],
            )
            .map_err(database_error)?;
    }
    transaction
        .execute(
            "DELETE FROM visits WHERE id IN (SELECT id FROM strawberrydisk_privacy_visits)",
            [],
        )
        .map_err(database_error)?;

    if table_exists(transaction, "visited_links")? {
        require_table_columns(transaction, "visited_links", &["id"])?;
        require_table_columns(transaction, "visits", &["visited_link_id"])?;
        transaction
            .execute(
                "DELETE FROM visited_links WHERE NOT EXISTS (SELECT 1 FROM visits WHERE visits.visited_link_id = visited_links.id)",
                [],
            )
            .map_err(database_error)?;
    }
    if table_exists(transaction, "segments")? {
        require_table_columns(transaction, "segments", &["id", "url_id"])?;
        delete_from_optional_table(
            transaction,
            "segment_usage",
            &["segment_id"],
            "DELETE FROM segment_usage WHERE segment_id IN (SELECT segments.id FROM segments WHERE NOT EXISTS (SELECT 1 FROM visits WHERE visits.url = segments.url_id))",
        )?;
        transaction
            .execute(
                "DELETE FROM segments WHERE NOT EXISTS (SELECT 1 FROM visits WHERE visits.url = segments.url_id)",
                [],
            )
            .map_err(database_error)?;
    }
    delete_from_optional_table(
        transaction,
        "keyword_search_terms",
        &["url_id"],
        "DELETE FROM keyword_search_terms WHERE NOT EXISTS (SELECT 1 FROM visits WHERE visits.url = keyword_search_terms.url_id)",
    )?;
    transaction
        .execute(
            "DELETE FROM urls WHERE NOT EXISTS (SELECT 1 FROM visits WHERE visits.url = urls.id)",
            [],
        )
        .map_err(database_error)?;
    if table_exists(transaction, "clusters")? && table_exists(transaction, "clusters_and_visits")? {
        require_table_columns(transaction, "clusters", &["cluster_id"])?;
        require_table_columns(transaction, "clusters_and_visits", &["cluster_id"])?;
        transaction
            .execute(
                "DELETE FROM clusters WHERE NOT EXISTS (SELECT 1 FROM clusters_and_visits WHERE clusters_and_visits.cluster_id = clusters.cluster_id)",
                [],
            )
            .map_err(database_error)?;
    }
    Ok(())
}

fn delete_from_optional_table(
    connection: &Connection,
    table: &str,
    required_columns: &[&str],
    statement: &str,
) -> CoreResult<()> {
    if !table_exists(connection, table)? {
        return Ok(());
    }
    require_table_columns(connection, table, required_columns)?;
    connection.execute(statement, []).map_err(database_error)?;
    Ok(())
}

fn table_exists(connection: &Connection, table: &str) -> CoreResult<bool> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
            [table],
            |row| row.get::<_, bool>(0),
        )
        .map_err(database_error)
}

fn table_has_columns(
    connection: &Connection,
    table: &str,
    required_columns: &[&str],
) -> CoreResult<bool> {
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(database_error)?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(database_error)?
        .collect::<Result<std::collections::BTreeSet<_>, _>>()
        .map_err(database_error)?;
    Ok(required_columns
        .iter()
        .all(|column| columns.contains(*column)))
}

fn require_table_columns(
    connection: &Connection,
    table: &str,
    required_columns: &[&str],
) -> CoreResult<()> {
    if table_has_columns(connection, table, required_columns)? {
        Ok(())
    } else {
        Err(CoreError::operation_failed(
            "privacy database schema is unsupported",
        ))
    }
}

fn delete_firefox_visits(
    transaction: &rusqlite::Transaction<'_>,
    cutoff: Option<i64>,
    downloads: bool,
) -> CoreResult<()> {
    let comparison = if downloads { "=" } else { "<>" };
    let sql = match cutoff {
        Some(_) => format!(
            "DELETE FROM moz_historyvisits WHERE visit_type {comparison} 7 AND visit_date >= ?1"
        ),
        None => format!("DELETE FROM moz_historyvisits WHERE visit_type {comparison} 7"),
    };
    match cutoff {
        Some(value) => transaction.execute(&sql, [value]),
        None => transaction.execute(&sql, []),
    }
    .map_err(database_error)?;
    // Bookmark-owned places must survive even after their final history visit is removed.
    transaction.execute(
        "DELETE FROM moz_places WHERE foreign_count = 0 AND NOT EXISTS (SELECT 1 FROM moz_historyvisits WHERE moz_historyvisits.place_id = moz_places.id) AND NOT EXISTS (SELECT 1 FROM moz_bookmarks WHERE moz_bookmarks.fk = moz_places.id)",
        [],
    ).map_err(database_error)?;
    Ok(())
}

fn validate_count_schema(
    connection: &Connection,
    browser: PlatformPrivacyBrowserKind,
    kind: PrivacyDataKind,
) -> CoreResult<()> {
    let required = match (browser, kind) {
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::BrowsingHistory) => {
            &[("visits", &["visit_time"] as &[_])][..]
        }
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::SearchHistory) => &[
            ("keyword_search_terms", &["url_id"] as &[_]),
            ("urls", &["id", "last_visit_time"]),
        ][..],
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::DownloadHistory) => {
            &[("downloads", &["start_time"] as &[_])][..]
        }
        (
            PlatformPrivacyBrowserKind::Firefox,
            PrivacyDataKind::BrowsingHistory | PrivacyDataKind::DownloadHistory,
        ) => &[("moz_historyvisits", &["visit_type", "visit_date"] as &[_])][..],
        (PlatformPrivacyBrowserKind::Safari, PrivacyDataKind::BrowsingHistory) => {
            &[("history_visits", &["visit_time"] as &[_])][..]
        }
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::Cookies) => {
            &[("cookies", &["last_access_utc"] as &[_])][..]
        }
        (PlatformPrivacyBrowserKind::Firefox, PrivacyDataKind::Cookies) => {
            &[("moz_cookies", &["lastAccessed"] as &[_])][..]
        }
        (PlatformPrivacyBrowserKind::Firefox, PrivacyDataKind::SitePermissions) => {
            &[("moz_perms", &[] as &[_])][..]
        }
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::SavedPasswords) => {
            &[("logins", &[] as &[_])][..]
        }
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::AutofillData) => {
            &[("autofill", &[] as &[_])][..]
        }
        (PlatformPrivacyBrowserKind::Firefox, PrivacyDataKind::AutofillData) => {
            &[("moz_formhistory", &[] as &[_])][..]
        }
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::FrequentlyVisitedSites) => {
            &[("top_sites", &["url"] as &[_])][..]
        }
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::AddressBarShortcuts) => {
            &[("omni_box_shortcuts", &["last_access_time"] as &[_])][..]
        }
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::WebsiteIcons) => {
            &[("icon_mapping", &["icon_id"] as &[_])][..]
        }
        _ => {
            return Err(CoreError::operation_failed(
                "privacy database schema is unsupported",
            ))
        }
    };
    validate_required_tables(connection, required)
}

fn validate_cleanup_schema(
    connection: &Connection,
    browser: PlatformPrivacyBrowserKind,
    kind: PrivacyDataKind,
) -> CoreResult<()> {
    let required = match (browser, kind) {
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::BrowsingHistory) => &[
            ("visits", &["url", "visit_time"] as &[_]),
            ("urls", &["id"]),
        ][..],
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::SearchHistory) => &[
            ("keyword_search_terms", &["url_id"] as &[_]),
            ("urls", &["id", "last_visit_time"]),
        ][..],
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::DownloadHistory) => &[
            ("downloads", &["id", "start_time"] as &[_]),
            ("downloads_url_chains", &["id"]),
        ][..],
        (
            PlatformPrivacyBrowserKind::Firefox,
            PrivacyDataKind::BrowsingHistory | PrivacyDataKind::DownloadHistory,
        ) => &[
            (
                "moz_historyvisits",
                &["place_id", "visit_type", "visit_date"] as &[_],
            ),
            ("moz_places", &["id", "foreign_count"]),
            ("moz_bookmarks", &["fk"]),
        ][..],
        (PlatformPrivacyBrowserKind::Safari, PrivacyDataKind::BrowsingHistory) => &[
            ("history_visits", &["history_item", "visit_time"] as &[_]),
            ("history_items", &["id"]),
        ][..],
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::Cookies) => {
            &[("cookies", &["last_access_utc"] as &[_])][..]
        }
        (PlatformPrivacyBrowserKind::Firefox, PrivacyDataKind::Cookies) => {
            &[("moz_cookies", &["lastAccessed"] as &[_])][..]
        }
        (PlatformPrivacyBrowserKind::Firefox, PrivacyDataKind::SitePermissions) => {
            &[("moz_perms", &[] as &[_])][..]
        }
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::SavedPasswords) => {
            &[("logins", &[] as &[_])][..]
        }
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::AutofillData) => {
            &[("autofill", &[] as &[_])][..]
        }
        (PlatformPrivacyBrowserKind::Firefox, PrivacyDataKind::AutofillData) => {
            &[("moz_formhistory", &[] as &[_])][..]
        }
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::FrequentlyVisitedSites) => {
            &[("top_sites", &["url"] as &[_])][..]
        }
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::AddressBarShortcuts) => {
            &[("omni_box_shortcuts", &["last_access_time"] as &[_])][..]
        }
        (PlatformPrivacyBrowserKind::Chromium, PrivacyDataKind::WebsiteIcons) => &[
            ("icon_mapping", &["icon_id"] as &[_]),
            ("favicon_bitmaps", &["icon_id"]),
            ("favicons", &["id"]),
        ][..],
        _ => {
            return Err(CoreError::operation_failed(
                "privacy database schema is unsupported",
            ))
        }
    };
    validate_required_tables(connection, required)
}

fn validate_required_tables(
    connection: &Connection,
    required: &[(&str, &[&str])],
) -> CoreResult<()> {
    for (table, required_columns) in required {
        let exists: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .map_err(database_error)?;
        if exists != 1 {
            return Err(CoreError::operation_failed(
                "privacy database schema is unsupported",
            ));
        }
        let mut statement = connection
            .prepare(&format!("PRAGMA table_info({table})"))
            .map_err(database_error)?;
        let columns = statement
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(database_error)?
            .collect::<Result<std::collections::BTreeSet<_>, _>>()
            .map_err(database_error)?;
        if required_columns
            .iter()
            .any(|column| !columns.contains(*column))
        {
            return Err(CoreError::operation_failed(
                "privacy database schema is unsupported",
            ));
        }
    }
    Ok(())
}

/// Owns a sensitive, short-lived copy of a browser database and its WAL.
///
/// The snapshot content never crosses the Core protocol or enters logs. Its directory is private
/// to StrawberryDisk and removed on every return path. Source metadata is compared before and after
/// copying so a concurrent checkpoint or write causes a bounded retry instead of producing an
/// internally inconsistent snapshot.
struct DatabaseSnapshot {
    directory: PathBuf,
    path: PathBuf,
}

impl DatabaseSnapshot {
    fn capture(source: &Path) -> CoreResult<Self> {
        for attempt in 0..SNAPSHOT_ATTEMPTS {
            let directory = create_snapshot_directory()?;
            let path = directory.join("database.sqlite");
            match copy_stable_database(source, &path) {
                Ok(()) => return Ok(Self { directory, path }),
                Err(error) if attempt + 1 < SNAPSHOT_ATTEMPTS => {
                    remove_snapshot_directory(&directory)?;
                    log::debug!(
                        "privacy_database_snapshot_retry attempt={} error={}",
                        attempt + 1,
                        strawberrydisk_platform::diagnostics::text(&error)
                    );
                }
                Err(error) => {
                    remove_snapshot_directory(&directory)?;
                    return Err(error);
                }
            }
        }
        Err(CoreError::operation_failed(
            "privacy database snapshot attempts exhausted",
        ))
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for DatabaseSnapshot {
    fn drop(&mut self) {
        if let Err(error) = remove_snapshot_directory(&self.directory) {
            log::warn!("privacy_database_snapshot_cleanup_failed error={error}");
        }
    }
}

fn remove_snapshot_directory(directory: &Path) -> CoreResult<()> {
    match fs::remove_dir_all(directory) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(CoreError::operation_failed(format!(
            "privacy database snapshot cleanup failed kind={:?}",
            error.kind()
        ))),
    }
}

fn create_snapshot_directory() -> CoreResult<PathBuf> {
    // macOS commonly exposes its temporary root through `/var`, which is itself a symlink. SQLite
    // NOFOLLOW validates every path segment, so canonicalize the system-owned root before creating
    // the private child directory while still rejecting symlinks for database files themselves.
    let temporary_root = fs::canonicalize(std::env::temp_dir()).map_err(snapshot_io_error)?;
    for _ in 0..16 {
        let nonce = NEXT_SNAPSHOT.fetch_add(1, Ordering::Relaxed);
        let directory = temporary_root.join(format!(
            "strawberrydisk-privacy-scan-{}-{nonce}",
            std::process::id()
        ));
        match create_private_directory(&directory) {
            Ok(()) => return Ok(directory),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(CoreError::operation_failed(format!(
                    "privacy database snapshot directory failed kind={:?}",
                    error.kind()
                )))
            }
        }
    }
    Err(CoreError::operation_failed(
        "privacy database snapshot directory collision",
    ))
}

#[cfg(unix)]
fn create_private_directory(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;

    let mut builder = fs::DirBuilder::new();
    builder.mode(0o700).create(path)
}

#[cfg(not(unix))]
fn create_private_directory(path: &Path) -> std::io::Result<()> {
    fs::create_dir(path)
}

fn copy_stable_database(source: &Path, destination: &Path) -> CoreResult<()> {
    let wal_source = companion_path(source, "-wal");
    let before_database = snapshot_file_identity(source, MAX_SNAPSHOT_DATABASE_BYTES)?;
    let before_wal = optional_snapshot_file_identity(&wal_source, MAX_SNAPSHOT_COMPANION_BYTES)?;

    copy_snapshot_file(source, destination, MAX_SNAPSHOT_DATABASE_BYTES)?;
    if before_wal.is_some() {
        copy_snapshot_file(
            &wal_source,
            &companion_path(destination, "-wal"),
            MAX_SNAPSHOT_COMPANION_BYTES,
        )?;
    }

    let after_database = snapshot_file_identity(source, MAX_SNAPSHOT_DATABASE_BYTES)?;
    let after_wal = optional_snapshot_file_identity(&wal_source, MAX_SNAPSHOT_COMPANION_BYTES)?;
    if before_database != after_database || before_wal != after_wal {
        return Err(CoreError::operation_failed(
            "privacy database changed while snapshotting",
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SnapshotFileIdentity {
    bytes: u64,
    modified_ns: u128,
}

fn snapshot_file_identity(path: &Path, max_bytes: u64) -> CoreResult<SnapshotFileIdentity> {
    let metadata = fs::symlink_metadata(path).map_err(snapshot_io_error)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > max_bytes {
        return Err(CoreError::operation_failed(
            "privacy database snapshot source is unsafe",
        ));
    }
    let modified_ns = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_nanos())
        .unwrap_or(0);
    Ok(SnapshotFileIdentity {
        bytes: metadata.len(),
        modified_ns,
    })
}

fn optional_snapshot_file_identity(
    path: &Path,
    max_bytes: u64,
) -> CoreResult<Option<SnapshotFileIdentity>> {
    match fs::symlink_metadata(path) {
        Ok(_) => snapshot_file_identity(path, max_bytes).map(Some),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(snapshot_io_error(error)),
    }
}

fn copy_snapshot_file(source: &Path, destination: &Path, max_bytes: u64) -> CoreResult<()> {
    let expected = snapshot_file_identity(source, max_bytes)?.bytes;
    let copied = fs::copy(source, destination).map_err(snapshot_io_error)?;
    if copied != expected {
        return Err(CoreError::operation_failed(
            "privacy database snapshot copy was incomplete",
        ));
    }
    Ok(())
}

fn companion_path(path: &Path, suffix: &str) -> PathBuf {
    let mut value: OsString = path.as_os_str().to_owned();
    value.push(suffix);
    PathBuf::from(value)
}

fn snapshot_io_error(error: std::io::Error) -> CoreError {
    CoreError::operation_failed(format!(
        "privacy database snapshot failed kind={:?}",
        error.kind()
    ))
}

fn cutoff_for(
    range: PrivacyTimeRange,
    browser: PlatformPrivacyBrowserKind,
    now_ms: u64,
) -> Option<i64> {
    let cutoff_ms = match range {
        PrivacyTimeRange::LastHour => now_ms.saturating_sub(60 * 60 * 1_000),
        PrivacyTimeRange::Today => {
            local_midnight_ms(now_ms).unwrap_or_else(|| now_ms - (now_ms % 86_400_000))
        }
        PrivacyTimeRange::LastSevenDays => now_ms.saturating_sub(7 * 24 * 60 * 60 * 1_000),
        PrivacyTimeRange::AllTime => return None,
    } as i64;
    Some(match browser {
        PlatformPrivacyBrowserKind::Chromium => (cutoff_ms + CHROMIUM_EPOCH_OFFSET_MS) * 1_000,
        PlatformPrivacyBrowserKind::Firefox => cutoff_ms * 1_000,
        PlatformPrivacyBrowserKind::Safari => cutoff_ms / 1_000 - SAFARI_EPOCH_OFFSET_SECONDS,
    })
}

fn local_midnight_ms(now_ms: u64) -> Option<u64> {
    use chrono::{Local, TimeZone};

    let instant = Local.timestamp_millis_opt(now_ms as i64).single()?;
    let midnight = instant.date_naive().and_hms_opt(0, 0, 0)?;
    let local_midnight = Local.from_local_datetime(&midnight).earliest()?;
    u64::try_from(local_midnight.timestamp_millis()).ok()
}

fn safe_database_path(path: &Path) -> CoreResult<std::path::PathBuf> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| {
        CoreError::operation_failed(format!(
            "privacy database identity is unavailable kind={:?}",
            error.kind()
        ))
    })?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(CoreError::operation_failed(
            "privacy database identity is unsafe",
        ));
    }
    std::fs::canonicalize(path).map_err(|error| {
        CoreError::operation_failed(format!(
            "privacy database canonicalization failed kind={:?}",
            error.kind()
        ))
    })
}

fn database_error(error: rusqlite::Error) -> CoreError {
    let reason = match error.sqlite_error_code() {
        Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked) => {
            Some(CoreErrorReason::ResourceBusy)
        }
        _ => None,
    };
    let diagnostic = match error {
        rusqlite::Error::SqliteFailure(code, _) => format!(
            "privacy database operation failed code={:?} extended_code={}",
            code.code, code.extended_code
        ),
        _ => "privacy database operation failed".into(),
    };
    let error = CoreError::operation_failed(diagnostic);
    match reason {
        Some(reason) => error.with_reason(reason),
        None => error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

    struct DatabaseFixture {
        directory: PathBuf,
        path: PathBuf,
    }

    impl DatabaseFixture {
        fn new(name: &str) -> Self {
            let directory = std::env::temp_dir().join(format!(
                "strawberrydisk-privacy-{name}-{}-{}",
                std::process::id(),
                NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&directory).expect("privacy fixture directory must be created");
            let path = directory.join("database.sqlite");
            Self { directory, path }
        }
        fn connection(&self) -> Connection {
            Connection::open(&self.path).expect("privacy fixture database must open")
        }
    }

    impl Drop for DatabaseFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.directory);
        }
    }

    #[test]
    fn browser_epochs_use_expected_units() {
        let now = 1_700_000_000_000;
        assert_eq!(
            cutoff_for(
                PrivacyTimeRange::AllTime,
                PlatformPrivacyBrowserKind::Chromium,
                now
            ),
            None
        );
        assert_eq!(
            cutoff_for(
                PrivacyTimeRange::LastHour,
                PlatformPrivacyBrowserKind::Firefox,
                now
            ),
            Some((now as i64 - 3_600_000) * 1_000)
        );
        assert_eq!(
            cutoff_for(
                PrivacyTimeRange::LastHour,
                PlatformPrivacyBrowserKind::Safari,
                now
            ),
            Some((now as i64 - 3_600_000) / 1_000 - SAFARI_EPOCH_OFFSET_SECONDS)
        );
    }

    #[test]
    fn locked_browser_database_is_counted_from_private_snapshot() {
        let fixture = DatabaseFixture::new("locked-snapshot");
        let connection = fixture.connection();
        connection
            .execute_batch(
                "CREATE TABLE visits(id INTEGER PRIMARY KEY, visit_time INTEGER);
                 INSERT INTO visits VALUES (1, 1), (2, 2);
                 PRAGMA locking_mode = EXCLUSIVE;
                 BEGIN EXCLUSIVE;",
            )
            .unwrap();

        assert_eq!(
            count(
                &fixture.path,
                PlatformPrivacyBrowserKind::Chromium,
                PrivacyDataKind::BrowsingHistory,
                PrivacyTimeRange::AllTime,
                0,
            )
            .unwrap(),
            2
        );
        connection.execute_batch("ROLLBACK").unwrap();
    }

    #[test]
    fn snapshot_includes_committed_wal_records_and_is_removed_on_drop() {
        let fixture = DatabaseFixture::new("wal-snapshot");
        let connection = fixture.connection();
        connection
            .execute_batch(
                "PRAGMA journal_mode = WAL;
                 PRAGMA wal_autocheckpoint = 0;
                 CREATE TABLE visits(id INTEGER PRIMARY KEY, visit_time INTEGER);
                 INSERT INTO visits VALUES (1, 1), (2, 2), (3, 3);",
            )
            .unwrap();

        let snapshot = DatabaseSnapshot::capture(&fixture.path).unwrap();
        let snapshot_directory = snapshot.directory.clone();
        assert!(snapshot_directory.is_dir());
        assert_eq!(
            count_at_path(
                snapshot.path(),
                PlatformPrivacyBrowserKind::Chromium,
                PrivacyDataKind::BrowsingHistory,
                PrivacyTimeRange::AllTime,
                0,
                true,
            )
            .unwrap(),
            3
        );
        drop(snapshot);
        assert!(!snapshot_directory.exists());
    }

    #[test]
    fn count_schema_does_not_require_cleanup_only_tables() {
        let fixture = DatabaseFixture::new("minimal-count-schema");
        let connection = fixture.connection();
        connection
            .execute_batch(
                "CREATE TABLE downloads(start_time INTEGER);
                 INSERT INTO downloads VALUES (1), (2);",
            )
            .unwrap();
        drop(connection);

        assert_eq!(
            count(
                &fixture.path,
                PlatformPrivacyBrowserKind::Chromium,
                PrivacyDataKind::DownloadHistory,
                PrivacyTimeRange::AllTime,
                0,
            )
            .unwrap(),
            2
        );
        assert!(clear(
            &fixture.path,
            PlatformPrivacyBrowserKind::Chromium,
            PrivacyDataKind::DownloadHistory,
            PrivacyTimeRange::AllTime,
            0,
        )
        .is_err());
    }

    #[test]
    fn credential_and_autofill_scans_return_only_aggregate_counts() {
        let login_fixture = DatabaseFixture::new("credential-count");
        let login_connection = login_fixture.connection();
        login_connection
            .execute_batch(
                "CREATE TABLE logins(origin_url TEXT, password_value BLOB);
                 INSERT INTO logins VALUES ('one', X'01'), ('two', X'02');",
            )
            .unwrap();
        drop(login_connection);
        assert_eq!(
            count(
                &login_fixture.path,
                PlatformPrivacyBrowserKind::Chromium,
                PrivacyDataKind::SavedPasswords,
                PrivacyTimeRange::AllTime,
                0,
            )
            .unwrap(),
            2
        );

        let autofill_fixture = DatabaseFixture::new("autofill-count");
        let autofill_connection = autofill_fixture.connection();
        autofill_connection
            .execute_batch(
                "CREATE TABLE autofill(name TEXT, value TEXT);
                 INSERT INTO autofill VALUES ('one', 'private'), ('two', 'private');",
            )
            .unwrap();
        drop(autofill_connection);
        assert_eq!(
            count(
                &autofill_fixture.path,
                PlatformPrivacyBrowserKind::Chromium,
                PrivacyDataKind::AutofillData,
                PrivacyTimeRange::AllTime,
                0,
            )
            .unwrap(),
            2
        );

        let firefox_fixture = DatabaseFixture::new("firefox-credential-count");
        fs::write(
            &firefox_fixture.path,
            br#"{"nextId":4,"logins":[{"id":1},{"id":2},{"id":3}]}"#,
        )
        .unwrap();
        assert_eq!(
            count(
                &firefox_fixture.path,
                PlatformPrivacyBrowserKind::Firefox,
                PrivacyDataKind::SavedPasswords,
                PrivacyTimeRange::AllTime,
                0,
            )
            .unwrap(),
            3
        );
    }

    #[test]
    fn chromium_credential_cleanup_removes_password_rows_and_preserves_unrelated_data() {
        let fixture = DatabaseFixture::new("chromium-credential-cleanup");
        let connection = fixture.connection();
        connection
            .execute_batch(
                "CREATE TABLE logins(id INTEGER PRIMARY KEY, origin_url TEXT, password_value BLOB);
                 CREATE TABLE password_notes(id INTEGER PRIMARY KEY, parent_id INTEGER, value BLOB);
                 CREATE TABLE stats(id INTEGER PRIMARY KEY, origin_domain TEXT);
                 CREATE TABLE unrelated_preferences(id INTEGER PRIMARY KEY, value TEXT);
                 INSERT INTO logins VALUES (1, 'one', X'01'), (2, 'two', X'02');
                 INSERT INTO password_notes VALUES (1, 1, X'03');
                 INSERT INTO stats VALUES (1, 'one');
                 INSERT INTO unrelated_preferences VALUES (1, 'preserve');",
            )
            .unwrap();
        drop(connection);

        assert_eq!(
            clear(
                &fixture.path,
                PlatformPrivacyBrowserKind::Chromium,
                PrivacyDataKind::SavedPasswords,
                PrivacyTimeRange::AllTime,
                0,
            )
            .unwrap(),
            2
        );
        let connection = fixture.connection();
        for table in ["logins", "password_notes", "stats"] {
            let remaining = connection
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap();
            assert_eq!(remaining, 0, "{table} must be cleared");
        }
        assert_eq!(
            connection
                .query_row("SELECT value FROM unrelated_preferences", [], |row| {
                    row.get::<_, String>(0)
                })
                .unwrap(),
            "preserve"
        );
    }

    #[test]
    fn form_autofill_cleanup_preserves_profiles_and_unrelated_tables() {
        let chromium_fixture = DatabaseFixture::new("chromium-autofill-cleanup");
        let chromium_connection = chromium_fixture.connection();
        chromium_connection
            .execute_batch(
                "CREATE TABLE autofill(name TEXT, value TEXT);
                 CREATE TABLE autofill_profiles(guid TEXT PRIMARY KEY, full_name TEXT);
                 INSERT INTO autofill VALUES ('one', 'private'), ('two', 'private');
                 INSERT INTO autofill_profiles VALUES ('profile', 'preserve');",
            )
            .unwrap();
        drop(chromium_connection);
        assert_eq!(
            clear(
                &chromium_fixture.path,
                PlatformPrivacyBrowserKind::Chromium,
                PrivacyDataKind::AutofillData,
                PrivacyTimeRange::AllTime,
                0,
            )
            .unwrap(),
            2
        );
        let chromium_connection = chromium_fixture.connection();
        assert_eq!(
            chromium_connection
                .query_row("SELECT COUNT(*) FROM autofill_profiles", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            1
        );

        let firefox_fixture = DatabaseFixture::new("firefox-autofill-cleanup");
        let firefox_connection = firefox_fixture.connection();
        firefox_connection
            .execute_batch(
                "CREATE TABLE moz_formhistory(id INTEGER PRIMARY KEY, value TEXT);
                 CREATE TABLE unrelated_preferences(id INTEGER PRIMARY KEY, value TEXT);
                 INSERT INTO moz_formhistory VALUES (1, 'private'), (2, 'private');
                 INSERT INTO unrelated_preferences VALUES (1, 'preserve');",
            )
            .unwrap();
        drop(firefox_connection);
        assert_eq!(
            clear(
                &firefox_fixture.path,
                PlatformPrivacyBrowserKind::Firefox,
                PrivacyDataKind::AutofillData,
                PrivacyTimeRange::AllTime,
                0,
            )
            .unwrap(),
            2
        );
        let firefox_connection = firefox_fixture.connection();
        assert_eq!(
            firefox_connection
                .query_row("SELECT COUNT(*) FROM unrelated_preferences", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            1
        );
    }

    #[test]
    fn firefox_credential_cleanup_preserves_non_login_metadata() {
        let fixture = DatabaseFixture::new("firefox-credential-cleanup");
        fs::write(
            &fixture.path,
            br#"{"nextId":4,"logins":[{"id":1,"encryptedPassword":"private"},{"id":2,"encryptedPassword":"private"}],"disabledHosts":["preserve.invalid"],"version":3}"#,
        )
        .unwrap();

        assert_eq!(
            clear(
                &fixture.path,
                PlatformPrivacyBrowserKind::Firefox,
                PrivacyDataKind::SavedPasswords,
                PrivacyTimeRange::AllTime,
                0,
            )
            .unwrap(),
            2
        );
        let document: serde_json::Value =
            serde_json::from_slice(&fs::read(&fixture.path).unwrap()).unwrap();
        assert_eq!(document["logins"].as_array().unwrap().len(), 0);
        assert_eq!(document["nextId"], 4);
        assert_eq!(document["disabledHosts"][0], "preserve.invalid");
        assert_eq!(document["version"], 3);
    }

    #[test]
    fn chromium_history_range_preserves_downloads_and_older_visits() {
        let fixture = DatabaseFixture::new("chromium-history");
        let connection = fixture.connection();
        connection
            .execute_batch(
                "CREATE TABLE urls(id INTEGER PRIMARY KEY, url TEXT);
             CREATE TABLE visits(id INTEGER PRIMARY KEY, url INTEGER, visit_time INTEGER);
             CREATE TABLE downloads(id INTEGER PRIMARY KEY, start_time INTEGER);
             CREATE TABLE downloads_url_chains(id INTEGER, chain_index INTEGER, url TEXT);",
            )
            .unwrap();
        let now = 1_700_000_000_000_u64;
        let cutoff = cutoff_for(
            PrivacyTimeRange::LastHour,
            PlatformPrivacyBrowserKind::Chromium,
            now,
        )
        .unwrap();
        connection
            .execute(
                "INSERT INTO urls VALUES (1, 'https://old.invalid'), (2, 'https://new.invalid')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO visits VALUES (1, 1, ?1), (2, 2, ?2)",
                [cutoff - 1, cutoff + 1],
            )
            .unwrap();
        connection
            .execute("INSERT INTO downloads VALUES (7, ?1)", [cutoff + 1])
            .unwrap();
        drop(connection);

        assert_eq!(
            clear(
                &fixture.path,
                PlatformPrivacyBrowserKind::Chromium,
                PrivacyDataKind::BrowsingHistory,
                PrivacyTimeRange::LastHour,
                now
            )
            .unwrap(),
            1
        );
        let connection = fixture.connection();
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM visits", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM urls", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM downloads", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }

    #[test]
    fn chromium_history_cleanup_removes_matching_derived_records() {
        let fixture = DatabaseFixture::new("chromium-history-derived");
        let connection = fixture.connection();
        connection
            .execute_batch(
                "CREATE TABLE urls(id INTEGER PRIMARY KEY, url TEXT);
                 CREATE TABLE visits(
                     id INTEGER PRIMARY KEY,
                     url INTEGER,
                     visit_time INTEGER,
                     from_visit INTEGER,
                     opener_visit INTEGER,
                     visited_link_id INTEGER
                 );
                 CREATE TABLE visit_source(id INTEGER PRIMARY KEY, source INTEGER);
                 CREATE TABLE content_annotations(visit_id INTEGER PRIMARY KEY, search_terms TEXT);
                 CREATE TABLE context_annotations(visit_id INTEGER PRIMARY KEY, flags INTEGER);
                 CREATE TABLE clusters(cluster_id INTEGER PRIMARY KEY, label TEXT);
                 CREATE TABLE clusters_and_visits(cluster_id INTEGER, visit_id INTEGER);
                 CREATE TABLE cluster_visit_duplicates(visit_id INTEGER, duplicate_visit_id INTEGER);
                 CREATE TABLE keyword_search_terms(url_id INTEGER, term TEXT);
                 CREATE TABLE segments(id INTEGER PRIMARY KEY, url_id INTEGER);
                 CREATE TABLE segment_usage(id INTEGER PRIMARY KEY, segment_id INTEGER);
                 CREATE TABLE visited_links(id INTEGER PRIMARY KEY, top_level_url TEXT);
                 INSERT INTO urls VALUES (1, 'https://old.invalid'), (2, 'https://new.invalid');
                 INSERT INTO visit_source VALUES (1, 1), (2, 1);
                 INSERT INTO content_annotations VALUES (1, 'old'), (2, 'new');
                 INSERT INTO context_annotations VALUES (1, 0), (2, 0);
                 INSERT INTO clusters VALUES (10, 'old'), (20, 'new');
                 INSERT INTO clusters_and_visits VALUES (10, 1), (20, 2);
                 INSERT INTO cluster_visit_duplicates VALUES (1, 2);
                 INSERT INTO keyword_search_terms VALUES (1, 'old'), (2, 'new');
                 INSERT INTO segments VALUES (100, 1), (200, 2);
                 INSERT INTO segment_usage VALUES (1000, 100), (2000, 200);
                 INSERT INTO visited_links VALUES (10000, 'old'), (20000, 'new');",
            )
            .unwrap();
        let now = 1_700_000_000_000_u64;
        let cutoff = cutoff_for(
            PrivacyTimeRange::LastHour,
            PlatformPrivacyBrowserKind::Chromium,
            now,
        )
        .unwrap();
        connection
            .execute(
                "INSERT INTO visits VALUES (1, 1, ?1, 2, 2, 10000), (2, 2, ?2, 0, 0, 20000)",
                [cutoff - 1, cutoff + 1],
            )
            .unwrap();
        drop(connection);

        assert_eq!(
            clear(
                &fixture.path,
                PlatformPrivacyBrowserKind::Chromium,
                PrivacyDataKind::BrowsingHistory,
                PrivacyTimeRange::LastHour,
                now,
            )
            .unwrap(),
            1
        );
        let connection = fixture.connection();
        for table in [
            "urls",
            "visits",
            "visit_source",
            "content_annotations",
            "context_annotations",
            "clusters",
            "clusters_and_visits",
            "keyword_search_terms",
            "segments",
            "segment_usage",
            "visited_links",
        ] {
            let count = connection
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap();
            assert_eq!(count, 1, "{table} must retain only the older relationship");
        }
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM cluster_visit_duplicates", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            0
        );
        assert_eq!(
            connection
                .query_row("SELECT from_visit + opener_visit FROM visits", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            0
        );
    }

    #[test]
    fn chromium_download_cleanup_never_removes_downloaded_file() {
        let fixture = DatabaseFixture::new("chromium-download");
        let downloaded = fixture.directory.join("important-document.txt");
        fs::write(&downloaded, b"sentinel").unwrap();
        let connection = fixture.connection();
        connection
            .execute_batch(
                "CREATE TABLE downloads(id INTEGER PRIMARY KEY, start_time INTEGER);
             CREATE TABLE downloads_url_chains(id INTEGER, chain_index INTEGER, url TEXT);
             INSERT INTO downloads VALUES (1, 1);
             INSERT INTO downloads_url_chains VALUES (1, 0, 'https://download.invalid');",
            )
            .unwrap();
        drop(connection);

        assert_eq!(
            clear(
                &fixture.path,
                PlatformPrivacyBrowserKind::Chromium,
                PrivacyDataKind::DownloadHistory,
                PrivacyTimeRange::AllTime,
                0
            )
            .unwrap(),
            1
        );
        assert_eq!(fs::read(&downloaded).unwrap(), b"sentinel");
    }

    #[test]
    fn chromium_cookie_range_preserves_older_cookie_and_login_database() {
        let fixture = DatabaseFixture::new("chromium-cookies");
        let login_database = fixture.directory.join("Login Data");
        fs::write(&login_database, b"synthetic-password-marker").unwrap();
        let connection = fixture.connection();
        connection
            .execute_batch(
                "CREATE TABLE cookies(id INTEGER PRIMARY KEY, last_access_utc INTEGER, encrypted_value BLOB);",
            )
            .unwrap();
        let now = 1_700_000_000_000_u64;
        let cutoff = cutoff_for(
            PrivacyTimeRange::LastHour,
            PlatformPrivacyBrowserKind::Chromium,
            now,
        )
        .unwrap();
        connection
            .execute(
                "INSERT INTO cookies VALUES (1, ?1, X'01'), (2, ?2, X'02')",
                [cutoff - 1, cutoff + 1],
            )
            .unwrap();
        drop(connection);

        assert_eq!(
            clear(
                &fixture.path,
                PlatformPrivacyBrowserKind::Chromium,
                PrivacyDataKind::Cookies,
                PrivacyTimeRange::LastHour,
                now
            )
            .unwrap(),
            1
        );
        let connection = fixture.connection();
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM cookies WHERE id = 1", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            1
        );
        assert_eq!(
            fs::read(&login_database).unwrap(),
            b"synthetic-password-marker"
        );
    }

    #[test]
    fn firefox_history_preserves_bookmarks_and_download_visits() {
        let fixture = DatabaseFixture::new("firefox-history");
        let connection = fixture.connection();
        connection.execute_batch(
            "CREATE TABLE moz_places(id INTEGER PRIMARY KEY, url TEXT, foreign_count INTEGER NOT NULL DEFAULT 0);
             CREATE TABLE moz_historyvisits(id INTEGER PRIMARY KEY, place_id INTEGER, visit_type INTEGER, visit_date INTEGER);
             CREATE TABLE moz_bookmarks(id INTEGER PRIMARY KEY, fk INTEGER);
             INSERT INTO moz_places VALUES (1, 'https://history.invalid', 0), (2, 'https://bookmark.invalid', 1), (3, 'https://download.invalid', 0);
             INSERT INTO moz_historyvisits VALUES (1, 1, 1, 1), (2, 2, 1, 1), (3, 3, 7, 1);
             INSERT INTO moz_bookmarks VALUES (1, 2);",
        ).unwrap();
        drop(connection);

        assert_eq!(
            clear(
                &fixture.path,
                PlatformPrivacyBrowserKind::Firefox,
                PrivacyDataKind::BrowsingHistory,
                PrivacyTimeRange::AllTime,
                0
            )
            .unwrap(),
            2
        );
        let connection = fixture.connection();
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM moz_historyvisits WHERE visit_type = 7",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM moz_places WHERE id = 2", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            1
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM moz_bookmarks", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }

    #[test]
    fn firefox_site_permissions_are_independent_from_cookies() {
        let fixture = DatabaseFixture::new("firefox-permissions");
        let connection = fixture.connection();
        connection
            .execute_batch(
                "CREATE TABLE moz_perms(id INTEGER PRIMARY KEY, origin TEXT, type TEXT);
                 CREATE TABLE moz_cookies(id INTEGER PRIMARY KEY, lastAccessed INTEGER);
                 INSERT INTO moz_perms VALUES (1, 'https://permission.invalid', 'camera');
                 INSERT INTO moz_cookies VALUES (1, 1);",
            )
            .unwrap();
        drop(connection);

        assert_eq!(
            clear(
                &fixture.path,
                PlatformPrivacyBrowserKind::Firefox,
                PrivacyDataKind::SitePermissions,
                PrivacyTimeRange::AllTime,
                0
            )
            .unwrap(),
            1
        );
        let connection = fixture.connection();
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM moz_cookies", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }

    #[test]
    fn safari_history_range_preserves_older_visits() {
        let fixture = DatabaseFixture::new("safari-history");
        let connection = fixture.connection();
        connection
            .execute_batch(
                "CREATE TABLE history_items(id INTEGER PRIMARY KEY, url TEXT);
                 CREATE TABLE history_visits(id INTEGER PRIMARY KEY, history_item INTEGER, visit_time REAL);",
            )
            .unwrap();
        let now = 1_700_000_000_000_u64;
        let cutoff = cutoff_for(
            PrivacyTimeRange::LastHour,
            PlatformPrivacyBrowserKind::Safari,
            now,
        )
        .unwrap();
        connection
            .execute(
                "INSERT INTO history_items VALUES (1, 'https://old.invalid'), (2, 'https://new.invalid')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO history_visits VALUES (1, 1, ?1), (2, 2, ?2)",
                [cutoff - 1, cutoff + 1],
            )
            .unwrap();
        drop(connection);

        assert_eq!(
            clear(
                &fixture.path,
                PlatformPrivacyBrowserKind::Safari,
                PrivacyDataKind::BrowsingHistory,
                PrivacyTimeRange::LastHour,
                now
            )
            .unwrap(),
            1
        );
        let connection = fixture.connection();
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM history_visits", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM history_items", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }

    #[test]
    fn sqlite_wal_records_are_visible_without_profile_copy() {
        let fixture = DatabaseFixture::new("wal");
        let connection = fixture.connection();
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .unwrap();
        connection.execute_batch("CREATE TABLE urls(id INTEGER PRIMARY KEY, url TEXT); CREATE TABLE visits(id INTEGER PRIMARY KEY, url INTEGER, visit_time INTEGER); INSERT INTO urls VALUES (1, 'https://wal.invalid'); INSERT INTO visits VALUES (1, 1, 1);").unwrap();

        assert_eq!(
            count(
                &fixture.path,
                PlatformPrivacyBrowserKind::Chromium,
                PrivacyDataKind::BrowsingHistory,
                PrivacyTimeRange::AllTime,
                0
            )
            .unwrap(),
            1
        );
        assert!(
            fixture.path.with_extension("sqlite-wal").exists()
                || PathBuf::from(format!("{}-wal", fixture.path.display())).exists()
        );
    }

    #[test]
    #[ignore = "fixed 100,000-row privacy scan performance workload"]
    fn large_chromium_history_count_remains_aggregate_and_bounded() {
        let fixture = DatabaseFixture::new("large-history");
        let connection = fixture.connection();
        connection
            .execute_batch(
                "CREATE TABLE urls(id INTEGER PRIMARY KEY, url TEXT);
                 CREATE TABLE visits(id INTEGER PRIMARY KEY, url INTEGER, visit_time INTEGER);
                 INSERT INTO urls VALUES (1, 'https://aggregate.invalid');
                 WITH RECURSIVE sequence(value) AS (
                   SELECT 1 UNION ALL SELECT value + 1 FROM sequence WHERE value < 100000
                 )
                 INSERT INTO visits(id, url, visit_time) SELECT value, 1, value FROM sequence;",
            )
            .unwrap();
        drop(connection);

        let started = std::time::Instant::now();
        let item_count = count(
            &fixture.path,
            PlatformPrivacyBrowserKind::Chromium,
            PrivacyDataKind::BrowsingHistory,
            PrivacyTimeRange::AllTime,
            0,
        )
        .unwrap();
        println!(
            "privacy_large_history_count={} elapsed_ms={}",
            item_count,
            started.elapsed().as_millis()
        );
        assert_eq!(item_count, 100_000);
        assert!(started.elapsed() < std::time::Duration::from_secs(10));
    }

    #[test]
    fn failed_sqlite_transaction_rolls_back_history_deletion() {
        let fixture = DatabaseFixture::new("rollback");
        let connection = fixture.connection();
        connection.execute_batch(
            "CREATE TABLE urls(id INTEGER PRIMARY KEY, url TEXT);
             CREATE TABLE visits(id INTEGER PRIMARY KEY, url INTEGER, visit_time INTEGER);
             INSERT INTO urls VALUES (1, 'https://keep.invalid');
             INSERT INTO visits VALUES (1, 1, 1);
             CREATE TRIGGER block_visit_delete BEFORE DELETE ON visits BEGIN SELECT RAISE(ABORT, 'blocked'); END;",
        ).unwrap();
        drop(connection);

        assert!(clear(
            &fixture.path,
            PlatformPrivacyBrowserKind::Chromium,
            PrivacyDataKind::BrowsingHistory,
            PrivacyTimeRange::AllTime,
            0
        )
        .is_err());
        let connection = fixture.connection();
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM visits", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM urls", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            1
        );
    }

    #[test]
    fn chromium_top_sites_cleanup_preserves_unrelated_database_state() {
        let fixture = DatabaseFixture::new("top-sites");
        let connection = fixture.connection();
        connection
            .execute_batch(
                "CREATE TABLE top_sites(url TEXT PRIMARY KEY, url_rank INTEGER, title TEXT);
                 CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT);
                 INSERT INTO top_sites VALUES
                   ('https://first.invalid', 0, 'private'),
                   ('https://second.invalid', 1, 'private');
                 INSERT INTO meta VALUES ('version', 'preserve');",
            )
            .unwrap();
        drop(connection);

        assert_eq!(
            clear(
                &fixture.path,
                PlatformPrivacyBrowserKind::Chromium,
                PrivacyDataKind::FrequentlyVisitedSites,
                PrivacyTimeRange::AllTime,
                0,
            )
            .unwrap(),
            2
        );
        let connection = fixture.connection();
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM meta", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            1
        );
    }

    #[test]
    fn chromium_address_bar_shortcuts_honor_the_selected_time_range() {
        let fixture = DatabaseFixture::new("address-bar-shortcuts");
        let now_ms = 1_700_000_000_000_u64;
        let cutoff = cutoff_for(
            PrivacyTimeRange::LastHour,
            PlatformPrivacyBrowserKind::Chromium,
            now_ms,
        )
        .unwrap();
        let connection = fixture.connection();
        connection
            .execute_batch(&format!(
                "CREATE TABLE omni_box_shortcuts(
                   id TEXT PRIMARY KEY,
                   text TEXT,
                   last_access_time INTEGER
                 );
                 INSERT INTO omni_box_shortcuts VALUES
                   ('older', 'private', {}),
                   ('newer', 'private', {});",
                cutoff - 1,
                cutoff + 1
            ))
            .unwrap();
        drop(connection);

        assert_eq!(
            clear(
                &fixture.path,
                PlatformPrivacyBrowserKind::Chromium,
                PrivacyDataKind::AddressBarShortcuts,
                PrivacyTimeRange::LastHour,
                now_ms,
            )
            .unwrap(),
            1
        );
        let connection = fixture.connection();
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM omni_box_shortcuts", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            1
        );
    }

    #[test]
    fn chromium_search_history_cleanup_preserves_navigation_history() {
        let fixture = DatabaseFixture::new("search-history");
        let now_ms = 1_700_000_000_000_u64;
        let cutoff = cutoff_for(
            PrivacyTimeRange::LastHour,
            PlatformPrivacyBrowserKind::Chromium,
            now_ms,
        )
        .unwrap();
        let connection = fixture.connection();
        connection
            .execute_batch(&format!(
                "CREATE TABLE urls(id INTEGER PRIMARY KEY, last_visit_time INTEGER);
                 CREATE TABLE visits(id INTEGER PRIMARY KEY, url INTEGER, visit_time INTEGER);
                 CREATE TABLE keyword_search_terms(url_id INTEGER, term TEXT);
                 INSERT INTO urls VALUES (1, {}), (2, {});
                 INSERT INTO visits VALUES (1, 1, {}), (2, 2, {});
                 INSERT INTO keyword_search_terms VALUES (1, 'older'), (2, 'newer');",
                cutoff - 1,
                cutoff + 1,
                cutoff - 1,
                cutoff + 1
            ))
            .unwrap();
        drop(connection);

        assert_eq!(
            clear(
                &fixture.path,
                PlatformPrivacyBrowserKind::Chromium,
                PrivacyDataKind::SearchHistory,
                PrivacyTimeRange::LastHour,
                now_ms,
            )
            .unwrap(),
            1
        );
        let connection = fixture.connection();
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM keyword_search_terms", [], |row| row
                    .get::<_, i64>(
                    0
                ))
                .unwrap(),
            1
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM visits", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            2
        );
    }

    #[test]
    fn chromium_website_icon_cleanup_preserves_database_metadata() {
        let fixture = DatabaseFixture::new("website-icons");
        let connection = fixture.connection();
        connection
            .execute_batch(
                "CREATE TABLE favicons(id INTEGER PRIMARY KEY, url TEXT);
                 CREATE TABLE favicon_bitmaps(id INTEGER PRIMARY KEY, icon_id INTEGER, image_data BLOB);
                 CREATE TABLE icon_mapping(id INTEGER PRIMARY KEY, page_url TEXT, icon_id INTEGER);
                 CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT);
                 INSERT INTO favicons VALUES (1, 'https://icon.invalid/favicon.ico');
                 INSERT INTO favicon_bitmaps VALUES (1, 1, X'00');
                 INSERT INTO icon_mapping VALUES (1, 'https://icon.invalid', 1);
                 INSERT INTO meta VALUES ('version', 'preserve');",
            )
            .unwrap();
        drop(connection);

        assert_eq!(
            clear(
                &fixture.path,
                PlatformPrivacyBrowserKind::Chromium,
                PrivacyDataKind::WebsiteIcons,
                PrivacyTimeRange::AllTime,
                0,
            )
            .unwrap(),
            1
        );
        let connection = fixture.connection();
        for table in ["icon_mapping", "favicon_bitmaps", "favicons"] {
            assert_eq!(
                connection
                    .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row
                        .get::<_, i64>(0))
                    .unwrap(),
                0
            );
        }
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM meta", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            1
        );
    }

    #[test]
    fn unknown_schema_fails_closed() {
        let fixture = DatabaseFixture::new("unknown-schema");
        fixture
            .connection()
            .execute("CREATE TABLE unexpected(value TEXT)", [])
            .unwrap();
        assert!(count(
            &fixture.path,
            PlatformPrivacyBrowserKind::Chromium,
            PrivacyDataKind::BrowsingHistory,
            PrivacyTimeRange::AllTime,
            0
        )
        .is_err());
    }

    #[test]
    fn history_details_group_matching_urls_and_page_without_exposing_database_paths() {
        let fixture = DatabaseFixture::new("history-details");
        fixture
            .connection()
            .execute_batch(
                "CREATE TABLE urls(id INTEGER PRIMARY KEY, url TEXT);
                 CREATE TABLE visits(id INTEGER PRIMARY KEY, url INTEGER, visit_time INTEGER);
                 INSERT INTO urls VALUES
                   (1, 'https://example.invalid/private?q=one'),
                   (2, 'https://second.invalid/path');
                 INSERT INTO visits VALUES (1, 1, 10), (2, 1, 20), (3, 2, 30);",
            )
            .unwrap();

        let first = details(
            &fixture.path,
            PlatformPrivacyBrowserKind::Chromium,
            PrivacyDataKind::BrowsingHistory,
            PrivacyTimeRange::AllTime,
            0,
            0,
            1,
        )
        .unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].label, "https://second.invalid/path");
        assert_eq!(first[0].item_count, 1);

        let second = details(
            &fixture.path,
            PlatformPrivacyBrowserKind::Chromium,
            PrivacyDataKind::BrowsingHistory,
            PrivacyTimeRange::AllTime,
            0,
            1,
            1,
        )
        .unwrap();
        assert_eq!(second[0].label, "https://example.invalid/private?q=one");
        assert_eq!(second[0].item_count, 2);
    }

    #[test]
    fn credential_details_expose_only_origins_and_counts() {
        let fixture = DatabaseFixture::new("credential-details");
        fixture
            .connection()
            .execute_batch(
                "CREATE TABLE logins(origin_url TEXT, username_value TEXT, password_value BLOB);
                 INSERT INTO logins VALUES
                   ('https://example.invalid', 'private-user', X'736563726574'),
                   ('https://example.invalid', 'second-user', X'6D6F7265');",
            )
            .unwrap();

        let entries = details(
            &fixture.path,
            PlatformPrivacyBrowserKind::Chromium,
            PrivacyDataKind::SavedPasswords,
            PrivacyTimeRange::AllTime,
            0,
            0,
            20,
        )
        .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].label, "https://example.invalid");
        assert_eq!(entries[0].item_count, 2);
        let serialized = serde_json::to_string(&entries).unwrap();
        assert!(!serialized.contains("private-user"));
        assert!(!serialized.contains("secret"));
    }

    #[cfg(unix)]
    #[test]
    fn symbolic_link_database_is_rejected_before_sqlite_opens_it() {
        use std::os::unix::fs::symlink;

        let fixture = DatabaseFixture::new("database-link");
        fixture
            .connection()
            .execute_batch(
                "CREATE TABLE urls(id INTEGER PRIMARY KEY, url TEXT);
                 CREATE TABLE visits(id INTEGER PRIMARY KEY, url INTEGER, visit_time INTEGER);",
            )
            .unwrap();
        let link = fixture.directory.join("linked.sqlite");
        symlink(&fixture.path, &link).unwrap();

        let error = count(
            &link,
            PlatformPrivacyBrowserKind::Chromium,
            PrivacyDataKind::BrowsingHistory,
            PrivacyTimeRange::AllTime,
            0,
        )
        .unwrap_err();
        assert!(!error.diagnostic().contains(link.to_string_lossy().as_ref()));
    }
}

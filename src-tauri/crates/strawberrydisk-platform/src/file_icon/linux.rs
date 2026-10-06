//! Desktop metadata stays off the sampling path; only GTK theme lookup uses its main thread.
use super::{
    linux_desktop_identity::{self, DesktopIcon},
    IconQuery,
};
use gio::prelude::*;
use gtk::prelude::IconThemeExt;
use std::{
    cell::Cell,
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};

const INDEX_TTL: Duration = Duration::from_secs(60);
const MAX_DESKTOP_ENTRIES: usize = 4096;
const MAX_RESOLVED_PATHS: usize = 2048;
const MAX_SOURCE_BYTES: u64 = 8 * 1024 * 1024;
const ICON_SIZE: i32 = 64;
static THEME_REVISION: AtomicU64 = AtomicU64::new(0);
static INDEX: Mutex<Option<DesktopIndex>> = Mutex::new(None);
static PROCESS_ICON: OnceLock<(PathBuf, &'static [u8])> = OnceLock::new();
thread_local! {
    static THEME_OBSERVED: Cell<bool> = const { Cell::new(false) };
}

#[derive(Clone)]
struct ResolvedIcon {
    desktop_file: PathBuf,
    image: PathBuf,
}

struct DesktopIndex {
    refreshed: Instant,
    theme_revision: u64,
    by_executable: HashMap<PathBuf, Option<DesktopIcon>>,
    resolved: HashMap<PathBuf, Option<ResolvedIcon>>,
    theme_retry_after: Option<Instant>,
}

pub(super) fn register_process_icon(path: PathBuf, png: &'static [u8]) {
    if super::cache::valid_png(png) {
        let _ = PROCESS_ICON.set((fs::canonicalize(&path).unwrap_or(path), png));
    }
}

fn process_icon(path: &Path) -> Option<&'static [u8]> {
    let (own_path, png) = PROCESS_ICON.get()?;
    (path == own_path).then_some(*png)
}

pub(super) fn provider_variant(query: &IconQuery) -> Vec<u8> {
    let IconQuery::Path { path, .. } = query else {
        return Vec::new();
    };
    if let Some(png) = process_icon(path) {
        return blake3::hash(png).as_bytes().to_vec();
    }
    let Some(icon) = resolve(path) else {
        return Vec::new();
    };
    let mut stamp = blake3::Hasher::new();
    stamp.update(b"linux-desktop-icon-v1-64px");
    for path in [&icon.desktop_file, &icon.image] {
        stamp.update(path.as_os_str().as_encoded_bytes());
        if let Ok(metadata) = fs::metadata(path) {
            stamp.update(&metadata.len().to_le_bytes());
            if let Ok(modified) = metadata.modified().and_then(|time| {
                time.duration_since(std::time::UNIX_EPOCH)
                    .map_err(std::io::Error::other)
            }) {
                stamp.update(&modified.as_nanos().to_le_bytes());
            }
        }
    }
    stamp.finalize().as_bytes().to_vec()
}

pub(super) fn load_png(query: &IconQuery) -> Option<Vec<u8>> {
    let IconQuery::Path { path, .. } = query else {
        return None;
    };
    if let Some(png) = process_icon(path) {
        return Some(png.to_vec());
    }
    let icon = resolve(path)?;
    let metadata = fs::metadata(&icon.image).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_SOURCE_BYTES {
        return None;
    }
    match gdk_pixbuf::Pixbuf::from_file_at_scale(&icon.image, ICON_SIZE, ICON_SIZE, true)
        .and_then(|image| image.save_to_bufferv("png", &[]))
    {
        Ok(png) => Some(png),
        Err(error) => {
            log::debug!(
                "linux_application_icon_decode_failed path={} error={}",
                crate::diagnostics::text(&icon.image.to_string_lossy()),
                crate::diagnostics::text(&error.to_string())
            );
            None
        }
    }
}

fn resolve(path: &Path) -> Option<ResolvedIcon> {
    if !gtk::is_initialized() {
        return None;
    }
    let mut state = INDEX.lock().ok()?;
    if state
        .as_ref()
        .is_none_or(|index| index.refreshed.elapsed() >= INDEX_TTL)
    {
        *state = Some(DesktopIndex::build());
    }
    let index = state.as_mut()?;
    let revision = THEME_REVISION.load(Ordering::Relaxed);
    if index.theme_revision != revision {
        index.resolved.clear();
        index.theme_revision = revision;
    }
    if index
        .theme_retry_after
        .is_some_and(|deadline| Instant::now() < deadline)
    {
        return None;
    }
    if let Some(icon) = index.resolved.get(path) {
        return icon.clone();
    }
    let canonical = fs::canonicalize(path).ok()?;
    let icon = index.by_executable.get(&canonical).and_then(Option::as_ref);
    let resolved = if let Some(icon) = icon {
        match theme_filename(&icon.icon) {
            Ok(image) => image.map(|image| ResolvedIcon {
                desktop_file: icon.desktop_file.clone(),
                image,
            }),
            Err(()) => {
                // One unavailable GUI loop must not cause a timeout for every row in the batch.
                // Do not cache this transient failure as a missing application icon.
                index.theme_retry_after = Some(Instant::now() + Duration::from_secs(1));
                return None;
            }
        }
    } else {
        None
    };
    if index.resolved.len() >= MAX_RESOLVED_PATHS {
        index.resolved.clear();
    }
    index.resolved.insert(path.into(), resolved.clone());
    resolved
}

impl DesktopIndex {
    fn build() -> Self {
        let started = Instant::now();
        let mut by_executable = HashMap::new();
        let search_paths = std::env::var_os("PATH")
            .map(|value| {
                std::env::split_paths(&value)
                    .filter(|path| path.is_absolute())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut entries = gio::AppInfo::all()
            .into_iter()
            .filter_map(|app| app.downcast::<gio::DesktopAppInfo>().ok())
            .take(MAX_DESKTOP_ENTRIES)
            .collect::<Vec<_>>();
        entries.sort_by_key(gio::DesktopAppInfo::filename);
        for app in entries {
            let Some(desktop_file) = app.filename() else {
                continue;
            };
            let Some(icon) = app.icon().and_then(|icon| IconExt::to_string(&icon)) else {
                continue;
            };
            let Some(command) = app.commandline() else {
                continue;
            };
            let Ok(arguments) = glib::shell_parse_argv(command.as_os_str()) else {
                continue;
            };
            let arguments = arguments
                .iter()
                .map(|argument| argument.to_string_lossy().into_owned())
                .collect::<Vec<_>>();
            let Some(command) = linux_desktop_identity::command_executable(&arguments) else {
                continue;
            };
            let program = Path::new(command.program);
            let resolved = if program.is_absolute() {
                fs::canonicalize(program).ok()
            } else if program.components().count() == 1 {
                search_paths
                    .iter()
                    .find_map(|root| fs::canonicalize(root.join(program)).ok())
            } else {
                None
            };
            // A harmless-looking launcher name can be a symlink to a shared runtime.
            // Validate its canonical identity before assigning application artwork.
            if let Some(executable) = resolved.filter(|path| command.identifies(path)) {
                linux_desktop_identity::insert(
                    &mut by_executable,
                    executable,
                    DesktopIcon {
                        icon: icon.to_string(),
                        desktop_file,
                    },
                );
            }
        }
        log::info!(
            "linux_application_icon_index_ready executables={} elapsed_ms={}",
            by_executable.len(),
            started.elapsed().as_millis()
        );
        Self {
            refreshed: Instant::now(),
            theme_revision: THEME_REVISION.load(Ordering::Relaxed),
            by_executable,
            resolved: HashMap::new(),
            theme_retry_after: None,
        }
    }
}

fn theme_filename(icon: &str) -> Result<Option<PathBuf>, ()> {
    if gtk::is_initialized_main_thread() {
        return Ok(theme_filename_on_main_thread(icon));
    }
    let (sender, receiver) = mpsc::sync_channel(1);
    let icon = icon.to_owned();
    // invoke() may execute on the calling worker when the context is unowned.
    // An idle source always dispatches GTK access through the GUI main loop.
    glib::idle_add_once(move || {
        let _ = sender.send(theme_filename_on_main_thread(&icon));
    });
    match receiver.recv_timeout(Duration::from_millis(500)) {
        Ok(path) => Ok(path),
        Err(error) => {
            log::debug!("linux_application_icon_theme_lookup_failed stage=main_thread outcome=fallback error={error}");
            Err(())
        }
    }
}

fn theme_filename_on_main_thread(icon: &str) -> Option<PathBuf> {
    if !gtk::is_initialized_main_thread() {
        return None;
    }
    let theme = gtk::IconTheme::default()?;
    THEME_OBSERVED.with(|observed| {
        if !observed.replace(true) {
            theme.connect_changed(|_| {
                THEME_REVISION.fetch_add(1, Ordering::Relaxed);
            });
        }
    });
    let icon = gio::Icon::for_string(icon).ok()?;
    theme
        .lookup_by_gicon(&icon, ICON_SIZE, gtk::IconLookupFlags::FORCE_SIZE)?
        .filename()
}

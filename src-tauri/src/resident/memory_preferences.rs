//! Persist release preferences independently from monitor appearance and sampling.
use super::diagnostics::Failure;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};
use strawberrydisk_core::system_resources::release_policy::ReleasePreferences;
use tauri::{Emitter, Manager};
use tauri_plugin_store::StoreExt;

const FILE: &str = "memory-release.json";
pub const EVENT: &str = "memory-release-preferences";
pub struct MemoryReleaseState {
    pub preferences: Mutex<ReleasePreferences>,
    pub actions: AtomicU64,
    writable: bool,
    pub window_creation: Mutex<()>,
}

pub fn install(app: &tauri::AppHandle) {
    let loaded = (|| {
        let store = app
            .store_builder(FILE)
            .disable_auto_save()
            .build()
            .map_err(|e| Failure::record("memory_preferences_load", &e))?;
        match store.get("preferences") {
            Some(value) => decode(value),
            None => Ok(ReleasePreferences::default()),
        }
    })();
    let writable = loaded.is_ok();
    if let Ok(preferences) = &loaded {
        log::info!("memory_release_preferences_loaded platform={} revision={} automatic={} interval_minutes={} threshold_percent={} skip_foreground={} exclusions={}", std::env::consts::OS, preferences.revision, preferences.automatic, preferences.interval_minutes, preferences.threshold_percent, preferences.skip_foreground, preferences.exclusions.len());
    }
    let state = Arc::new(MemoryReleaseState {
        preferences: Mutex::new(loaded.unwrap_or_default()),
        actions: AtomicU64::new(0),
        writable,
        window_creation: Mutex::new(()),
    });
    app.manage(state.clone());
    super::memory_automatic::start(app.clone(), state);
}

fn decode(value: serde_json::Value) -> Result<ReleasePreferences, Failure> {
    let preferences: ReleasePreferences = serde_json::from_value(value)
        .map_err(|e| Failure::record("memory_preferences_decode", &e))?;
    preferences.validate().map_err(Failure::state)?;
    Ok(preferences)
}

pub fn get(app: &tauri::AppHandle) -> Result<ReleasePreferences, Failure> {
    let state = app.state::<Arc<MemoryReleaseState>>();
    // Do not silently discard exclusions when their persisted policy cannot be read.
    if !state.writable {
        return Err(Failure::state("memory_preferences_unreadable"));
    }
    let preferences = state
        .preferences
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    Ok(preferences)
}

pub fn save(
    app: &tauri::AppHandle,
    mut preferences: ReleasePreferences,
) -> Result<ReleasePreferences, Failure> {
    preferences.validate().map_err(Failure::state)?;
    let state = app.state::<Arc<MemoryReleaseState>>();
    if !state.writable {
        return Err(Failure::state("memory_preferences_unreadable"));
    }
    let mut current = state.preferences.lock().unwrap_or_else(|e| e.into_inner());
    if current.revision != preferences.revision {
        return Err(Failure::state("memory_preferences_conflict"));
    }
    preferences.revision = current
        .revision
        .checked_add(1)
        .ok_or_else(|| Failure::state("memory_preferences_revision"))?;
    let store = app
        .store_builder(FILE)
        .disable_auto_save()
        .build()
        .map_err(|e| Failure::record("memory_preferences_open", &e))?;
    let previous = store.get("preferences");
    store.set(
        "preferences",
        serde_json::to_value(&preferences)
            .map_err(|e| Failure::record("memory_preferences_encode", &e))?,
    );
    if let Err(error) = store.save() {
        if let Some(value) = previous {
            store.set("preferences", value);
        } else {
            store.delete("preferences");
        }
        return Err(Failure::record("memory_preferences_save", &error));
    }
    for item in &preferences.exclusions {
        if !current.exclusions.contains(item) {
            log::info!(
                "memory_release_exclusion_added name={} path={}",
                strawberrydisk_platform::diagnostics::text(&item.name),
                strawberrydisk_platform::diagnostics::text(&item.path)
            );
        }
    }
    for item in &current.exclusions {
        if !preferences.exclusions.contains(item) {
            log::info!(
                "memory_release_exclusion_removed name={} path={}",
                strawberrydisk_platform::diagnostics::text(&item.name),
                strawberrydisk_platform::diagnostics::text(&item.path)
            );
        }
    }
    *current = preferences.clone();
    drop(current);
    log::info!("memory_release_preferences_saved revision={} automatic={} interval_minutes={} threshold_percent={} skip_foreground={} exclusions={}", preferences.revision, preferences.automatic, preferences.interval_minutes, preferences.threshold_percent, preferences.skip_foreground, preferences.exclusions.len());
    if let Err(e) = app.emit(EVENT, &preferences) {
        Failure::record("memory_preferences_notify", &e);
    }
    Ok(preferences)
}

pub fn record_action(app: &tauri::AppHandle) {
    app.state::<Arc<MemoryReleaseState>>()
        .actions
        .fetch_add(1, Ordering::Relaxed);
}

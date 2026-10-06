//! Publish one immutable snapshot to native surfaces and visible WebViews.
use super::runtime::{ResidentReading, ResidentState, READING_EVENT};
use std::sync::{
    atomic::Ordering,
    mpsc::{self, SyncSender},
    Arc, Mutex,
};
use tauri::{Emitter, Manager};

pub struct Presentation {
    wake: SyncSender<()>,
}

impl Presentation {
    pub fn start(app: tauri::AppHandle, state: Arc<ResidentState>) -> Self {
        let (wake, events) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let mut stalled = false;
            let mut panel_seeded = false;
            while events.recv().is_ok() {
                let started = std::time::Instant::now();
                // Preference commits and publications share a gate. Sampling never waits
                // for this gate, and only the newest reading survives a blocked shell.
                let _update = state
                    .preference_update
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                let preferences = state
                    .preferences
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .clone();
                publish_snapshot(
                    &state.reading,
                    &state.published_reading,
                    |reading| super::tray_display::refresh(&app, &preferences, reading),
                    |reading| {
                        let has_cpu = reading.resources.cpu_processes.value.is_some();
                        if !has_cpu {
                            panel_seeded = false;
                        }
                        // Seed a prewarmed hidden panel once so its first visible frame
                        // can already contain rows. Hidden panels receive no periodic events.
                        let seed =
                            !panel_seeded && has_cpu && state.panel_ready.load(Ordering::Relaxed);
                        if (state.panel_open.load(Ordering::Relaxed) || seed)
                            && app
                                .emit_to(super::PANEL_LABEL, READING_EVENT, reading)
                                .is_ok()
                        {
                            panel_seeded = has_cpu;
                        }
                        if app
                            .get_webview_window(crate::MAIN_WINDOW_LABEL)
                            .is_some_and(|window| window.is_visible().unwrap_or(false))
                        {
                            // The main window only consumes overview metrics. Avoid serializing
                            // full process identities into a second WebView on every tick.
                            let mut overview = reading.clone();
                            overview.resources.gpu_details = Default::default();
                            overview.resources.gpu_detail_history.clear();
                            overview.resources.cpu_processes = Default::default();
                            overview.resources.memory_processes = Default::default();
                            let _ = app.emit_to(crate::MAIN_WINDOW_LABEL, READING_EVENT, &overview);
                        }
                    },
                );
                let elapsed_ms = started.elapsed().as_millis();
                let slow = elapsed_ms > 1000;
                if slow && !stalled {
                    log::warn!("resident_presentation_delayed elapsed_ms={elapsed_ms}");
                } else if stalled && !slow {
                    log::info!("resident_presentation_recovered");
                }
                stalled = slow;
            }
        });
        Self { wake }
    }

    pub fn publish(&self) {
        // A full slot already guarantees another publication of the latest reading.
        let _ = self.wake.try_send(());
    }
}

fn publish_snapshot(
    latest: &Mutex<ResidentReading>,
    published: &Mutex<ResidentReading>,
    display: impl FnOnce(&ResidentReading),
    notify: impl FnOnce(&ResidentReading),
) {
    let reading = latest
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone();
    display(&reading);
    *published.lock().unwrap_or_else(|error| error.into_inner()) = reading.clone();
    notify(&reading);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading(revision: u64) -> ResidentReading {
        ResidentReading {
            revision,
            resources: Default::default(),
        }
    }

    #[test]
    fn native_event_and_reopen_cache_share_a_snapshot_while_sampling_advances() {
        let latest = Mutex::new(reading(1));
        let published = Mutex::new(reading(0));
        publish_snapshot(
            &latest,
            &published,
            |native| {
                assert_eq!(native.revision, 1);
                // A slow native call must neither block sampling nor reread its newer value.
                latest.lock().unwrap().revision = 2;
                assert_eq!(published.lock().unwrap().revision, 0);
            },
            |event| {
                assert_eq!(event.revision, 1);
                assert_eq!(published.lock().unwrap().revision, event.revision);
            },
        );
        publish_snapshot(
            &latest,
            &published,
            |native| assert_eq!(native.revision, 2),
            |event| assert_eq!(event.revision, 2),
        );
    }

    #[test]
    fn stalled_presentation_coalesces_requests_and_keeps_the_next_publication() {
        let (wake, events) = mpsc::sync_channel(1);
        let presentation = Presentation { wake };
        for _ in 0..10_000 {
            presentation.publish();
        }
        events.try_recv().unwrap();
        assert!(events.try_recv().is_err());
        presentation.publish();
        events.try_recv().unwrap();
    }
}

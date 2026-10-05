//! Metric labels remain local to the resident display domain.
use mangodisk_core::system_resources::metrics::{MetricId, MetricStatus};

use crate::services::native_labels::NativeLabels;

pub struct Labels {
    pub locale: String,
    messages: NativeLabels,
}

impl Labels {
    pub fn load(app: &tauri::AppHandle) -> Self {
        Self::new(NativeLabels::load(app))
    }

    #[cfg(any(target_os = "windows", test))]
    pub fn for_locale(locale: &str) -> Self {
        Self::new(NativeLabels::for_locale(locale))
    }

    fn new(messages: NativeLabels) -> Self {
        Self {
            locale: messages.locale.clone(),
            messages,
        }
    }

    pub fn text(&self, key: &str) -> &str {
        self.messages.text(&format!("/systemStatus/{key}"))
    }
    pub fn metric(&self, metric: MetricId) -> &str {
        self.text(match metric {
            MetricId::Cpu => "cpu",
            MetricId::Gpu => "gpu",
            MetricId::Memory => "memory",
            MetricId::Network => "network",
            MetricId::Disk => "disk",
        })
    }
    pub fn status(&self, status: MetricStatus) -> &str {
        self.text(match status {
            MetricStatus::Loading => "loading",
            MetricStatus::Ready => "ready",
            MetricStatus::Stale => "stale",
            MetricStatus::Disconnected => "disconnected",
            MetricStatus::Unsupported => "unsupported",
            MetricStatus::Failed => "failed",
        })
    }
}

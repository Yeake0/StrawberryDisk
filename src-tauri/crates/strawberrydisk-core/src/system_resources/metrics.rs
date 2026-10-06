//! Versioned per-metric protocol. A missing value never means a valid zero.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MetricId {
    Cpu,
    Gpu,
    Memory,
    Network,
    Disk,
}

impl MetricId {
    pub const ALL: [Self; 5] = [
        Self::Cpu,
        Self::Gpu,
        Self::Memory,
        Self::Network,
        Self::Disk,
    ];

    pub const fn interval_ms(self) -> u64 {
        match self {
            Self::Cpu | Self::Gpu => 2_000,
            Self::Memory => 3_000,
            Self::Network => 1_000,
            Self::Disk => 30_000,
        }
    }

    pub const fn freshness_ms(self) -> u64 {
        match self {
            Self::Cpu | Self::Gpu | Self::Network => 5_000,
            Self::Memory => 10_000,
            Self::Disk => 90_000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MetricStatus {
    Loading,
    Ready,
    Stale,
    Disconnected,
    Unsupported,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricReading<T> {
    pub status: MetricStatus,
    pub sampled_at_ms: Option<u64>,
    pub value: Option<T>,
}

impl<T> Default for MetricReading<T> {
    fn default() -> Self {
        Self {
            status: MetricStatus::Loading,
            sampled_at_ms: None,
            value: None,
        }
    }
}

impl<T> MetricReading<T> {
    pub fn ready(value: T, sampled_at_ms: u64) -> Self {
        Self {
            status: MetricStatus::Ready,
            sampled_at_ms: Some(sampled_at_ms),
            value: Some(value),
        }
    }

    pub fn expire(&mut self, now_ms: u64, lifetime_ms: u64) {
        if self.status == MetricStatus::Ready
            && self.sampled_at_ms.is_some_and(|sampled| {
                now_ms < sampled || now_ms.saturating_sub(sampled) > lifetime_ms
            })
        {
            self.status = MetricStatus::Stale;
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CpuUsage {
    pub used_percent: f64,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendPoint {
    pub sampled_at_ms: u64,
    pub primary: f64,
    pub secondary: Option<f64>,
}

// The UI plays a buffered minute and needs its offscreen endpoints again after
// reopening. Keep a small margin in Core as well as in the mounted chart.
const HISTORY_RETENTION_MS: u64 = 80_000;
const HISTORY_POINT_LIMIT: usize = 96;

/// Both time and count bounds apply, including duplicate or backwards wall times.
#[derive(Debug, Default)]
pub struct Trend {
    points: VecDeque<TrendPoint>,
}

impl Trend {
    pub fn push(&mut self, point: TrendPoint) {
        if self
            .points
            .back()
            .is_some_and(|last| last.sampled_at_ms >= point.sampled_at_ms)
        {
            self.points.clear();
        }
        self.points.push_back(point);
        while self.points.len() > HISTORY_POINT_LIMIT
            || self.points.front().is_some_and(|first| {
                point.sampled_at_ms.saturating_sub(first.sampled_at_ms) > HISTORY_RETENTION_MS
            })
        {
            self.points.pop_front();
        }
    }

    pub fn snapshot(&self, now_ms: u64) -> Vec<TrendPoint> {
        self.points
            .iter()
            .filter(|point| now_ms.saturating_sub(point.sampled_at_ms) <= HISTORY_RETENTION_MS)
            .copied()
            .collect()
    }

    pub fn clear(&mut self) {
        self.points.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reopened_charts_receive_the_buffered_minute_and_offscreen_endpoints() {
        let mut trend = Trend::default();
        for sampled_at_ms in (0..=120_000).step_by(1000) {
            trend.push(TrendPoint {
                sampled_at_ms,
                primary: 10.0,
                secondary: None,
            });
        }
        let history = trend.snapshot(120_500);
        assert!(history[0].sampled_at_ms < 120_500 - 60_000 - 3_250 - 6_000);
        assert!(history.len() <= HISTORY_POINT_LIMIT);
        assert!(trend.snapshot(201_000).is_empty());
    }

    #[test]
    fn a_backwards_wall_clock_never_keeps_a_future_sample_ready() {
        let mut reading = MetricReading::ready(1, 1000);
        reading.expire(999, 5000);
        assert_eq!(reading.status, MetricStatus::Stale);
    }

    #[test]
    fn missing_sample_is_loading_and_expiry_preserves_diagnostic_value() {
        let missing = MetricReading::<CpuUsage>::default();
        assert_eq!(missing.status, MetricStatus::Loading);
        assert!(missing.value.is_none());
        let mut value = MetricReading::ready(CpuUsage { used_percent: 12.0 }, 1_000);
        value.expire(6_000, MetricId::Cpu.freshness_ms());
        assert_eq!(value.status, MetricStatus::Ready);
        value.expire(6_001, MetricId::Cpu.freshness_ms());
        assert_eq!(value.status, MetricStatus::Stale);
        assert_eq!(value.value.unwrap().used_percent, 12.0);
    }

    #[test]
    fn trends_are_bounded_and_never_fabricate_old_history() {
        let mut trend = Trend::default();
        for sampled_at_ms in 0..500 {
            trend.push(TrendPoint {
                sampled_at_ms,
                primary: 1.0,
                secondary: None,
            });
        }
        assert_eq!(trend.snapshot(500).len(), HISTORY_POINT_LIMIT);
        assert!(trend.snapshot(81_000).is_empty());
        trend.push(TrendPoint {
            sampled_at_ms: 1,
            primary: 2.0,
            secondary: None,
        });
        assert_eq!(trend.snapshot(1).len(), 1);
    }
}

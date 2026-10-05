import type { TrendPoint } from '@/lib/models/system-resources';

/** Summarize actual observations in the last minute; gaps never count as idle. */
export function summarizeGpuHistory(history: TrendPoint[], observedAtMs: number) {
  const values = history
    .filter(point => point.sampledAtMs >= observedAtMs - 60_000 && point.sampledAtMs <= observedAtMs)
    .map(point => point.primary)
    .filter(value => Number.isFinite(value) && value >= 0 && value <= 100);
  return values.length
    ? { average: values.reduce((total, value) => total + value, 0) / values.length, peak: Math.max(...values) }
    : null;
}

import type { GpuActivity, GpuActivityKind } from '@/lib/models/gpu-details';

const ENGINE_ORDER: Record<GpuActivityKind, number> = {
  graphics: 0,
  copy: 1,
  videoDecode: 2,
  videoEncode: 3,
  videoProcessing: 4,
  other: 5,
  renderer: 6,
  tiler: 7,
};
// Native engine names are identifiers, so ordering must not change with the UI locale.
const engineNames = new Intl.Collator('en', { numeric: true, sensitivity: 'base' });
function compareNames(a: GpuActivity, b: GpuActivity): number {
  return (
    engineNames.compare(a.name || a.id, b.name || b.id) ||
    engineNames.compare(a.id, b.id) ||
    (a.id < b.id ? -1 : a.id > b.id ? 1 : 0)
  );
}

export function customGpuActivities(activities: readonly GpuActivity[]): GpuActivity[] {
  return activities
    .filter(activity => !activity.includedInSummary && activity.kind !== 'renderer' && activity.kind !== 'tiler')
    .sort(compareNames);
}

/** Group standard activity by engine kind, retaining the busiest engine rather than summing parallel work. */
export function summarizeGpuActivities(activities: readonly GpuActivity[]): GpuActivity[] {
  const grouped = new Map<string, GpuActivity>();
  for (const activity of activities) {
    if (!activity.includedInSummary && activity.kind !== 'renderer' && activity.kind !== 'tiler') continue;
    const key = activity.kind === 'other' ? activity.id : activity.kind;
    const previous = grouped.get(key);
    if (!previous || activity.usedPercent > previous.usedPercent) grouped.set(key, { ...activity, id: key });
  }
  return [...grouped.values()].sort((a, b) => ENGINE_ORDER[a.kind] - ENGINE_ORDER[b.kind] || compareNames(a, b));
}

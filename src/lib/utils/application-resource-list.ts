import type {
  ResourceApplication,
  ResourceListRow,
  ResourceSort,
  ResourceSortPreferences,
} from '@/lib/models/application-resource-list';

export function defaultResourceSort(): ResourceSortPreferences {
  return {
    schemaVersion: 1,
    cpu: { column: 'usage', direction: 'descending' },
    memory: { column: 'usage', direction: 'descending' },
  };
}
export function parseResourceSort(value: unknown): ResourceSortPreferences {
  const defaults = defaultResourceSort();
  if (!value || typeof value !== 'object' || !('schemaVersion' in value) || value.schemaVersion !== 1) return defaults;
  const parse = (item: unknown, fallback: ResourceSort): ResourceSort => {
    if (!item || typeof item !== 'object' || !('column' in item) || !('direction' in item)) return fallback;
    if (
      (item.column !== 'name' && item.column !== 'usage') ||
      (item.direction !== 'ascending' && item.direction !== 'descending')
    )
      return fallback;
    return { column: item.column, direction: item.direction };
  };
  return {
    schemaVersion: 1,
    cpu: parse('cpu' in value ? value.cpu : null, defaults.cpu),
    memory: parse('memory' in value ? value.memory : null, defaults.memory),
  };
}
export function resourceValue(row: ResourceApplication): number {
  return 'usedPercent' in row ? row.usedPercent : (row.usedBytes ?? 0);
}
export function sortResourceRows(rows: ResourceListRow[], sort: ResourceSort, locale: string): ResourceListRow[] {
  const collator = new Intl.Collator(locale, { numeric: true, sensitivity: 'base' });
  const sign = sort.direction === 'ascending' ? 1 : -1;
  return [...rows].sort((a, b) => {
    const primary =
      sort.column === 'name'
        ? collator.compare(a.application.name, b.application.name)
        : Number(a.available) - Number(b.available) || resourceValue(a.application) - resourceValue(b.application);
    // Unreadable values remain last in both usage directions; zero is a valid reading.
    return (
      (sort.column === 'usage' ? Number(b.available) - Number(a.available) : 0) ||
      primary * sign ||
      collator.compare(a.application.name, b.application.name) ||
      (a.application.id < b.application.id ? -1 : a.application.id > b.application.id ? 1 : 0)
    );
  });
}
export function updateResourceRows(
  previous: ResourceListRow[],
  applications: ResourceApplication[],
  locked: boolean
): ResourceListRow[] {
  const current = new Map(applications.map(application => [application.id, application]));
  const fresh = (application: ResourceApplication): ResourceListRow => ({
    application,
    available: 'usedPercent' in application || application.usedBytes !== null,
    members: 'processes' in application ? [...(application.processes ?? [])] : [],
  });
  if (!locked) return applications.map(fresh);
  return previous.map(row => {
    const application = current.get(row.application.id);
    if (!application) return { ...row, available: false, members: row.members.map(p => ({ ...p, usedPercent: null })) };
    const members = new Map(
      ('processes' in application ? (application.processes ?? []) : []).map(p => [`${p.pid}:${p.startedAt}`, p])
    );
    return {
      application,
      available: 'usedPercent' in application || application.usedBytes !== null,
      // Preserve member geometry too. New members appear after unlocking.
      members: row.members.map(p => members.get(`${p.pid}:${p.startedAt}`) ?? { ...p, usedPercent: null }),
    };
  });
}

import { expect, it, vi } from 'vitest';

import { PreferenceStorageService } from './preference-storage-service';

const { loadMock, values } = vi.hoisted(() => {
  const values = new Map<string, unknown>();
  const store = {
    get: async (key: string) => values.get(key),
    set: async (key: string, value: unknown) => {
      values.set(key, value);
    },
    save: async () => undefined,
  };
  return { values, loadMock: vi.fn().mockRejectedValueOnce(new Error('store unavailable')).mockResolvedValue(store) };
});

vi.mock('@tauri-apps/plugin-store', () => ({ load: loadMock }));

it('retries after the settings store fails to open before consuming a coalesced update', async () => {
  const initial = { schemaVersion: 1 as const, viewMode: 'treemap' as const, treemapDepth: 1, sunburstDepth: 3 };
  const first = PreferenceStorageService.saveAnalysisViewPreferences(initial);
  const second = PreferenceStorageService.saveAnalysisViewPreferences({ ...initial, treemapDepth: 2 });
  await Promise.all([
    expect(first).rejects.toThrow('store unavailable'),
    expect(second).rejects.toThrow('store unavailable'),
  ]);
  const recovered = { ...initial, viewMode: 'sunburst' as const, sunburstDepth: 6 };
  await PreferenceStorageService.saveAnalysisViewPreferences(recovered);
  expect(loadMock).toHaveBeenCalledTimes(2);
  expect(values.get('analysisViewPreferences')).toEqual(recovered);
});

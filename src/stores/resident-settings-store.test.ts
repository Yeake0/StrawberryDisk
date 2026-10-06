import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createPinia, setActivePinia } from 'pinia';
import { useResidentSettingsStore } from './resident-settings-store';
import { ResidentService } from '@/lib/services/resident-service';
import { preferencesFixture } from '@/tests/fixtures/resident';
import type { ResidentPreferences } from '@/lib/models/resident';

vi.mock('@/lib/services/resident-service', () => ({
  ResidentService: { preferences: vi.fn(), savePreferences: vi.fn() },
}));
vi.mock('@/lib/services/logger-service', () => ({ LoggerService: { warn: vi.fn() } }));

describe('resident preference transactions', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
    vi.mocked(ResidentService.preferences).mockResolvedValue(preferencesFixture());
    vi.mocked(ResidentService.savePreferences).mockImplementation(async value => ({
      ...value,
      revision: value.revision + 1,
    }));
  });

  it('coalesces fifty edits behind one in-flight write and uses its committed revision', async () => {
    const store = useResidentSettingsStore();
    await store.load();
    let complete!: (value: ResidentPreferences) => void;
    vi.mocked(ResidentService.savePreferences).mockReturnValueOnce(
      new Promise(resolve => {
        complete = resolve;
      })
    );
    const first = store.change({ showIcon: false });
    for (let index = 0; index < 50; index++) await store.change({ networkInterface: `device-${index}` });
    expect(ResidentService.savePreferences).toHaveBeenCalledTimes(1);
    expect(store.draft?.networkInterface).toBe('device-49');
    expect(store.preferences?.networkInterface).toBeNull();
    complete({ ...preferencesFixture(), showIcon: false, revision: 1 });
    await first;
    expect(ResidentService.savePreferences).toHaveBeenCalledTimes(2);
    expect(vi.mocked(ResidentService.savePreferences).mock.calls[1]?.[0]).toMatchObject({
      revision: 1,
      networkInterface: 'device-49',
    });
    expect(store.preferences).toMatchObject({ revision: 2, networkInterface: 'device-49', showIcon: false });
    expect(store.saving).toBe(false);
  });

  it('rolls preview back to committed state after a rejected native transaction', async () => {
    const store = useResidentSettingsStore();
    await store.load();
    vi.mocked(ResidentService.savePreferences).mockRejectedValueOnce(new Error('disk full'));
    await store.change({ showIcon: false, diskVolume: 'uncommitted' });
    expect(store.error).toBe(true);
    expect(store.draft).toEqual(preferencesFixture());
    expect(store.preferences).toEqual(preferencesFixture());
    await store.change({ showIcon: false });
    expect(store.error).toBe(false);
    expect(store.preferences?.showIcon).toBe(false);
  });

  it('retains known preferences if reconciliation also fails', async () => {
    const store = useResidentSettingsStore();
    await store.load();
    vi.mocked(ResidentService.savePreferences).mockRejectedValueOnce(new Error('native failure'));
    vi.mocked(ResidentService.preferences).mockRejectedValueOnce(new Error('IPC failure'));
    await store.change({ enabled: false });
    expect(store.draft?.enabled).toBe(true);
    expect(store.error).toBe(true);
  });

  it.each(['resolve', 'reject'] as const)(
    'ignores a stale retry that completes with %s after a newer edit',
    async outcome => {
      const store = useResidentSettingsStore();
      await store.load();
      let resolve!: (value: ResidentPreferences) => void;
      let reject!: (error: Error) => void;
      vi.mocked(ResidentService.preferences).mockReturnValueOnce(
        new Promise((success, failure) => {
          resolve = success;
          reject = failure;
        })
      );
      const retry = store.load();
      await store.change({ showIcon: false });
      if (outcome === 'resolve') resolve(preferencesFixture());
      else reject(new Error('late read failure'));
      await retry;
      expect(store.preferences).toMatchObject({ revision: 1, showIcon: false });
      expect(store.draft).toEqual(store.preferences);
      expect(store.error).toBe(false);
      expect(store.loading).toBe(false);
    }
  );
});

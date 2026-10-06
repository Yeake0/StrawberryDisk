import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createPinia, setActivePinia } from 'pinia';
import { useMemoryReleaseStore } from './memory-release-store';
import { MemoryReleaseService } from '@/lib/services/memory-release-service';
import type { MemoryReleasePreferences } from '@/lib/models/memory-release';
vi.mock('@/lib/services/memory-release-service', () => ({
  MemoryReleaseService: { preferences: vi.fn(), save: vi.fn() },
}));
vi.mock('@/lib/services/logger-service', () => ({ LoggerService: { warn: vi.fn() } }));
const preferences = (): MemoryReleasePreferences => ({
  schemaVersion: 1,
  revision: 0,
  automatic: false,
  intervalMinutes: 30,
  thresholdPercent: 80,
  skipForeground: true,
  exclusions: [],
});
beforeEach(() => {
  setActivePinia(createPinia());
  vi.clearAllMocks();
});
describe('memory exclusion preferences', () => {
  it('keeps a newer window update when an older read finishes', () => {
    const store = useMemoryReleaseStore();
    store.accept({ ...preferences(), revision: 2 });
    store.accept(preferences());
    expect(store.preferences?.revision).toBe(2);
  });
  it('persists the program path and removes it with case-insensitive matching', async () => {
    const store = useMemoryReleaseStore();
    store.accept(preferences());
    vi.mocked(MemoryReleaseService.save).mockImplementation(async p => ({ ...p, revision: p.revision + 1 }));
    await store.toggle({ name: 'Code.exe', path: 'C:\\Apps\\Code.exe' });
    expect(store.excluded('c:\\apps\\code.exe')).toBe(true);
    await store.toggle({ name: 'Code.exe', path: 'c:\\apps\\code.exe' });
    expect(store.preferences?.exclusions).toEqual([]);
    expect(store.preferences?.revision).toBe(2);
  });
  it('does not pretend a failed write succeeded and reconciles concurrent edits', async () => {
    const store = useMemoryReleaseStore();
    store.accept(preferences());
    vi.mocked(MemoryReleaseService.save).mockRejectedValue(new Error('conflict'));
    vi.mocked(MemoryReleaseService.preferences).mockResolvedValue({ ...preferences(), revision: 3 });
    expect(await store.save({ ...preferences(), automatic: true })).toBe(false);
    expect(store.failed).toBe(true);
    expect(store.preferences?.automatic).toBe(false);
    expect(store.preferences?.revision).toBe(3);
  });
});

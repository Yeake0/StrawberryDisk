import { createPinia, setActivePinia } from 'pinia';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { LoggerService } from '@/lib/services/logger-service';
import { PreferenceStorageService } from '@/lib/services/preference-storage-service';

import { useStorageScanPreferencesStore } from './storage-scan-preferences-store';

describe('scan exclusion preferences store', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.restoreAllMocks();
  });

  it('loads scoped preferences without consulting legacy keys', async () => {
    vi.spyOn(PreferenceStorageService, 'loadScanExclusionPreferences').mockResolvedValue({
      schemaVersion: 3,
      names: [],
      folders: [{ path: '/fixture/cache', scopes: ['cleanup'] }],
    });
    const loadLegacy = vi.spyOn(PreferenceStorageService, 'loadLegacyStorageScanPreferences');

    const store = useStorageScanPreferencesStore();
    await store.initialize();

    expect(store.pathsForScope('cleanup')).toEqual(['/fixture/cache']);
    expect(store.pathsForScope('largeFiles')).toEqual([]);
    expect(loadLegacy).not.toHaveBeenCalled();
  });

  it('migrates shared v1 preferences to large and duplicate scans only', async () => {
    vi.spyOn(PreferenceStorageService, 'loadScanExclusionPreferences').mockResolvedValue(null);
    vi.spyOn(PreferenceStorageService, 'loadLegacyStorageScanPreferences').mockResolvedValue({
      schemaVersion: 1,
      excludedFolders: ['/fixture/cache'],
    });
    const migrate = vi.spyOn(PreferenceStorageService, 'migrateScanExclusionPreferences').mockResolvedValue();
    const info = vi.spyOn(LoggerService, 'info').mockImplementation(() => undefined);

    const store = useStorageScanPreferencesStore();
    await store.initialize();

    expect(store.pathsForScope('cleanup')).toEqual([]);
    expect(store.pathsForScope('largeFiles')).toEqual(['/fixture/cache']);
    expect(store.pathsForScope('duplicateFiles')).toEqual(['/fixture/cache']);
    expect(store.pathsForScope('analysis')).toEqual([]);
    expect(migrate).toHaveBeenCalledWith({
      schemaVersion: 3,
      names: [],
      folders: [{ path: '/fixture/cache', scopes: ['largeFiles', 'duplicateFiles'] }],
    });
    expect(info).toHaveBeenCalledWith('storage-scan', 'preferences_migrated', {
      source: 'storageScanPreferences',
      excludedFolderCount: 1,
      fromSchemaVersion: 1,
      toSchemaVersion: 3,
    });
  });

  it('keeps valid original large-file values active when migration persistence fails', async () => {
    vi.spyOn(PreferenceStorageService, 'loadScanExclusionPreferences').mockResolvedValue(null);
    vi.spyOn(PreferenceStorageService, 'loadLegacyStorageScanPreferences').mockResolvedValue(null);
    vi.spyOn(PreferenceStorageService, 'loadLegacyLargeFilePreferences').mockResolvedValue({
      schemaVersion: 1,
      excludedFolders: ['/fixture/cache'],
    });
    vi.spyOn(PreferenceStorageService, 'migrateScanExclusionPreferences').mockRejectedValue(new Error('disk full'));
    const warn = vi.spyOn(LoggerService, 'warn').mockImplementation(() => undefined);

    const store = useStorageScanPreferencesStore();
    await store.initialize();

    expect(store.pathsForScope('largeFiles')).toEqual(['/fixture/cache']);
    expect(store.pathsForScope('duplicateFiles')).toEqual([]);
    expect(store.pathsForScope('analysis')).toEqual([]);
    expect(warn).toHaveBeenCalledWith('storage-scan', 'preferences_migration_failed', {
      error: expect.any(Error),
    });
  });

  it('saves per-folder scopes and collapses overlapping paths per scan', async () => {
    vi.spyOn(PreferenceStorageService, 'loadScanExclusionPreferences').mockResolvedValue(null);
    vi.spyOn(PreferenceStorageService, 'loadLegacyStorageScanPreferences').mockResolvedValue(null);
    vi.spyOn(PreferenceStorageService, 'loadLegacyLargeFilePreferences').mockResolvedValue(null);
    const save = vi.spyOn(PreferenceStorageService, 'saveScanExclusionPreferences').mockResolvedValue();
    const store = useStorageScanPreferencesStore();

    await store.saveFolders([
      { path: '/fixture/cache/nested', scopes: ['cleanup'] },
      { path: '/fixture/cache', scopes: ['cleanup', 'largeFiles'] },
    ]);

    expect(store.pathsForScope('cleanup')).toEqual(['/fixture/cache']);
    expect(store.pathsForScope('largeFiles')).toEqual(['/fixture/cache']);
    expect(save).toHaveBeenCalledWith({
      schemaVersion: 3,
      names: [],
      folders: [
        { path: '/fixture/cache/nested', scopes: ['cleanup'] },
        { path: '/fixture/cache', scopes: ['cleanup', 'largeFiles'] },
      ],
    });
  });

  it('records the folder and scope transition when exclusions change', async () => {
    vi.spyOn(PreferenceStorageService, 'saveScanExclusionPreferences').mockResolvedValue();
    const info = vi.spyOn(LoggerService, 'info').mockImplementation(() => undefined);
    const store = useStorageScanPreferencesStore();
    store.initialized = true;
    store.folders = [{ path: '/fixture/cache', scopes: ['largeFiles'] }];

    await store.saveFolders([{ path: '/fixture/cache', scopes: ['largeFiles', 'analysis'] }]);

    expect(info).toHaveBeenCalledWith(
      'storage-scan',
      'exclusions_updated',
      expect.objectContaining({
        changedFolders: [
          {
            path: '/fixture/cache',
            previousScopes: ['largeFiles'],
            currentScopes: ['largeFiles', 'analysis'],
          },
        ],
      })
    );
  });
});

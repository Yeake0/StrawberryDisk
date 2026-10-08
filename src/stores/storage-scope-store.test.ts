import { createPinia, setActivePinia } from 'pinia';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { DiskInfo } from '@/lib/models/disk';
import { STORAGE_SCOPE_IDS } from '@/lib/models/storage-scope';
import { FolderSelectionService } from '@/lib/services/folder-selection-service';
import { PreferenceStorageService } from '@/lib/services/preference-storage-service';
import { STANDARD_SCAN_FOLDER_IDS, StandardScanFolderService } from '@/lib/services/standard-scan-folder-service';

import { useStorageScopeStore } from './storage-scope-store';

const { loadMock, values } = vi.hoisted(() => {
  const storedValues = new Map<string, unknown>();
  const store = {
    get: vi.fn(async (key: string) => storedValues.get(key)),
    set: vi.fn(async (key: string, value: unknown) => {
      storedValues.set(key, value);
    }),
    delete: vi.fn(async (key: string) => storedValues.delete(key)),
    save: vi.fn(async () => undefined),
  };
  return {
    loadMock: vi.fn(async () => store),
    values: storedValues,
  };
});

vi.mock('@tauri-apps/plugin-store', () => ({ load: loadMock }));
const disks: DiskInfo[] = [
  {
    name: 'System',
    mountPoint: '/',
    totalBytes: 1_000,
    availableBytes: 400,
    usedBytes: 600,
  },
];

describe('storage scope store', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    values.clear();
    vi.restoreAllMocks();
    vi.spyOn(StandardScanFolderService, 'listAvailable').mockResolvedValue([
      { id: STANDARD_SCAN_FOLDER_IDS.downloads, path: '/Users/example/Downloads' },
    ]);
  });

  it('restores every selected directory independently of the eight-item history', async () => {
    const paths = Array.from({ length: 70 }, (_, index) => `/fixture/${index}`);
    const store = useStorageScopeStore();
    store.selectPaths('duplicate-files', paths, disks);
    expect(store.recentFolders).toHaveLength(8);
    const recent = [...store.recentFolders];
    store.selectPaths('duplicate-files', paths.slice(1), disks);
    expect(store.recentFolders).toEqual(recent);
    store.selectPaths('duplicate-files', paths, disks);
    const resolve = vi
      .spyOn(FolderSelectionService, 'resolveDirectories')
      .mockImplementation(async values => values.map(path => ({ requestedPath: path, path })));
    setActivePinia(createPinia());
    const restored = useStorageScopeStore();
    await restored.initialize(disks);
    expect(restored.selectedPaths['duplicate-files']).toEqual(paths);
    expect(resolve.mock.calls.every(([batch]) => batch.length <= 64)).toBe(true);
    expect(restored.recentFolders).toHaveLength(8);
    restored.removeFolder(paths[0]!);
    expect(restored.selectedPaths['duplicate-files']).toEqual(paths.slice(1));
  });

  it('persists deselecting every location and deduplicates Windows aliases', async () => {
    const store = useStorageScopeStore();
    store.selectPaths('duplicate-files', ['E:\\Work', 'e:/work/', 'F:\\Chat'], disks);
    expect(store.selectedPaths['duplicate-files']).toEqual(['E:\\Work', 'F:\\Chat']);
    store.selectPaths('duplicate-files', [], disks);
    await expect(PreferenceStorageService.loadStorageScopePreferences()).resolves.toMatchObject({
      selectedPaths: { 'duplicate-files': [] },
    });
  });

  it('keeps large-file and duplicate multi-selections independent', async () => {
    const store = useStorageScopeStore();
    store.selectPaths('duplicate-files', ['/chat'], disks);
    store.selectPaths('large-files', ['/work', '/downloads'], disks);
    store.selectPaths('large-files', [], disks);
    await expect(PreferenceStorageService.loadStorageScopePreferences()).resolves.toMatchObject({
      schemaVersion: 3,
      selectedPaths: { 'large-files': [], 'duplicate-files': ['/chat'] },
      duplicateFileProtectedPaths: [],
    });
  });

  it('persists separate page selections and shared recent folders', async () => {
    const store = useStorageScopeStore();

    store.select(STORAGE_SCOPE_IDS.analysis, '/Users/example/Downloads', disks);
    store.select(STORAGE_SCOPE_IDS.largeFiles, '/Users/example/Movies', disks);
    store.select(STORAGE_SCOPE_IDS.duplicateFiles, '/Users/example/Documents', disks);

    expect(store.selectedPath(STORAGE_SCOPE_IDS.analysis)).toBe('/Users/example/Downloads');
    expect(store.selectedPath(STORAGE_SCOPE_IDS.largeFiles)).toBe('/Users/example/Movies');
    expect(store.selectedPath(STORAGE_SCOPE_IDS.duplicateFiles)).toBe('/Users/example/Documents');
    expect(store.recentFolders).toEqual([
      '/Users/example/Documents',
      '/Users/example/Movies',
      '/Users/example/Downloads',
    ]);
    await expect(PreferenceStorageService.loadStorageScopePreferences()).resolves.toEqual({
      schemaVersion: 3,
      selectedPaths: {
        analysis: '/Users/example/Downloads',
        'large-files': ['/Users/example/Movies'],
        'duplicate-files': ['/Users/example/Documents'],
      },
      recentFolders: ['/Users/example/Documents', '/Users/example/Movies', '/Users/example/Downloads'],
      duplicateFileProtectedPaths: [],
    });
  });

  it('persists duplicate-file protection separately from scan selection', async () => {
    const store = useStorageScopeStore();
    store.selectPaths('duplicate-files', ['E:\\Work', 'F:\\Chat'], disks);
    store.setDuplicateFileProtectedPaths(['E:\\Work', 'G:\\Stale']);

    await expect(PreferenceStorageService.loadStorageScopePreferences()).resolves.toMatchObject({
      schemaVersion: 3,
      selectedPaths: { 'duplicate-files': ['E:\\Work', 'F:\\Chat'] },
      duplicateFileProtectedPaths: ['E:\\Work'],
    });

    store.selectPaths('duplicate-files', ['F:\\Chat'], disks);
    expect(store.duplicateFileProtectedPaths).toEqual([]);
  });

  it('does not add disk roots to folder history', () => {
    const store = useStorageScopeStore();

    store.select(STORAGE_SCOPE_IDS.analysis, '/', disks);

    expect(store.selectedPath(STORAGE_SCOPE_IDS.analysis)).toBe('/');
    expect(store.recentFolders).toEqual([]);
  });

  it('shares one in-flight initialization and publishes only the completed snapshot', async () => {
    const store = useStorageScopeStore();
    let resolveStandardFolders!: (folders: Awaited<ReturnType<typeof StandardScanFolderService.listAvailable>>) => void;
    vi.mocked(StandardScanFolderService.listAvailable).mockImplementationOnce(
      () =>
        new Promise(resolve => {
          resolveStandardFolders = resolve;
        })
    );

    const firstInitialization = store.initialize(disks);
    const secondInitialization = store.initialize(disks);
    let secondInitializationCompleted = false;
    void secondInitialization.then(() => {
      secondInitializationCompleted = true;
    });

    await Promise.resolve();
    expect(store.initialized).toBe(false);
    expect(secondInitializationCompleted).toBe(false);
    expect(StandardScanFolderService.listAvailable).toHaveBeenCalledTimes(1);

    resolveStandardFolders([{ id: STANDARD_SCAN_FOLDER_IDS.downloads, path: '/Users/example/Downloads' }]);
    await Promise.all([firstInitialization, secondInitialization]);
    await store.initialize(disks);

    expect(store.initialized).toBe(true);
    expect(store.standardFolders).toEqual([
      { id: STANDARD_SCAN_FOLDER_IDS.downloads, path: '/Users/example/Downloads' },
    ]);
    expect(StandardScanFolderService.listAvailable).toHaveBeenCalledTimes(1);
  });

  it('allows initialization to retry after an unexpected failure', async () => {
    const store = useStorageScopeStore();
    vi.spyOn(store, 'loadStandardFolders').mockRejectedValueOnce(new Error('unexpected initialization failure'));

    await expect(store.initialize(disks)).rejects.toThrow('unexpected initialization failure');
    expect(store.initialized).toBe(false);

    await expect(store.initialize(disks)).resolves.toBeUndefined();
    expect(store.initialized).toBe(true);
    expect(StandardScanFolderService.listAvailable).toHaveBeenCalledTimes(1);
  });

  it('removes a folder from history and every page selection', () => {
    const store = useStorageScopeStore();
    store.select(STORAGE_SCOPE_IDS.analysis, '/Users/example/Downloads', disks);
    store.select(STORAGE_SCOPE_IDS.largeFiles, '/Users/example/Downloads', disks);
    store.select(STORAGE_SCOPE_IDS.duplicateFiles, '/Users/example/Downloads', disks);

    store.removeFolder('/Users/example/Downloads');

    expect(store.selectedPaths).toEqual({ 'large-files': [], 'duplicate-files': [] });
    expect(store.recentFolders).toEqual([]);
  });

  it('migrates saved aliases even when every folder still exists', async () => {
    const alias = 'C:\\Mac\\Home\\Documents';
    const target = '\\\\psf\\Home\\Documents';
    await PreferenceStorageService.saveStorageScopePreferences({
      selectedPaths: { analysis: alias, 'large-files': target },
      recentFolders: [alias, target],
    });
    vi.spyOn(FolderSelectionService, 'resolveDirectories').mockResolvedValue([
      { requestedPath: alias, path: target },
      { requestedPath: target, path: target },
    ]);
    const store = useStorageScopeStore();
    await store.initialize(disks);
    expect(store.recentFolders).toEqual([target]);
    expect(store.selectedPaths).toEqual({ analysis: target, 'large-files': [target] });
  });

  it('does not rewrite unchanged folder preferences during restoration', async () => {
    const path = '/Users/example/Downloads';
    await PreferenceStorageService.saveStorageScopePreferences({
      selectedPaths: { analysis: path },
      recentFolders: [path],
    });
    vi.spyOn(FolderSelectionService, 'resolveDirectories').mockResolvedValue([{ requestedPath: path, path }]);
    const saveSpy = vi.spyOn(PreferenceStorageService, 'saveStorageScopePreferences');

    await useStorageScopeStore().initialize(disks);

    expect(saveSpy).not.toHaveBeenCalled();
  });

  it('drops missing folders while restoring saved preferences', async () => {
    await PreferenceStorageService.saveStorageScopePreferences({
      selectedPaths: {
        analysis: '/Users/example/Missing',
        'large-files': '/Users/example/Downloads',
      },
      recentFolders: ['/Users/example/Missing', '/Users/example/Downloads'],
    });
    vi.spyOn(FolderSelectionService, 'resolveDirectories').mockResolvedValue([
      { requestedPath: '/Users/example/Downloads', path: '/Users/example/Downloads' },
    ]);

    const store = useStorageScopeStore();
    await store.initialize(disks);

    expect(store.selectedPaths).toEqual({
      'large-files': ['/Users/example/Downloads'],
    });
    expect(store.recentFolders).toEqual(['/Users/example/Downloads']);
    await vi.waitFor(() => {
      expect(values.get('storageScopePreferences')).toMatchObject({
        selectedPaths: {
          'large-files': ['/Users/example/Downloads'],
        },
      });
    });
  });
});

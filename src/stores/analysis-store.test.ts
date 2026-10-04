import { createPinia, setActivePinia } from 'pinia';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { PAGE_IDS } from '@/lib/models/application-shell';
import type { AnalysisResult, DirectoryEntryInfo } from '@/lib/models/analysis';
import { AnalysisService } from '@/lib/services/analysis-service';
import * as AnalysisCacheUtils from '@/lib/utils/analysis-cache';

import { useAnalysisStore } from './analysis-store';
import { useAppStore } from './app-store';
import { useStorageScanPreferencesStore } from './storage-scan-preferences-store';

const result: AnalysisResult = {
  scanId: 7,
  root: '/fixture',
  scannedAtMs: 1_000,
  totalBytes: 64,
  skippedCount: 0,
  truncated: false,
  entries: [],
};

const entry: DirectoryEntryInfo = {
  name: 'fixture.bin',
  path: '/fixture/fixture.bin',
  bytes: 64,
  isDirectory: false,
  fileCount: 1,
  modifiedAtMs: null,
  contentFingerprint: null,
};

describe('analysis store', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.restoreAllMocks();
    useStorageScanPreferencesStore().initialized = true;
    vi.spyOn(AnalysisService, 'listenProgress').mockResolvedValue(vi.fn());
  });

  it('preserves the page selected while an analysis is running', async () => {
    let completeAnalysis: (value: AnalysisResult) => void = () => undefined;
    const analyze = vi.spyOn(AnalysisService, 'analyze').mockImplementation(
      () =>
        new Promise(resolve => {
          completeAnalysis = resolve;
        })
    );
    const appStore = useAppStore();
    const analysisStore = useAnalysisStore();

    const analysis = analysisStore.analyze('/fixture', true, true);
    await vi.waitFor(() => expect(analyze).toHaveBeenCalledOnce());
    appStore.navigate(PAGE_IDS.settings);
    completeAnalysis(result);
    await analysis;

    expect(appStore.currentPage).toBe(PAGE_IDS.settings);
    expect(analysisStore.result).toEqual(result);
  });

  it('starts the pending state before loading preferences and cancels before the native scan', async () => {
    let finishInitialization: () => void = () => undefined;
    const preferences = useStorageScanPreferencesStore();
    vi.spyOn(preferences, 'initialize').mockImplementation(
      () =>
        new Promise<void>(resolve => {
          finishInitialization = resolve;
        })
    );
    const analyze = vi.spyOn(AnalysisService, 'analyze');
    const cancel = vi.spyOn(AnalysisService, 'cancel');
    const analysisStore = useAnalysisStore();
    analysisStore.result = result;

    const request = analysisStore.analyze('/fixture', true);
    expect(analysisStore.pending).toBe(true);
    expect(analysisStore.result).toEqual(result);

    await analysisStore.cancel();
    finishInitialization();
    await request;

    expect(analyze).not.toHaveBeenCalled();
    expect(cancel).not.toHaveBeenCalled();
    expect(analysisStore.result).toEqual(result);
    expect(analysisStore.pending).toBe(false);
    expect(analysisStore.cancelling).toBe(false);
  });

  it('does not navigate when showing a cached result', async () => {
    const appStore = useAppStore();
    const analysisStore = useAnalysisStore();
    const cacheKey = AnalysisCacheUtils.key('/fixture');
    analysisStore.cache = { [cacheKey]: result };
    analysisStore.cacheOrder = [cacheKey];
    appStore.navigate(PAGE_IDS.settings);
    const analyze = vi.spyOn(AnalysisService, 'analyze');

    await analysisStore.analyze('/fixture');

    expect(analyze).not.toHaveBeenCalled();
    expect(appStore.currentPage).toBe(PAGE_IDS.settings);
    expect(analysisStore.result).toEqual(result);
  });

  it('keeps full-space analysis independent from scan exclusion preferences', async () => {
    const preferences = useStorageScanPreferencesStore();
    preferences.initialized = true;
    preferences.folders = [{ path: '/fixture/cache', scopes: ['largeFiles', 'duplicateFiles'] }];
    const analysisStore = useAnalysisStore();
    const cacheKey = AnalysisCacheUtils.key('/fixture');
    analysisStore.cache = { [cacheKey]: result };
    analysisStore.cacheOrder = [cacheKey];
    const analyze = vi.spyOn(AnalysisService, 'analyze').mockResolvedValue(result);

    await analysisStore.analyze('/fixture');

    expect(analyze).not.toHaveBeenCalled();
    expect(analysisStore.result).toEqual(result);
  });

  it('passes only analysis-scoped folders to a new scan', async () => {
    const preferences = useStorageScanPreferencesStore();
    preferences.folders = [
      { path: '/fixture/cache', scopes: ['analysis', 'largeFiles'] },
      { path: '/fixture/downloads', scopes: ['duplicateFiles'] },
    ];
    const analyze = vi.spyOn(AnalysisService, 'analyze').mockResolvedValue(result);
    const analysisStore = useAnalysisStore();

    await analysisStore.analyze('/fixture');

    expect(analyze).toHaveBeenCalledWith('/fixture', false, ['/fixture/cache'], []);
    expect(analysisStore.scanExcludedFolders).toEqual(['/fixture/cache']);
    expect(analysisStore.result).toEqual(result);
  });

  it('discards every cached folder result when analysis exclusions change', async () => {
    const analysisStore = useAnalysisStore();
    const cacheKey = AnalysisCacheUtils.key('/fixture');
    const childKey = AnalysisCacheUtils.key('/fixture/child');
    analysisStore.result = result;
    analysisStore.cache = { [cacheKey]: result, [childKey]: { ...result, root: '/fixture/child' } };
    analysisStore.cacheOrder = [cacheKey, childKey];
    useStorageScanPreferencesStore().folders = [{ path: '/fixture/cache', scopes: ['analysis'] }];
    const analyze = vi.spyOn(AnalysisService, 'analyze').mockResolvedValue(result);

    analysisStore.invalidateResultForExclusionChange();

    expect(analysisStore.result).toBeNull();
    expect(analysisStore.cache).toEqual({});
    expect(analysisStore.cacheOrder).toEqual([]);
    await analysisStore.analyze('/fixture');
    expect(analyze).toHaveBeenCalledWith('/fixture', false, ['/fixture/cache'], []);
  });

  it('ignores a scan that finishes after its exclusions change', async () => {
    let completeAnalysis: (value: AnalysisResult) => void = () => undefined;
    const analyze = vi.spyOn(AnalysisService, 'analyze').mockImplementation(
      () =>
        new Promise(resolve => {
          completeAnalysis = resolve;
        })
    );
    const analysisStore = useAnalysisStore();
    const analysis = analysisStore.analyze('/fixture');
    await vi.waitFor(() => expect(analyze).toHaveBeenCalledOnce());
    useStorageScanPreferencesStore().folders = [{ path: '/fixture/cache', scopes: ['analysis'] }];
    completeAnalysis(result);
    await analysis;

    expect(analysisStore.result).toBeNull();
    expect(analysisStore.cache).toEqual({});
    expect(analysisStore.scanExcludedFolders).toEqual(['/fixture/cache']);
  });

  it('rejects deletion while an analysis is active', async () => {
    const remove = vi.spyOn(AnalysisService, 'deletePermanently');
    const analysisStore = useAnalysisStore();
    analysisStore.result = { ...result, entries: [entry] };
    analysisStore.pending = true;

    await analysisStore.deletePermanently(entry);

    expect(remove).not.toHaveBeenCalled();
    expect(analysisStore.deleting).toBe(false);
  });

  it('refreshes a recreated original path instead of removing its new contents from view', async () => {
    vi.spyOn(AnalysisService, 'deletePermanently').mockResolvedValue({
      requiresRescan: true,
      removedPath: entry.path,
      releasedBytes: entry.bytes,
      removedFileCount: 1,
    });
    const refreshed = { ...result, scanId: 99, entries: [{ ...entry, bytes: 1 }] };
    const analyze = vi.spyOn(AnalysisService, 'analyze').mockResolvedValue(refreshed);
    vi.spyOn(useAppStore(), 'refreshSystemDisk').mockResolvedValue(true);
    const store = useAnalysisStore();
    store.result = { ...result, entries: [entry] };
    store.cache = { [AnalysisCacheUtils.key(result.root)]: store.result };
    await store.deletePermanently(entry);
    expect(analyze).toHaveBeenCalledOnce();
    expect(store.result).toEqual(refreshed);
    expect(store.deletingPath).toBeNull();
  });

  it('drops sibling snapshots when deletion requires allocation reconciliation', async () => {
    vi.spyOn(AnalysisService, 'deletePermanently').mockResolvedValue({
      requiresRescan: true,
      removedPath: entry.path,
      releasedBytes: 64,
      removedFileCount: 1,
    });
    const refreshed = { ...result, scanId: 99, entries: [] };
    vi.spyOn(AnalysisService, 'analyze').mockResolvedValue(refreshed);
    vi.spyOn(useAppStore(), 'refreshSystemDisk').mockResolvedValue(true);
    const store = useAnalysisStore();
    store.result = { ...result, entries: [entry] };
    store.cache = { '/fixture': store.result, '/sibling': { ...result, root: '/sibling', totalBytes: 0 } };
    store.cacheOrder = Object.keys(store.cache);
    await store.deletePermanently(entry);
    expect(Object.keys(store.cache)).toEqual(['/fixture']);
    expect(store.result).toEqual(refreshed);
  });

  it('refreshes shared disk capacity after a completed deletion', async () => {
    vi.spyOn(AnalysisService, 'deletePermanently').mockResolvedValue({
      requiresRescan: false,
      removedPath: entry.path,
      releasedBytes: entry.bytes,
      removedFileCount: entry.fileCount,
    });
    const appStore = useAppStore();
    const refreshDisk = vi.spyOn(appStore, 'refreshSystemDisk').mockResolvedValue(true);
    const analysisStore = useAnalysisStore();
    analysisStore.result = { ...result, entries: [entry], totalBytes: entry.bytes };
    analysisStore.cache = { [AnalysisCacheUtils.key(result.root)]: analysisStore.result };
    analysisStore.cacheOrder = [AnalysisCacheUtils.key(result.root)];

    await analysisStore.deletePermanently(entry);

    expect(refreshDisk).toHaveBeenCalledOnce();
    expect(analysisStore.result?.entries).toEqual([]);
  });

  it('explains when a cancelled native scan is still releasing resources', async () => {
    vi.spyOn(AnalysisService, 'analyze').mockRejectedValue({
      code: 'operationBusy',
      details: { operation: 'analyze_path', reason: 'scanResourcesReleasing' },
      retryable: true,
    });
    const appStore = useAppStore();
    const analysisStore = useAnalysisStore();

    await analysisStore.analyze('/fixture', true);

    expect(appStore.errorCode).toBe('operationBusy');
    expect(appStore.errorReason).toBe('scanResourcesReleasing');
    expect(analysisStore.pending).toBe(false);
  });
  it('marks only the requested path busy and rejects duplicate deletion requests', async () => {
    let finish: (value: {
      requiresRescan: boolean;
      removedPath: string;
      releasedBytes: number;
      removedFileCount: number;
    }) => void = () => undefined;
    const remove = vi.spyOn(AnalysisService, 'deletePermanently').mockImplementation(
      () =>
        new Promise(resolve => {
          finish = resolve;
        })
    );
    vi.spyOn(useAppStore(), 'refreshSystemDisk').mockResolvedValue(true);
    const store = useAnalysisStore();
    store.result = { ...result, entries: [entry] };
    store.cache = { [AnalysisCacheUtils.key(result.root)]: store.result };
    const request = store.deletePermanently(entry);
    expect(store.deletingPath).toBe(entry.path);
    expect(store.deleting).toBe(true);
    await store.deletePermanently(entry);
    expect(remove).toHaveBeenCalledOnce();
    finish({ requiresRescan: false, removedPath: entry.path, releasedBytes: 64, removedFileCount: 1 });
    await request;
    expect(store.deletingPath).toBeNull();
    expect(store.deleting).toBe(false);
  });

  it('refreshes a partial deletion and expires ancestor and descendant snapshots', async () => {
    const failure = {
      code: 'operationFailed',
      retryable: true,
      details: { reason: 'directoryNotEmpty', mutationState: 'mayHaveChanged' },
    };
    vi.spyOn(AnalysisService, 'deletePermanently').mockRejectedValue(failure);
    let finishRefresh: (value: AnalysisResult) => void = () => undefined;
    const scan = vi.spyOn(AnalysisService, 'analyze').mockImplementation(
      () =>
        new Promise(resolve => {
          finishRefresh = resolve;
        })
    );
    const refreshDisk = vi.spyOn(useAppStore(), 'refreshSystemDisk').mockResolvedValue(true);
    const store = useAnalysisStore();
    store.result = { ...result, entries: [entry] };
    store.cache = {
      '/fixture': store.result,
      '/': { ...result, root: '/' },
      [entry.path]: { ...result, root: entry.path },
      '/unrelated': { ...result, root: '/unrelated' },
    };
    store.cacheOrder = Object.keys(store.cache);
    const request = store.deletePermanently(entry);
    await vi.waitFor(() => expect(scan).toHaveBeenCalledOnce());
    expect(store.deletingPath).toBeNull();
    expect(store.pending).toBe(true);
    expect(Object.keys(store.cache)).toEqual(['/unrelated']);
    const refreshed = { ...result, scanId: 8, entries: [{ ...entry, bytes: 20 }], totalBytes: 20 };
    finishRefresh(refreshed);
    await request;
    expect(store.result).toEqual(refreshed);
    expect(store.cache['/']).toBeUndefined();
    expect(store.cache[entry.path]).toBeUndefined();
    expect(store.deletingPath).toBeNull();
    expect(useAppStore().errorReason).toBe('directoryNotEmpty');
    expect(refreshDisk).toHaveBeenCalledOnce();
  });

  it('does not revive old results when refreshing a partial deletion fails', async () => {
    vi.spyOn(AnalysisService, 'deletePermanently').mockRejectedValue({
      code: 'operationFailed',
      retryable: true,
      details: { mutationState: 'mayHaveChanged', reason: 'deleteRecoveryFailed' },
    });
    vi.spyOn(AnalysisService, 'analyze').mockRejectedValue(new Error('unavailable'));
    vi.spyOn(useAppStore(), 'refreshSystemDisk').mockResolvedValue(true);
    const store = useAnalysisStore();
    store.result = { ...result, entries: [entry] };
    store.cache = { '/fixture': store.result };
    await store.deletePermanently(entry);
    expect(store.result).toBeNull();
    expect(store.cache).toEqual({});
    expect(store.deletingPath).toBeNull();
    expect(useAppStore().errorReason).toBe('deleteRecoveryFailed');
  });

  it('shows a partial deletion failure before recovery scanning and allows cancellation', async () => {
    vi.spyOn(AnalysisService, 'deletePermanently').mockRejectedValue({
      code: 'operationFailed',
      retryable: true,
      details: { mutationState: 'mayHaveChanged', reason: 'directoryNotEmpty' },
    });
    let finishScan: (value: AnalysisResult) => void = () => undefined;
    const analyze = vi.spyOn(AnalysisService, 'analyze').mockImplementation(
      () =>
        new Promise(resolve => {
          finishScan = resolve;
        })
    );
    const cancel = vi.spyOn(AnalysisService, 'cancel').mockResolvedValue();
    const store = useAnalysisStore();
    store.result = { ...result, entries: [entry] };
    store.cache = { [AnalysisCacheUtils.key(result.root)]: store.result };

    const request = store.deletePermanently(entry);
    await vi.waitFor(() => expect(analyze).toHaveBeenCalledOnce());
    expect(useAppStore().errorReason).toBe('directoryNotEmpty');
    expect(store.result).toBeNull();
    expect(store.deleting).toBe(false);
    expect(store.pending).toBe(true);
    await store.cancel();
    expect(cancel).toHaveBeenCalledOnce();
    finishScan(result);
    await request;
    expect(store.pending).toBe(false);
    expect(useAppStore().errorReason).toBe('directoryNotEmpty');
  });

  it('reports when deletion succeeds but its result cannot be refreshed', async () => {
    vi.spyOn(AnalysisService, 'deletePermanently').mockResolvedValue({
      requiresRescan: true,
      removedPath: entry.path,
      releasedBytes: entry.bytes,
      removedFileCount: 1,
    });
    vi.spyOn(AnalysisService, 'analyze').mockRejectedValue({
      code: 'permissionDenied',
      retryable: false,
      details: {},
    });
    vi.spyOn(useAppStore(), 'refreshSystemDisk').mockResolvedValue(true);
    const store = useAnalysisStore();
    store.result = { ...result, entries: [entry] };
    store.cache = { [AnalysisCacheUtils.key(result.root)]: store.result };

    await store.deletePermanently(entry);
    expect(store.result).toBeNull();
    expect(store.pending).toBe(false);
    expect(useAppStore().errorReason).toBe('analysisRefreshFailedAfterDelete');
  });

  it('keeps an unchanged result when deletion is rejected before mutation', async () => {
    vi.spyOn(AnalysisService, 'deletePermanently').mockRejectedValue({
      code: 'operationFailed',
      retryable: true,
      details: { reason: 'itemChanged' },
    });
    const scan = vi.spyOn(AnalysisService, 'analyze');
    const store = useAnalysisStore();
    store.result = { ...result, entries: [entry] };
    await store.deletePermanently(entry);
    expect(scan).not.toHaveBeenCalled();
    expect(store.result?.entries).toEqual([entry]);
    expect(store.deletingPath).toBeNull();
  });

  it('does not retain an ancestor scan ID that Core expires after successful deletion', async () => {
    vi.spyOn(AnalysisService, 'deletePermanently').mockResolvedValue({
      requiresRescan: false,
      removedPath: entry.path,
      releasedBytes: 64,
      removedFileCount: 1,
    });
    vi.spyOn(useAppStore(), 'refreshSystemDisk').mockResolvedValue(true);
    const store = useAnalysisStore();
    store.result = { ...result, entries: [entry] };
    store.cache = {
      '/fixture': store.result,
      '/': { ...result, root: '/' },
      '/unrelated': { ...result, root: '/unrelated' },
    };
    await store.deletePermanently(entry);
    expect(store.cache['/']).toBeUndefined();
    expect(store.cache['/unrelated']).toBeDefined();
    expect(store.result?.entries).toEqual([]);
    expect(store.result?.totalBytes).toBe(0);
  });
});

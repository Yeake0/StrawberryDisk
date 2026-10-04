import { createPinia, setActivePinia } from 'pinia';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { AnalysisViewPreferences } from '@/lib/models/analysis';
import { PreferenceStorageService } from '@/lib/services/preference-storage-service';
import { LoggerService } from '@/lib/services/logger-service';
import { AnalysisService } from '@/lib/services/analysis-service';
import { useAnalysisStore } from './analysis-store';

describe('analysis preference lifecycle', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.spyOn(PreferenceStorageService, 'loadAnalysisViewPreferences').mockResolvedValue(null);
    vi.spyOn(PreferenceStorageService, 'saveAnalysisViewPreferences').mockResolvedValue();
    vi.spyOn(LoggerService, 'warn').mockImplementation(() => undefined);
  });

  afterEach(() => vi.restoreAllMocks());

  it('loads once and does not write defaults on startup', async () => {
    const store = useAnalysisStore();
    await Promise.all([store.initializeViewPreferences(), store.initializeViewPreferences()]);
    await store.initializeViewPreferences();
    expect(PreferenceStorageService.loadAnalysisViewPreferences).toHaveBeenCalledOnce();
    expect(PreferenceStorageService.saveAnalysisViewPreferences).not.toHaveBeenCalled();
    expect(store.viewPreferencesInitialized).toBe(true);
  });

  it('restores both independent depths in a new application session without scanning', async () => {
    let saved: AnalysisViewPreferences | null = null;
    vi.mocked(PreferenceStorageService.loadAnalysisViewPreferences).mockImplementation(async () => saved);
    vi.mocked(PreferenceStorageService.saveAnalysisViewPreferences).mockImplementation(async value => {
      saved = { ...value };
    });
    const scan = vi.spyOn(AnalysisService, 'analyze');
    const store = useAnalysisStore();
    await store.initializeViewPreferences();
    store.setChartDepth('treemap', 2);
    store.setChartDepth('sunburst', 5);
    store.setViewMode('sunburst');
    setActivePinia(createPinia());
    const restarted = useAnalysisStore();
    await restarted.initializeViewPreferences();
    expect(restarted.viewPreferences).toEqual({
      schemaVersion: 1,
      viewMode: 'sunburst',
      treemapDepth: 2,
      sunburstDepth: 5,
    });
    restarted.setViewMode('treemap');
    expect(restarted.viewPreferences.treemapDepth).toBe(2);
    expect(restarted.viewPreferences.sunburstDepth).toBe(5);
    expect(scan).not.toHaveBeenCalled();
  });

  it('preserves user changes during a delayed read and restores untouched fields', async () => {
    let restore: (value: unknown) => void = () => undefined;
    vi.mocked(PreferenceStorageService.loadAnalysisViewPreferences).mockImplementation(
      () =>
        new Promise(resolve => {
          restore = resolve;
        })
    );
    const store = useAnalysisStore();
    const loading = store.initializeViewPreferences();
    store.setViewMode('sunburst');
    store.setChartDepth('treemap', 1);
    expect(store.viewPreferences.viewMode).toBe('sunburst');
    expect(PreferenceStorageService.saveAnalysisViewPreferences).not.toHaveBeenCalled();
    restore({ schemaVersion: 1, viewMode: 'treemap', treemapDepth: 6, sunburstDepth: 4 });
    await loading;
    expect(store.viewPreferences).toEqual({
      schemaVersion: 1,
      viewMode: 'sunburst',
      treemapDepth: 1,
      sunburstDepth: 4,
    });
    expect(PreferenceStorageService.saveAnalysisViewPreferences).toHaveBeenCalledOnce();
    expect(PreferenceStorageService.saveAnalysisViewPreferences).toHaveBeenCalledWith(store.viewPreferences);
  });

  it('keeps interactions immediate while writes are blocked and skips unchanged choices', async () => {
    const store = useAnalysisStore();
    await store.initializeViewPreferences();
    let finish: () => void = () => undefined;
    vi.mocked(PreferenceStorageService.saveAnalysisViewPreferences).mockImplementation(
      () =>
        new Promise(resolve => {
          finish = resolve;
        })
    );
    store.setViewMode('treemap');
    store.setChartDepth('treemap', 1);
    expect(PreferenceStorageService.saveAnalysisViewPreferences).not.toHaveBeenCalled();
    store.setViewMode('sunburst');
    expect(store.viewPreferences.viewMode).toBe('sunburst');
    finish();
    await Promise.resolve();
  });

  it('falls back on read failure and retains the selected view on save failure', async () => {
    vi.mocked(PreferenceStorageService.loadAnalysisViewPreferences).mockRejectedValue(new Error('read failed'));
    const store = useAnalysisStore();
    await store.initializeViewPreferences();
    expect(store.viewPreferencesInitialized).toBe(true);
    vi.mocked(PreferenceStorageService.saveAnalysisViewPreferences).mockRejectedValue(new Error('write failed'));
    store.setViewMode('sunburst');
    await Promise.resolve();
    expect(store.viewPreferences.viewMode).toBe('sunburst');
    expect(LoggerService.warn).toHaveBeenCalledTimes(2);
  });

  it('ignores unsupported user choices without writing or changing preferences', async () => {
    const store = useAnalysisStore();
    await store.initializeViewPreferences();
    for (const depth of [0, 7, 1.5, NaN]) store.setChartDepth('treemap', depth);
    store.setChartDepth('sunburst', 1);
    expect(store.viewPreferences).toEqual({ schemaVersion: 1, viewMode: 'treemap', treemapDepth: 1, sunburstDepth: 3 });
    expect(PreferenceStorageService.saveAnalysisViewPreferences).not.toHaveBeenCalled();
  });
});

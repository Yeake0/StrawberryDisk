// @vitest-environment happy-dom

import { flushPromises, shallowMount } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import MdStorageScanExclusionsDialog from '@/components/custom/md-storage-scan-exclusions-dialog.vue';
import type { AnalysisResult } from '@/lib/models/analysis';
import type { CleanupScanResult } from '@/lib/models/cleanup';
import type { DuplicateFilesResult } from '@/lib/models/duplicate-file';
import type { LargeFilesResult } from '@/lib/models/large-file';
import { SCAN_EXCLUSION_PREFERENCES_SCHEMA_VERSION, type ScanExclusionPreferences } from '@/lib/models/storage-scan';
import { PreferenceStorageService } from '@/lib/services/preference-storage-service';
import { useAnalysisStore } from '@/stores/analysis-store';
import { useCleanupStore } from '@/stores/cleanup-store';
import { useDuplicateFilesStore } from '@/stores/duplicate-files-store';
import { useLargeFilesStore } from '@/stores/large-files-store';
import { useStorageScanPreferencesStore } from '@/stores/storage-scan-preferences-store';

import MdStorageScanExclusionsEditor from './md-storage-scan-exclusions-editor.vue';

beforeEach(() => {
  setActivePinia(createPinia());
  vi.restoreAllMocks();
});

describe('shared scan exclusion editor', () => {
  it('waits for saved folders before opening the editable dialog', async () => {
    let finishLoad: (value: ScanExclusionPreferences) => void = () => undefined;
    vi.spyOn(PreferenceStorageService, 'loadScanExclusionPreferences').mockImplementation(
      () =>
        new Promise(resolve => {
          finishLoad = resolve;
        })
    );
    const wrapper = shallowMount(MdStorageScanExclusionsEditor);

    const opening = wrapper.vm.open();
    expect(wrapper.findComponent(MdStorageScanExclusionsDialog).exists()).toBe(false);

    finishLoad({
      schemaVersion: SCAN_EXCLUSION_PREFERENCES_SCHEMA_VERSION,
      names: [],
      folders: [{ path: '/saved/folder', scopes: ['largeFiles'] }],
    });
    await opening;
    await flushPromises();

    expect(wrapper.getComponent(MdStorageScanExclusionsDialog).props('modelValue')).toBe(true);
    expect(wrapper.getComponent(MdStorageScanExclusionsDialog).props('folders')).toEqual([
      { path: '/saved/folder', scopes: ['largeFiles'] },
    ]);
    wrapper.unmount();
  });

  it('keeps the deep-cleanup scan and selection after saving changed exclusions', async () => {
    const preferences = useStorageScanPreferencesStore();
    preferences.initialized = true;
    vi.spyOn(PreferenceStorageService, 'saveScanExclusionPreferences').mockResolvedValue();
    const cleanup = useCleanupStore();
    const scan: CleanupScanResult = {
      schemaVersion: '2',
      customScanId: null,
      scannedAtMs: 1,
      disk: { name: 'Fixture', mountPoint: '/', totalBytes: 1_000, availableBytes: 500, usedBytes: 500 },
      rules: [],
      applicationIcons: [],
      warningCount: 0,
      accessLimited: false,
      readFailureCount: 0,
      safeBytes: 0,
      reclaimableBytes: 0,
      applicabilityElapsedMs: 0,
      applicableRuleCount: 0,
      filteredRuleCount: 0,
      inventoryApplicationCount: 0,
      inventoryProcessCount: 0,
      elapsedMs: 0,
    };
    cleanup.scan = scan;
    cleanup.scanExcludedFolders = [];
    cleanup.selectedRuleIds = ['project.rust-build-artifacts'];
    const wrapper = shallowMount(MdStorageScanExclusionsEditor);
    await wrapper.vm.open();

    wrapper
      .getComponent(MdStorageScanExclusionsDialog)
      .vm.$emit('save', [{ path: '/fixture/excluded', scopes: ['cleanup'] }]);
    await flushPromises();

    expect(cleanup.scan).toEqual(scan);
    expect(cleanup.scanExcludedFolders).toEqual([]);
    expect(cleanup.selectedRuleIds).toEqual(['project.rust-build-artifacts']);
    expect(preferences.pathsForScope('cleanup')).toEqual(['/fixture/excluded']);
    expect(wrapper.getComponent(MdStorageScanExclusionsDialog).props('modelValue')).toBe(false);
    wrapper.unmount();
  });

  it('keeps published storage results as snapshots after exclusion settings change', async () => {
    const preferences = useStorageScanPreferencesStore();
    preferences.initialized = true;
    vi.spyOn(PreferenceStorageService, 'saveScanExclusionPreferences').mockResolvedValue();
    const largeFiles = useLargeFilesStore();
    const duplicates = useDuplicateFilesStore();
    const analysis = useAnalysisStore();
    const largeResult = { scanId: 11 } as LargeFilesResult;
    const duplicateResult = { scanId: 12 } as DuplicateFilesResult;
    const analysisResult = { scanId: 13, root: '/fixture' } as AnalysisResult;
    largeFiles.result = largeResult;
    duplicates.result = duplicateResult;
    analysis.result = analysisResult;
    analysis.cache = { '/fixture': analysisResult };
    const wrapper = shallowMount(MdStorageScanExclusionsEditor);
    await wrapper.vm.open();

    wrapper
      .getComponent(MdStorageScanExclusionsDialog)
      .vm.$emit('save', [{ path: '/fixture/excluded', scopes: ['largeFiles', 'duplicateFiles', 'analysis'] }]);
    await flushPromises();

    expect(largeFiles.result).toEqual(largeResult);
    expect(duplicates.result).toEqual(duplicateResult);
    expect(analysis.result).toEqual(analysisResult);
    expect(analysis.cache['/fixture']).toEqual(analysisResult);
    expect(preferences.pathsForScope('analysis')).toEqual(['/fixture/excluded']);
    wrapper.unmount();
  });
});

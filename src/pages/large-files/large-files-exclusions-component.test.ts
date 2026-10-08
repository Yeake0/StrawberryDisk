// @vitest-environment happy-dom

import { flushPromises, shallowMount } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import MdScanExclusionLink from '@/components/custom/md-scan-exclusion-link.vue';
import { i18n } from '@/i18n';
import type { LargeFilesResult } from '@/lib/models/large-file';
import { useLargeFilesStore } from '@/stores/large-files-store';
import { useStorageScanPreferencesStore } from '@/stores/storage-scan-preferences-store';

import LargeFilesPage from './index.vue';

vi.mock('@tauri-apps/plugin-os', () => ({ platform: () => 'macos' }));

const result: LargeFilesResult = {
  scanId: 7,
  roots: ['/scan'],
  scannedAtMs: 1,
  scanMode: 'quick',
  minimumBytes: 50 * 1024 * 1024,
  totalBytes: 0,
  totalCount: 0,
  returnedCount: 0,
  truncated: false,
  skippedCount: 0,
  entries: [],
};

beforeEach(() => {
  setActivePinia(createPinia());
  useStorageScanPreferencesStore().initialized = true;
});

describe('large-file scan exclusions', () => {
  it('shows the scan snapshot exclusion link and opens its settings', async () => {
    const store = useLargeFilesStore();
    const wrapper = shallowMount(LargeFilesPage, {
      props: {
        disk: null,
        disks: [],
        result,
        progress: null,
        minimumBytes: result.minimumBytes,
        busy: false,
        cancelling: false,
        deleting: false,
      },
      global: {
        plugins: [i18n],
        stubs: {
          MdPageShell: { template: '<div><slot /></div>' },
          MdResultWorkspace: { template: '<div><slot name="summary" /><slot /></div>' },
          MdResultSummary: { template: '<div><slot name="status" /></div>' },
        },
      },
    });

    expect(wrapper.findComponent(MdScanExclusionLink).exists()).toBe(false);
    store.resultExcludedFolders = ['/other/private'];
    await flushPromises();
    expect(wrapper.findComponent(MdScanExclusionLink).exists()).toBe(false);

    store.resultExcludedFolders = ['/scan/private'];
    await flushPromises();

    wrapper.getComponent(MdScanExclusionLink).vm.$emit('open');
    expect(wrapper.emitted('openExclusions')).toHaveLength(1);

    await wrapper.setProps({ result: null });
    expect(wrapper.findComponent(MdScanExclusionLink).exists()).toBe(false);
    wrapper.unmount();
  });
});

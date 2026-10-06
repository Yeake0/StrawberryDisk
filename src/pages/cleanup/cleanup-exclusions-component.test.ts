// @vitest-environment happy-dom

import { flushPromises, shallowMount } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import MdScanExclusionLink from '@/components/custom/md-scan-exclusion-link.vue';
import MdPermissionGuidance from '@/components/custom/md-permission-guidance.vue';
import { i18n } from '@/i18n';
import type { PresentedCleanupScanResult } from '@/lib/models/cleanup';
import { LANGUAGE_IDS } from '@/lib/models/settings';
import { FileManagerService } from '@/lib/services/file-manager-service';
import { MACOS_PRIVACY_DESTINATION_IDS } from '@/lib/models/macos-permissions';
import { MacOsPermissionService } from '@/lib/services/macos-permission-service';
import { useCleanupStore } from '@/stores/cleanup-store';
import { useCustomCleanupStore } from '@/stores/custom-cleanup-store';
import { useAppStore } from '@/stores/app-store';
import { PreferenceStorageService } from '@/lib/services/preference-storage-service';
import { ThemeService } from '@/lib/services/theme-service';

import CleanupPage from './index.vue';
import MdIncompleteScanGuidance from './components/md-incomplete-scan-guidance.vue';

const { platformMock } = vi.hoisted(() => ({ platformMock: vi.fn(() => 'macos') }));
vi.mock('@tauri-apps/plugin-os', () => ({ platform: platformMock }));

const scan: PresentedCleanupScanResult = {
  schemaVersion: '1.10',
  customScanId: null,
  scannedAtMs: 1,
  disk: { name: 'Fixture', mountPoint: '/scan', totalBytes: 100, usedBytes: 50, availableBytes: 50 },
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

beforeEach(() => {
  platformMock.mockReturnValue('macos');
  i18n.global.locale.value = LANGUAGE_IDS.enUS;
  setActivePinia(createPinia());
  useCustomCleanupStore().initialized = true;
  vi.spyOn(PreferenceStorageService, 'saveSettings').mockResolvedValue();
  vi.spyOn(ThemeService, 'apply').mockImplementation(() => undefined);
});

afterEach(() => {
  i18n.global.locale.value = LANGUAGE_IDS.enUS;
  vi.restoreAllMocks();
});

async function mountPage(snapshot: PresentedCleanupScanResult) {
  // Resolve the page's async result component before the test environment is torn down.
  await import('./components/md-cleanup-rule-groups.vue');
  return shallowMount(CleanupPage, {
    props: {
      busy: false,
      disk: scan.disk,
      disks: [],
      leftovers: null,
      leftoverResult: null,
      scanningLeftovers: false,
      deletingLeftovers: false,
      loadingMessage: '',
      operation: 'idle',
      progress: null,
      result: null,
      scan: snapshot,
      scanScope: { mode: 'standard' },
      selectedBytes: 0,
      selectedRuleIds: [],
      sourceSelections: [],
      closingApplications: false,
      closeResult: null,
      privilegedScanRuleId: null,
    },
    global: {
      plugins: [i18n],
      stubs: {
        MdPageShell: { template: '<div><slot /></div>' },
        MdResultWorkspace: { template: '<div><slot name="summary" /><slot /></div>' },
        MdResultSummary: {
          template:
            '<div><div data-test="summary-status"><slot name="status" /></div><div data-test="summary-actions"><slot name="actions" /></div></div>',
        },
      },
    },
  });
}

describe('deep cleanup scan notices', () => {
  it('shows the scan snapshot exclusion link and opens its settings', async () => {
    const store = useCleanupStore();
    const wrapper = await mountPage(scan);

    expect(wrapper.findComponent(MdScanExclusionLink).exists()).toBe(false);
    store.scanExcludedFolders = ['/scan/private'];
    await flushPromises();

    expect(wrapper.get('[data-test="summary-status"]').findComponent(MdScanExclusionLink).exists()).toBe(true);
    expect(wrapper.get('[data-test="summary-actions"]').findComponent(MdScanExclusionLink).exists()).toBe(false);
    wrapper.getComponent(MdScanExclusionLink).vm.$emit('open');
    expect(wrapper.emitted('openExclusions')).toHaveLength(1);

    await wrapper.setProps({ scan: null });
    expect(wrapper.findComponent(MdScanExclusionLink).exists()).toBe(false);
    wrapper.unmount();
  });

  it.each(Object.values(LANGUAGE_IDS))(
    'offers permission checks for possible macOS privacy restrictions in %s',
    async locale => {
      i18n.global.locale.value = locale;
      const openSettings = vi.spyOn(MacOsPermissionService, 'openPrivacySettings').mockResolvedValue();
      const wrapper = await mountPage({ ...scan, warningCount: 1, accessLimited: true, readFailureCount: 1 });
      await flushPromises();

      const guidance = wrapper.getComponent(MdPermissionGuidance);
      expect(guidance.props('modelValue')).toBe(true);
      expect(guidance.props('summary')).toBe(i18n.global.t('fullDiskAccessGuidance.summary'));
      expect(guidance.props('title')).toBe(i18n.global.t('fullDiskAccessGuidance.title'));
      expect(guidance.props('instructions')).toBe(i18n.global.t('fullDiskAccessGuidance.instructions'));
      expect(guidance.props('openSettingsLabel')).toBe(i18n.global.t('fullDiskAccessGuidance.openSettings'));
      expect(wrapper.get('[data-test="summary-status"]').text()).toBe('');
      expect(wrapper.get('[data-test="summary-actions"]').findComponent(MdPermissionGuidance).exists()).toBe(true);
      expect(wrapper.text()).not.toContain(i18n.global.t('cleanup.permission.otherWarning'));
      await guidance.props('openSettings')();
      expect(openSettings).toHaveBeenCalledWith(MACOS_PRIVACY_DESTINATION_IDS.fullDiskAccess);

      expect(wrapper.findComponent(MdIncompleteScanGuidance).exists()).toBe(false);
      await wrapper.setProps({ scan: { ...scan, scannedAtMs: 2 } });
      expect(wrapper.findComponent(MdPermissionGuidance).exists()).toBe(false);
      wrapper.unmount();
      openSettings.mockRestore();
    }
  );
  it.each(['macos', 'windows', 'linux'])('offers generic read-failure details on %s', async platform => {
    platformMock.mockReturnValue(platform);
    const openLogs = vi.spyOn(FileManagerService, 'openApplicationLogs').mockResolvedValue();
    const details = [
      {
        path: '/fixture/blocked',
        stage: 'openDirectory' as const,
        reason: 'permissionDenied' as const,
        osError: 13,
        error: 'Permission denied (os error 13)',
        privacyRestrictionPossible: false,
      },
    ];
    const wrapper = await mountPage({
      ...scan,
      warningCount: 6,
      accessLimited: false,
      readFailureCount: 6,
      readFailureDetails: details,
    });
    expect(wrapper.findComponent(MdPermissionGuidance).exists()).toBe(false);
    const guidance = wrapper.getComponent(MdIncompleteScanGuidance);
    expect(guidance.props('failureCount')).toBe(6);
    expect(guidance.props('failureDetails')).toEqual(details);
    guidance.vm.$emit('update:modelValue', true);
    await flushPromises();
    expect(guidance.props('modelValue')).toBe(true);
    guidance.vm.$emit('openLogs');
    await flushPromises();
    expect(openLogs).toHaveBeenCalledOnce();
    guidance.vm.$emit('retry');
    await vi.waitFor(() => {
      expect(wrapper.emitted('scan')?.at(-1)).toEqual([{ mode: 'standard' }]);
    });
    await wrapper.setProps({ scan: { ...scan, scannedAtMs: 2 } });
    expect(wrapper.findComponent(MdIncompleteScanGuidance).exists()).toBe(false);
    wrapper.unmount();
  });

  it('switches from ordinary read failures to permission guidance when a later scan observes restricted access', async () => {
    const wrapper = await mountPage({ ...scan, readFailureCount: 6 });
    expect(wrapper.findComponent(MdPermissionGuidance).exists()).toBe(false);
    const incomplete = wrapper.getComponent(MdIncompleteScanGuidance);
    incomplete.vm.$emit('update:modelValue', true);
    await flushPromises();

    await wrapper.setProps({ scan: { ...scan, scannedAtMs: 2, accessLimited: true, readFailureCount: 1 } });
    expect(wrapper.findComponent(MdIncompleteScanGuidance).exists()).toBe(false);
    expect(wrapper.getComponent(MdPermissionGuidance).props('modelValue')).toBe(true);

    await wrapper.setProps({ scan: { ...scan, scannedAtMs: 3, readFailureCount: 6 } });
    expect(wrapper.findComponent(MdPermissionGuidance).exists()).toBe(false);
    expect(wrapper.getComponent(MdIncompleteScanGuidance).props('modelValue')).toBe(false);
    wrapper.unmount();
  });

  it('does not turn intentional safety skips into read-failure notices', async () => {
    const wrapper = await mountPage({ ...scan, warningCount: 3 });
    expect(wrapper.findComponent(MdIncompleteScanGuidance).exists()).toBe(false);
    expect(wrapper.findComponent(MdPermissionGuidance).exists()).toBe(false);
    wrapper.unmount();
  });

  it('saves the hide choice without closing the current details, then hides future read-failure notices', async () => {
    const snapshot = { ...scan, readFailureCount: 80, permissionDeniedReadFailureCount: 79 };
    const wrapper = await mountPage(snapshot);
    const guidance = wrapper.getComponent(MdIncompleteScanGuidance);
    guidance.vm.$emit('update:modelValue', true);
    await flushPromises();
    guidance.vm.$emit('update:hideReadFailureAlerts', true);
    await flushPromises();
    expect(useAppStore().settings.hideCleanupReadFailureAlerts).toBe(true);
    expect(PreferenceStorageService.saveSettings).toHaveBeenCalledWith(
      expect.objectContaining({ hideCleanupReadFailureAlerts: true })
    );
    expect(wrapper.getComponent(MdIncompleteScanGuidance).props('modelValue')).toBe(true);
    guidance.vm.$emit('update:modelValue', false);
    await flushPromises();
    expect(wrapper.findComponent(MdIncompleteScanGuidance).exists()).toBe(false);
    await wrapper.setProps({ scan: { ...snapshot, scannedAtMs: 2, accessLimited: true } });
    expect(wrapper.findComponent(MdPermissionGuidance).exists()).toBe(false);
    expect(wrapper.findComponent(MdIncompleteScanGuidance).exists()).toBe(false);
    useAppStore().settings.hideCleanupReadFailureAlerts = false;
    await flushPromises();
    expect(wrapper.getComponent(MdPermissionGuidance).props('modelValue')).toBe(true);
    wrapper.unmount();
  });

  it('hides mixed read failures without discarding their diagnostics', async () => {
    useAppStore().settings.hideCleanupReadFailureAlerts = true;
    const snapshot = {
      ...scan,
      accessLimited: true,
      readFailureCount: 80,
      permissionDeniedReadFailureCount: 79,
      readFailureDetails: Array.from({ length: 50 }, (_, index) => ({
        path: `/fixture/${index}`,
        stage: 'openDirectory' as const,
        reason: 'permissionDenied' as const,
        osError: 13,
        error: 'Permission denied',
        privacyRestrictionPossible: false,
      })),
    };
    const wrapper = await mountPage(snapshot);
    expect(wrapper.findComponent(MdPermissionGuidance).exists()).toBe(false);
    expect(wrapper.findComponent(MdIncompleteScanGuidance).exists()).toBe(false);
    expect(wrapper.props('scan')).toEqual(snapshot);
    useAppStore().settings.hideCleanupReadFailureAlerts = false;
    await flushPromises();
    expect(wrapper.getComponent(MdPermissionGuidance).props('modelValue')).toBe(true);
    wrapper.unmount();
  });

  it.each(['macos', 'windows', 'linux'])('hides older and non-permission read failures on %s', async platform => {
    platformMock.mockReturnValue(platform);
    useAppStore().settings.hideCleanupReadFailureAlerts = true;
    const wrapper = await mountPage({ ...scan, readFailureCount: 43 });
    expect(wrapper.findComponent(MdIncompleteScanGuidance).exists()).toBe(false);
    await wrapper.setProps({ scan: { ...scan, readFailureCount: 2, permissionDeniedReadFailureCount: 0 } });
    expect(wrapper.findComponent(MdIncompleteScanGuidance).exists()).toBe(false);
    useAppStore().settings.hideCleanupReadFailureAlerts = false;
    await flushPromises();
    expect(wrapper.getComponent(MdIncompleteScanGuidance).props('failureCount')).toBe(2);
    wrapper.unmount();
  });
});

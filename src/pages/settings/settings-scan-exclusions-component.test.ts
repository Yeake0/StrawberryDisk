// @vitest-environment happy-dom

import { flushPromises, shallowMount } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import MdSettingsRow from '@/components/custom/md-settings-row.vue';
import MdSwitch from '@/components/custom/md-switch.vue';
import { i18n } from '@/i18n';
import { SCAN_EXCLUSION_PREFERENCES_SCHEMA_VERSION, type ScanExclusionPreferences } from '@/lib/models/storage-scan';
import { MacOsPermissionService } from '@/lib/services/macos-permission-service';
import { OperatingSystemService } from '@/lib/services/operating-system-service';
import { PreferenceStorageService } from '@/lib/services/preference-storage-service';
import * as AppSettingsUtils from '@/lib/utils/app-settings';
import { useStorageScanPreferencesStore } from '@/stores/storage-scan-preferences-store';

import SettingsPage from './index.vue';

beforeEach(() => {
  setActivePinia(createPinia());
  vi.spyOn(MacOsPermissionService, 'isMacOs').mockReturnValue(false);
  vi.spyOn(OperatingSystemService, 'isLinux').mockReturnValue(false);
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe('scan exclusion settings', () => {
  it('lets users restore cleanup read-failure alerts from settings', async () => {
    vi.spyOn(PreferenceStorageService, 'loadScanExclusionPreferences').mockResolvedValue({
      schemaVersion: 3,
      folders: [],
      names: [],
    });
    const wrapper = shallowMount(SettingsPage, {
      props: { settings: { ...AppSettingsUtils.defaults(), hideCleanupReadFailureAlerts: true }, focusRevision: 0 },
      global: {
        plugins: [i18n],
        stubs: {
          MdPageShell: { template: '<div><slot /></div>' },
          MdSettingsGroup: { template: '<div><slot /></div>' },
          MdSettingsRow: { template: '<div><slot /></div>' },
        },
      },
    });
    const control = wrapper.getComponent(MdSwitch);
    expect(control.props('modelValue')).toBe(true);
    control.vm.$emit('update:modelValue', false);
    await flushPromises();
    expect(wrapper.emitted('save')?.at(-1)).toEqual([expect.objectContaining({ hideCleanupReadFailureAlerts: false })]);
    wrapper.unmount();
  });
  it('keeps the settings entry available while saved folders load', async () => {
    let finishLoad: (value: ScanExclusionPreferences) => void = () => undefined;
    vi.spyOn(PreferenceStorageService, 'loadScanExclusionPreferences').mockImplementation(
      () =>
        new Promise(resolve => {
          finishLoad = resolve;
        })
    );
    const wrapper = shallowMount(SettingsPage, {
      props: { settings: AppSettingsUtils.defaults(), focusRevision: 0 },
      global: {
        plugins: [i18n],
        stubs: {
          MdPageShell: { template: '<div><slot /></div>' },
          MdSettingsGroup: { template: '<div><slot /></div>' },
        },
      },
    });
    const editor = wrapper
      .findAllComponents(MdSettingsRow)
      .find(row => row.props('title') === i18n.global.t('settings.scanExclusionsTitle'));
    expect(editor).toBeDefined();
    await editor!.trigger('click');
    expect(wrapper.emitted('openScanExclusions')).toHaveLength(1);

    finishLoad({
      schemaVersion: SCAN_EXCLUSION_PREFERENCES_SCHEMA_VERSION,
      names: [],
      folders: [{ path: '/saved/folder', scopes: ['largeFiles'] }],
    });
    await flushPromises();

    expect(useStorageScanPreferencesStore().folders).toEqual([{ path: '/saved/folder', scopes: ['largeFiles'] }]);
    wrapper.unmount();
  });
});

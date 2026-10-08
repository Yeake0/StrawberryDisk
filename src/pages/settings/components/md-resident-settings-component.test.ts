import { createPinia } from 'pinia';
import type { ResidentPreferences } from '@/lib/models/resident';
import { preferencesFixture } from '@/tests/fixtures/resident';
// @vitest-environment happy-dom
import { flushPromises, mount } from '@vue/test-utils';
import { createI18n } from 'vue-i18n';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import AutostartSettings from './md-autostart-settings.vue';
import DisplaySettings from './md-status-display-settings.vue';
import { readingFixture } from '@/tests/fixtures/resident';
import { ResidentService } from '@/lib/services/resident-service';

vi.mock('@/lib/services/resident-service', () => ({
  ResidentService: {
    preferences: vi.fn(),
    catalogue: vi.fn(),
    onReading: vi.fn(),
    autostartEnabled: vi.fn(),
    setAutostart: vi.fn(),
    savePreferences: vi.fn(),
  },
}));

function render() {
  return mount(
    {
      components: { AutostartSettings, DisplaySettings },
      template: '<AutostartSettings /><DisplaySettings :is-mac-os="true" />',
    },
    {
      global: {
        plugins: [
          createPinia(),
          createI18n({ legacy: false, locale: 'en', messages: { en: {} }, missingWarn: false, fallbackWarn: false }),
        ],
      },
    }
  );
}

describe('resident settings', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(ResidentService.catalogue).mockResolvedValue(readingFixture());
    vi.mocked(ResidentService.onReading).mockResolvedValue(vi.fn());
    vi.mocked(ResidentService.autostartEnabled).mockResolvedValue(false);
    vi.mocked(ResidentService.setAutostart).mockResolvedValue();
    vi.mocked(ResidentService.preferences).mockResolvedValue(preferencesFixture());
    vi.mocked(ResidentService.savePreferences).mockImplementation(async value => ({
      ...value,
      revision: value.revision + 1,
    }));
  });

  it('loads native preferences and persists the switch before displaying its new state', async () => {
    const wrapper = render();
    await flushPromises();
    const toggle = wrapper.get('#resident-enabled');
    expect(toggle.attributes('aria-checked')).toBe('true');
    let finish!: (value: ResidentPreferences) => void;
    vi.mocked(ResidentService.savePreferences).mockReturnValueOnce(
      new Promise(resolve => {
        finish = resolve;
      })
    );
    await toggle.trigger('click');
    expect(toggle.attributes('disabled')).toBeDefined();
    expect(toggle.attributes('aria-checked')).toBe('true');
    finish({ ...preferencesFixture(), enabled: false, revision: 1 });
    await flushPromises();
    expect(toggle.attributes('aria-checked')).toBe('false');
    expect(wrapper.get('#resident-autostart').attributes('disabled')).toBeUndefined();
    expect(ResidentService.savePreferences).toHaveBeenCalledWith({
      ...preferencesFixture(),
      enabled: false,
    });
    wrapper.unmount();
  });

  it('keeps the saved state and shows an error on failed writes', async () => {
    const wrapper = render();
    await flushPromises();
    vi.mocked(ResidentService.savePreferences).mockRejectedValueOnce(new Error('storage'));
    await wrapper.get('#resident-enabled').trigger('click');
    await flushPromises();
    expect(wrapper.get('#resident-enabled').attributes('aria-checked')).toBe('true');
    expect(wrapper.get('.settings-feedback').text()).toContain('systemStatus.saveFailed');
    wrapper.unmount();
  });

  it('supports reload after a read failure', async () => {
    vi.mocked(ResidentService.preferences).mockRejectedValueOnce(new Error('storage'));
    const wrapper = render();
    await flushPromises();
    expect(wrapper.get('#resident-enabled').attributes('disabled')).toBeDefined();
    await wrapper.get('.settings-feedback button').trigger('click');
    await flushPromises();
    expect(wrapper.get('#resident-enabled').attributes('disabled')).toBeUndefined();
    wrapper.unmount();
  });

  it('preserves the OS login setting when residency is disabled', async () => {
    vi.mocked(ResidentService.autostartEnabled).mockResolvedValue(true);
    const wrapper = render();
    await flushPromises();
    expect(wrapper.get('#resident-autostart').attributes('aria-checked')).toBe('true');
    await wrapper.get('#resident-enabled').trigger('click');
    await flushPromises();
    expect(wrapper.get('#resident-autostart').attributes('aria-checked')).toBe('true');
    expect(wrapper.get('#resident-autostart').attributes('disabled')).toBeUndefined();
    expect(ResidentService.setAutostart).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it('enables login startup without enabling residency', async () => {
    vi.mocked(ResidentService.preferences).mockResolvedValue({ ...preferencesFixture(), enabled: false });
    const wrapper = render();
    await flushPromises();
    expect(wrapper.get('#resident-autostart').attributes('disabled')).toBeUndefined();
    vi.mocked(ResidentService.autostartEnabled).mockResolvedValue(true);
    await wrapper.get('#resident-autostart').trigger('click');
    await flushPromises();
    expect(ResidentService.setAutostart).toHaveBeenCalledWith(true);
    expect(wrapper.get('#resident-autostart').attributes('aria-checked')).toBe('true');
    expect(wrapper.get('#resident-enabled').attributes('aria-checked')).toBe('false');
    expect(ResidentService.savePreferences).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it('keeps the prior login setting on failure and accepts the OS state after success', async () => {
    const wrapper = render();
    await flushPromises();
    vi.mocked(ResidentService.setAutostart).mockRejectedValueOnce(new Error('OS error'));
    await wrapper.get('#resident-autostart').trigger('click');
    await flushPromises();
    expect(wrapper.get('#resident-autostart').attributes('aria-checked')).toBe('false');
    expect(wrapper.find('[role="alert"]').exists()).toBe(true);
    vi.mocked(ResidentService.autostartEnabled).mockResolvedValue(true);
    await wrapper.get('#resident-autostart').trigger('click');
    await flushPromises();
    expect(wrapper.get('#resident-autostart').attributes('aria-checked')).toBe('true');
    expect(ResidentService.setAutostart).toHaveBeenCalledWith(true);
    wrapper.unmount();
  });

  it('keeps the display switch with its options and login independent', async () => {
    const wrapper = render();
    await flushPromises();
    expect(wrapper.find('#resident-memory').exists()).toBe(false);
    expect(wrapper.findAll('[role="switch"]')).toHaveLength(2);
    expect(wrapper.find('.status-settings #resident-enabled').exists()).toBe(true);
    expect(wrapper.find('.autostart-settings #resident-autostart').exists()).toBe(true);
    expect(wrapper.find('.autostart-settings #resident-enabled').exists()).toBe(false);
    expect(wrapper.text()).not.toContain('monitoring.openPanel');
    wrapper.unmount();
  });
  it('keeps residency editable when the independent login read fails', async () => {
    vi.mocked(ResidentService.autostartEnabled).mockRejectedValueOnce(new Error('OS registration unavailable'));
    const wrapper = render();
    await flushPromises();
    expect(wrapper.get('#resident-enabled').attributes('aria-checked')).toBe('true');
    expect(wrapper.get('#resident-enabled').attributes('disabled')).toBeUndefined();
    expect(wrapper.get('#resident-autostart').attributes('disabled')).toBeDefined();
    await wrapper.get('#resident-enabled').trigger('click');
    expect(ResidentService.savePreferences).toHaveBeenCalled();
    wrapper.unmount();
  });
  it('keeps login editable when resident preferences fail to load', async () => {
    vi.mocked(ResidentService.preferences).mockRejectedValueOnce(new Error('store unavailable'));
    const wrapper = render();
    await flushPromises();
    expect(wrapper.get('#resident-autostart').attributes('disabled')).toBeUndefined();
    await wrapper.get('#resident-autostart').trigger('click');
    expect(ResidentService.setAutostart).toHaveBeenCalledWith(true);
    wrapper.unmount();
  });
});

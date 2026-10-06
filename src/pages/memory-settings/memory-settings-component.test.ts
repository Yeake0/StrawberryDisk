// @vitest-environment happy-dom
import { flushPromises, mount } from '@vue/test-utils';
import { createPinia } from 'pinia';
import { createI18n } from 'vue-i18n';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import Page from './index.vue';
import { Select } from '@/components/ui/select';
import { MemoryReleaseService } from '@/lib/services/memory-release-service';
import en from '@/locales/en-US.json';
import zh from '@/locales/zh-CN.json';
import tw from '@/locales/zh-TW.json';
import ja from '@/locales/ja-JP.json';
import ko from '@/locales/ko-KR.json';
const environment = vi.hoisted(() => ({ windows: true }));
vi.mock('@/lib/services/operating-system-service', () => ({
  OperatingSystemService: { isWindows: () => environment.windows },
}));
const prefs = {
  schemaVersion: 1 as const,
  revision: 0,
  automatic: false,
  intervalMinutes: 30,
  thresholdPercent: 80,
  skipForeground: true,
  exclusions: [],
};
vi.mock('@/lib/services/memory-release-service', () => ({
  MemoryReleaseService: {
    preferences: vi.fn(),
    save: vi.fn(),
    applications: vi.fn(),
    onPreferences: vi.fn(),
    onFocus: vi.fn(),
    showSettings: vi.fn(),
    closeSettings: vi.fn(),
  },
}));
vi.mock('@/lib/services/logger-service', () => ({ LoggerService: { warn: vi.fn() } }));
beforeEach(() => {
  vi.clearAllMocks();
  environment.windows = true;
  vi.mocked(MemoryReleaseService.preferences).mockResolvedValue(prefs);
  vi.mocked(MemoryReleaseService.onPreferences).mockResolvedValue(() => {});
  vi.mocked(MemoryReleaseService.onFocus).mockResolvedValue(() => {});
  vi.mocked(MemoryReleaseService.showSettings).mockResolvedValue(undefined);
  vi.mocked(MemoryReleaseService.save).mockImplementation(async p => ({ ...p, revision: p.revision + 1 }));
  vi.mocked(MemoryReleaseService.applications).mockResolvedValue([{ name: 'Code.exe', path: 'C:\\Apps\\Code.exe' }]);
});
function render(messages = en) {
  return mount(Page, {
    global: {
      plugins: [createPinia(), createI18n({ legacy: false, locale: 'test', messages: { test: messages } })],
      stubs: { MdIcon: true, MdNativeFileIcon: true },
    },
  });
}
describe('memory release settings window', () => {
  it('keeps automatic controls available on macOS without unsupported foreground trimming', async () => {
    environment.windows = false;
    vi.mocked(MemoryReleaseService.preferences).mockResolvedValue({ ...prefs, skipForeground: false });
    const page = render();
    await flushPromises();
    expect(page.find('#automatic-release').exists()).toBe(true);
    expect(page.find('.foreground').exists()).toBe(false);
    expect(page.find('.exclusion-section').exists()).toBe(false);
    expect(MemoryReleaseService.applications).not.toHaveBeenCalled();
    await page.get('#automatic-release').trigger('click');
    await page.get('footer .primary').trigger('click');
    await flushPromises();
    expect(MemoryReleaseService.save).toHaveBeenCalledWith(
      expect.objectContaining({ automatic: true, skipForeground: false })
    );
    page.unmount();
  });
  it.each([en, zh, tw, ja, ko])('renders localized labels with automatic release disabled', async messages => {
    const page = render(messages);
    await flushPromises();
    expect(page.get('h1').text()).toBe(messages.memoryRelease.title);
    expect(page.get('#release-interval').attributes('disabled')).toBeDefined();
    expect(MemoryReleaseService.showSettings).toHaveBeenCalledOnce();
    page.unmount();
  });
  it('adds a running app in the same window and saves only on explicit save', async () => {
    const page = render();
    await flushPromises();
    await page.get('.section-heading > .page-action').trigger('click');
    await flushPromises();
    await page.get('.candidate [role="checkbox"]').trigger('click');
    await page.get('footer .primary').trigger('click');
    expect(page.get('.excluded-row').text()).toContain('Code.exe');
    expect(MemoryReleaseService.save).not.toHaveBeenCalled();
    await page.get('#automatic-release').trigger('click');
    await page.get('footer .primary').trigger('click');
    await flushPromises();
    expect(MemoryReleaseService.save).toHaveBeenCalledWith(
      expect.objectContaining({ automatic: true, exclusions: [{ name: 'Code.exe', path: 'C:\\Apps\\Code.exe' }] })
    );
    expect(MemoryReleaseService.closeSettings).toHaveBeenCalledOnce();
    page.unmount();
  });
  it.each([3, 5])('saves a %i minute interval without changing exclusions', async minutes => {
    const page = render();
    await flushPromises();
    await page.get('#automatic-release').trigger('click');
    page.findAllComponents(Select)[0].vm.$emit('update:modelValue', minutes);
    await flushPromises();
    await page.get('footer .primary').trigger('click');
    await flushPromises();
    expect(MemoryReleaseService.save).toHaveBeenCalledWith(
      expect.objectContaining({ intervalMinutes: minutes, exclusions: [] })
    );
    page.unmount();
  });
  it('does not persist draft changes on cancel', async () => {
    const page = render();
    await flushPromises();
    await page.get('#automatic-release').trigger('click');
    await page.get('footer button:not(.primary)').trigger('click');
    expect(MemoryReleaseService.save).not.toHaveBeenCalled();
    expect(MemoryReleaseService.closeSettings).toHaveBeenCalledOnce();
    page.unmount();
  });
});

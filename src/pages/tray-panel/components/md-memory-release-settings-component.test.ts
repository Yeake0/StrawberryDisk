// @vitest-environment happy-dom
import { flushPromises, mount } from '@vue/test-utils';
import { createPinia } from 'pinia';
import { createI18n } from 'vue-i18n';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import Form from '@/components/memory-release/md-memory-release-settings.vue';
import { defineComponent, h } from 'vue';
import { useMemoryReleaseStore } from '@/stores/memory-release-store';
import { MemoryReleaseService } from '@/lib/services/memory-release-service';
import en from '@/locales/en-US.json';
import zh from '@/locales/zh-CN.json';
import tw from '@/locales/zh-TW.json';
import ja from '@/locales/ja-JP.json';
import ko from '@/locales/ko-KR.json';

vi.mock('@/lib/services/operating-system-service', () => ({
  OperatingSystemService: { isWindows: () => false },
}));
vi.mock('@/lib/services/memory-release-service', () => ({
  MemoryReleaseService: { preferences: vi.fn(), save: vi.fn(), onPreferences: vi.fn() },
}));
vi.mock('@/lib/services/logger-service', () => ({ LoggerService: { warn: vi.fn() } }));
const preferences = {
  schemaVersion: 1 as const,
  revision: 0,
  automatic: true,
  intervalMinutes: 15,
  thresholdPercent: 90,
  skipForeground: false,
  exclusions: [],
};
const pages: ReturnType<typeof mount>[] = [];
function render(messages = en) {
  const pinia = createPinia();
  const store = useMemoryReleaseStore(pinia);
  store.accept(preferences);
  const Harness = defineComponent({
    emits: ['close'],
    setup(_props, { emit }) {
      return () =>
        h(Form, {
          dialog: true,
          preferences: store.preferences,
          saving: store.saving,
          failed: store.failed,
          reloadPreferences: store.load,
          savePreferences: store.save,
          onClose: () => emit('close'),
        });
    },
  });
  const page = mount(Harness, {
    global: {
      plugins: [pinia, createI18n({ legacy: false, locale: 'en', messages: { en: messages } })],
      stubs: {
        MdIcon: true,
        MdDialogHeader: { template: '<header><slot /></header>' },
        DialogTitle: { template: '<h2><slot /></h2>' },
      },
    },
  });
  pages.push(page);
  return { page, store };
}
beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(MemoryReleaseService.preferences).mockResolvedValue(preferences);
  vi.mocked(MemoryReleaseService.save).mockImplementation(async value => ({ ...value, revision: value.revision + 1 }));
});
afterEach(() => pages.splice(0).forEach(page => page.unmount()));
describe('inline memory release settings', () => {
  it('does not silently replace edits made while a reload is pending', async () => {
    const { page, store } = render();
    store.failed = true;
    await flushPromises();
    let finish!: (value: typeof preferences) => void;
    vi.mocked(MemoryReleaseService.preferences).mockReturnValueOnce(
      new Promise(resolve => {
        finish = resolve;
      })
    );
    await page.get('.notice button').trigger('click');
    expect(page.get('#automatic-release').attributes('disabled')).toBeDefined();
    expect(page.get('.settings-footer .primary').attributes('disabled')).toBeDefined();
    await page.get('#automatic-release').trigger('click');
    finish(preferences);
    await flushPromises();
    await page.get('.settings-footer .primary').trigger('click');
    await flushPromises();
    // The disabled control must not produce an unnoticed edit that gets lost.
    expect(vi.mocked(MemoryReleaseService.save).mock.calls[0]![0].automatic).toBe(true);
  });
  it.each([en, zh, tw, ja, ko])(
    'renders cached settings immediately without another read or subscription',
    async messages => {
      const { page, store } = render(messages);
      expect(page.get('#automatic-release').attributes('aria-checked')).toBe('true');
      expect(page.get('.settings-footer .primary').text()).toBe(messages.memoryRelease.save);
      expect(page.find('.settings-footer span').exists()).toBe(false);
      expect(page.text()).not.toContain(messages.memoryRelease.loading);
      await flushPromises();
      expect(MemoryReleaseService.preferences).not.toHaveBeenCalled();
      expect(MemoryReleaseService.onPreferences).not.toHaveBeenCalled();
      await page.get('#automatic-release').trigger('click');
      await page.get('.settings-footer button:not(.primary)').trigger('click');
      expect(page.emitted('close')).toHaveLength(1);
      expect(store.preferences?.automatic).toBe(true);
      expect(MemoryReleaseService.save).not.toHaveBeenCalled();
    }
  );

  it('keeps the draft after a failed reload so retrying save retains the intended change', async () => {
    const { page, store } = render();
    await page.get('#automatic-release').trigger('click');
    store.failed = true;
    await flushPromises();
    vi.mocked(MemoryReleaseService.preferences).mockRejectedValueOnce(new Error('unavailable'));
    await page.get('.notice button').trigger('click');
    await flushPromises();
    expect(page.get('#automatic-release').attributes('aria-checked')).toBe('false');
    expect(page.get('.notice').text()).toContain(en.memoryRelease.failed);
    await page.get('.settings-footer .primary').trigger('click');
    await flushPromises();
    expect(MemoryReleaseService.save).toHaveBeenCalledWith(expect.objectContaining({ automatic: false }));
    expect(page.emitted('close')).toHaveLength(1);
  });

  it('does not dismiss a pending or failed save and allows retrying the same draft', async () => {
    const { page } = render();
    await page.get('#automatic-release').trigger('click');
    let fail!: (reason: Error) => void;
    vi.mocked(MemoryReleaseService.save).mockReturnValueOnce(
      new Promise((_resolve, reject) => {
        fail = reject;
      })
    );
    await page.get('.settings-footer .primary').trigger('click');
    expect(page.get('#automatic-release').attributes('disabled')).toBeDefined();
    await page.get('.settings-footer button:not(.primary)').trigger('click');
    expect(page.emitted('close')).toBeUndefined();
    fail(new Error('write failed'));
    await flushPromises();
    expect(page.emitted('close')).toBeUndefined();
    expect(page.get('#automatic-release').attributes('aria-checked')).toBe('false');
    await page.get('.settings-footer .primary').trigger('click');
    await flushPromises();
    expect(MemoryReleaseService.save).toHaveBeenLastCalledWith(expect.objectContaining({ automatic: false }));
    expect(page.emitted('close')).toHaveLength(1);
  });

  it('requires an explicit reload after another window changes the revision', async () => {
    const { page, store } = render();
    await page.get('#automatic-release').trigger('click');
    const newer = { ...preferences, revision: 1, intervalMinutes: 30 };
    store.accept(newer);
    await flushPromises();
    expect(page.get('.notice').text()).toContain(en.memoryRelease.conflict);
    await page.get('.settings-footer .primary').trigger('click');
    expect(MemoryReleaseService.save).not.toHaveBeenCalled();
    vi.mocked(MemoryReleaseService.preferences).mockResolvedValueOnce(newer);
    await page.get('.notice button').trigger('click');
    await flushPromises();
    expect(page.find('.notice').exists()).toBe(false);
    await page.get('.settings-footer .primary').trigger('click');
    await flushPromises();
    expect(MemoryReleaseService.save).toHaveBeenCalledWith(
      expect.objectContaining({ revision: 1, intervalMinutes: 30 })
    );
  });
});

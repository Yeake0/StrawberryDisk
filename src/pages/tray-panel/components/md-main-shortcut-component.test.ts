// @vitest-environment happy-dom
import { flushPromises, mount } from '@vue/test-utils';
import { createI18n } from 'vue-i18n';
import { beforeEach, expect, it, vi } from 'vitest';
import type { AppUpdateNotice } from '@/lib/models/app-update';
import Notice from './md-main-shortcut.vue';
import { BackgroundUpdateService } from '@/lib/services/background-update-service';
import { ResidentService } from '@/lib/services/resident-service';
import en from '@/locales/en-US.json';
import zh from '@/locales/zh-CN.json';
import tw from '@/locales/zh-TW.json';
import ja from '@/locales/ja-JP.json';
import ko from '@/locales/ko-KR.json';
vi.mock('@/lib/services/background-update-service', () => ({ BackgroundUpdateService: { watch: vi.fn() } }));
vi.mock('@/lib/services/resident-service', () => ({ ResidentService: { openMain: vi.fn() } }));
vi.mock('@/lib/services/logger-service', () => ({ LoggerService: { warn: vi.fn(), info: vi.fn() } }));
let publish: (notice: AppUpdateNotice) => void;
const stop = vi.fn();
beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(BackgroundUpdateService.watch).mockImplementation(async handler => {
    publish = handler;
    return stop;
  });
  vi.mocked(ResidentService.openMain).mockResolvedValue(undefined);
});
it.each([en, zh, tw, ja, ko])(
  'keeps the main shortcut label and switches its badge and destination with update availability',
  async messages => {
    const wrapper = mount(Notice, {
      global: {
        plugins: [createI18n({ legacy: false, locale: 'test', messages: { test: messages } })],
        stubs: { MdIcon: true },
      },
    });
    await flushPromises();
    expect(wrapper.get('button').text()).toBe(messages.monitoring.openMain);
    expect(wrapper.find('.update-dot').exists()).toBe(false);
    await wrapper.get('button').trigger('click');
    await flushPromises();
    expect(ResidentService.openMain).toHaveBeenLastCalledWith('main');
    publish({ schemaVersion: 1, checked: true, revision: 1, version: '1.2.0' });
    await flushPromises();
    expect(wrapper.get('button').text()).toBe(messages.monitoring.openMain);
    expect(wrapper.get('button').attributes('aria-label')).toBe(messages.updates.noticeAvailable);
    expect(wrapper.find('.update-dot').exists()).toBe(true);
    await wrapper.get('button').trigger('click');
    await flushPromises();
    expect(ResidentService.openMain).toHaveBeenCalledWith('about');
    publish({ schemaVersion: 1, checked: true, revision: 2, version: null });
    await flushPromises();
    expect(wrapper.find('.update-dot').exists()).toBe(false);
    expect(wrapper.get('button').text()).toBe(messages.monitoring.openMain);
    wrapper.unmount();
    expect(stop).toHaveBeenCalledOnce();
  }
);

it('prevents duplicate navigation and keeps the update badge available after failure', async () => {
  const wrapper = mount(Notice, {
    global: {
      plugins: [createI18n({ legacy: false, locale: 'en', messages: { en } })],
    },
  });
  await flushPromises();
  publish({ schemaVersion: 1, checked: true, revision: 1, version: '1.2.0' });
  await flushPromises();
  let reject!: (error: Error) => void;
  vi.mocked(ResidentService.openMain).mockReturnValueOnce(
    new Promise((_resolve, fail) => {
      reject = fail;
    })
  );
  await wrapper.get('button').trigger('click');
  expect(wrapper.get('button').attributes('disabled')).toBeDefined();
  await wrapper.get('button').trigger('click');
  expect(ResidentService.openMain).toHaveBeenCalledOnce();
  reject(new Error('navigation unavailable'));
  await flushPromises();
  expect(wrapper.emitted('error')).toHaveLength(1);
  expect(wrapper.find('.update-dot').exists()).toBe(true);
  expect(wrapper.get('button').attributes('aria-label')).toBe(en.updates.noticeRetry);
  expect(wrapper.get('button').text()).toBe(en.monitoring.openMain);
  await wrapper.get('button').trigger('click');
  await flushPromises();
  expect(ResidentService.openMain).toHaveBeenLastCalledWith('about');
  wrapper.unmount();
  expect(stop).toHaveBeenCalledOnce();
});

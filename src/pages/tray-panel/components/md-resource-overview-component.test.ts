// @vitest-environment happy-dom
import { mount } from '@vue/test-utils';
import { createI18n } from 'vue-i18n';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import Overview from './md-resource-overview.vue';
import { emptyReadings } from '@/lib/utils/system-resources';
import en from '@/locales/en-US.json';
import zh from '@/locales/zh-CN.json';
import tw from '@/locales/zh-TW.json';
import ja from '@/locales/ja-JP.json';
import ko from '@/locales/ko-KR.json';
import MdTooltip from '@/components/custom/md-tooltip.vue';
import { OperatingSystemService } from '@/lib/services/operating-system-service';
vi.mock('@/lib/services/byte-size-service', () => ({
  ByteSizeService: {
    bytes: (value: number) => `${value} B`,
    diskCapacity: (value: number) => `${value} B`,
    memory: (value: number) => `${value} B`,
  },
}));

describe('resource details', () => {
  beforeEach(() => vi.spyOn(OperatingSystemService, 'currentPlatform').mockReturnValue('macos'));
  afterEach(() => vi.restoreAllMocks());

  it.each(['macos', 'linux', 'windows'] as const)('limits the reclaimable-space hint to macOS on %s', platform => {
    vi.spyOn(OperatingSystemService, 'currentPlatform').mockReturnValue(platform);
    const reading = emptyReadings();
    reading.disk = {
      status: 'ready',
      sampledAtMs: 0,
      value: {
        volume: { id: 'system', name: '/', system: true },
        totalBytes: 100,
        availableBytes: 75,
        usedBytes: 25,
        usedPercent: 25,
      },
    };
    const wrapper = mount(Overview, {
      props: { metric: 'disk', reading },
      global: { plugins: [createI18n({ legacy: false, locale: 'en', messages: { en } })] },
    });
    expect(wrapper.get('.resource-meta').text()).toContain('75 B / 100 B');
    expect(wrapper.getComponent(MdTooltip).props('text')).toBe(
      platform === 'macos' ? en.systemStatus.diskCapacityHint : null
    );
    wrapper.unmount();
  });

  it.each(['cpu', 'memory'] as const)('opens %s details from the whole card only when interactive', async metric => {
    const wrapper = mount(Overview, {
      props: { metric, reading: emptyReadings(), interactive: true },
      global: { plugins: [createI18n({ legacy: false, locale: 'en', messages: { en } })] },
    });
    await wrapper.get('.card-navigation').trigger('click');
    expect(wrapper.emitted(metric)).toHaveLength(1);
    expect(wrapper.emitted(metric === 'cpu' ? 'memory' : 'cpu')).toBeUndefined();
    await wrapper.setProps({ interactive: false });
    expect(wrapper.find('.card-navigation').exists()).toBe(false);
    wrapper.unmount();
  });

  it('leaves the disconnected interval blank instead of moving old samples to now', () => {
    const reading = emptyReadings();
    reading.observedAtMs = 90000;
    reading.cpu.status = 'stale';
    reading.cpuHistory = [{ sampledAtMs: 60000, primary: 40, secondary: null }];
    const wrapper = mount(Overview, {
      props: { metric: 'cpu', reading },
      global: { plugins: [createI18n({ legacy: false, locale: 'en', messages: { en } })] },
    });
    expect(wrapper.get('.resource-value').text()).toBe('—');
    expect(wrapper.get('path[stroke]').attributes('d')).toMatch(/^M50,/);

    wrapper.unmount();
  });

  it.each([en, zh, tw, ja, ko])(
    'localizes the disk action and emits navigation without performing cleanup',
    async messages => {
      const reading = emptyReadings();
      reading.disk = {
        status: 'ready',
        sampledAtMs: 0,
        value: {
          volume: { id: 'test', name: 'Test', system: false },
          totalBytes: 100,
          availableBytes: 75,
          usedBytes: 25,
          usedPercent: 25,
        },
      };
      const wrapper = mount(Overview, {
        props: { metric: 'disk', reading },
        global: { plugins: [createI18n({ legacy: false, locale: 'test', messages: { test: messages } })] },
      });
      expect(wrapper.get('.cleanup-link').text()).toContain(messages.navigation.cleanup);
      expect(wrapper.get('.resource-meta').text()).toContain('75 B / 100 B');
      expect(wrapper.get('[role="meter"]').attributes('aria-valuenow')).toBe('25');
      await wrapper.get('.cleanup-link').trigger('click');
      expect(wrapper.emitted('cleanup')).toHaveLength(1);
      await wrapper.setProps({ reading: { ...reading, disk: { ...reading.disk, status: 'disconnected' } } });
      expect(wrapper.get('.resource-value').text()).toBe('—');
      expect(wrapper.get('.resource-source').text()).toBe('Test');
      expect(wrapper.find('[role="meter"]').exists()).toBe(false);
      wrapper.unmount();
    }
  );
});

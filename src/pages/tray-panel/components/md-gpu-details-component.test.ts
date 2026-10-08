// @vitest-environment happy-dom
import { mount, flushPromises } from '@vue/test-utils';
import { createPinia } from 'pinia';
import { createI18n } from 'vue-i18n';
import { beforeEach, afterEach, describe, it, expect, vi } from 'vitest';
import MdGpuDetails from './md-gpu-details.vue';
import { useResidentSettingsStore } from '@/stores/resident-settings-store';
import { emptyReadings } from '@/lib/utils/system-resources';
import { preferencesFixture } from '@/tests/fixtures/resident';
import type { ResourceReadings } from '@/lib/models/system-resources';
import { ResidentService } from '@/lib/services/resident-service';
import enUS from '@/locales/en-US.json';
import zhCN from '@/locales/zh-CN.json';
import zhTW from '@/locales/zh-TW.json';
import jaJP from '@/locales/ja-JP.json';
import koKR from '@/locales/ko-KR.json';
import ptBR from '@/locales/pt-BR.json';
import ruRU from '@/locales/ru-RU.json';
import trTR from '@/locales/tr-TR.json';
vi.mock('@/lib/services/resident-service', () => ({
  ResidentService: { preferences: vi.fn(), savePreferences: vi.fn() },
}));
vi.mock('@/lib/services/logger-service', () => ({ LoggerService: { warn: vi.fn() } }));
const messages = {
  'en-US': enUS,
  'zh-CN': zhCN,
  'zh-TW': zhTW,
  'ja-JP': jaJP,
  'ko-KR': koKR,
  'pt-BR': ptBR,
  'ru-RU': ruRU,
  'tr-TR': trTR,
};
function reading(): ResourceReadings {
  return {
    ...emptyReadings(),
    observedAtMs: 2000,
    gpuAdapters: [
      { id: 'a', name: 'GPU 0 · RTX 4090' },
      { id: 'b', name: 'GPU 1 · RTX 4090' },
    ],
    gpuDetailAdapterId: 'a',
    gpuDetailHistory: [{ sampledAtMs: 2000, primary: 3, secondary: null }],
    gpuDetails: {
      status: 'ready',
      sampledAtMs: 2000,
      value: {
        adapterId: 'a',
        adapterName: 'RTX 4090',
        usedPercent: 3,
        details: {
          activities: [
            { id: '0', kind: 'graphics', name: '3D', usedPercent: 3, includedInSummary: true },
            { id: '12', kind: 'other', name: 'Graphics_1', usedPercent: 56, includedInSummary: false },
          ],
          telemetry: {
            coreCount: null,
            temperatureCelsius: null,
            coreClockMhz: null,
            memoryClockMhz: null,
            fanPercent: null,
          },
          memoryArchitecture: 'dedicated',
          memoryStatus: 'ready',
          memory: {
            dedicatedUsedBytes: 1024 ** 3,
            dedicatedTotalBytes: 24 * 1024 ** 3,
            sharedUsedBytes: 512 * 1024 ** 2,
          },
        },
      },
    },
  };
}
const wrappers: ReturnType<typeof mount>[] = [];
function render(value = reading(), locale = 'en-US') {
  const pinia = createPinia();
  const wrapper = mount(MdGpuDetails, {
    props: { reading: value, active: false },
    attachTo: document.body,
    global: { plugins: [pinia, createI18n({ legacy: false, locale, messages })], stubs: { MdResourceTrend: true } },
  });
  wrappers.push(wrapper);
  return { wrapper, settings: useResidentSettingsStore(pinia) };
}
beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(ResidentService.preferences).mockResolvedValue(preferencesFixture());
});
afterEach(() => {
  wrappers.forEach(wrapper => wrapper.unmount());
  wrappers.length = 0;
  document.body.innerHTML = '';
});
describe('GPU detail capabilities and selection', () => {
  it('keeps custom engine activity separate from the summary', async () => {
    const { wrapper } = render();
    await flushPromises();
    expect(wrapper.get('header strong').text()).toBe('3%');
    expect(wrapper.get('.custom-activities').text()).toContain('Graphics_1');
    expect(wrapper.get('.custom-activities').text()).toContain('56%');
    expect(wrapper.get('.custom-activities').attributes('open')).toBeUndefined();
    expect(wrapper.findAll('.memory-row')).toHaveLength(1);
    expect(wrapper.text()).not.toContain('Shared memory');
    expect(wrapper.text()).not.toContain(enUS.gpuDetails.customHint);
  });
  it('renders stable functional and natural-name orders as activity changes', async () => {
    const value = reading();
    const activities = value.gpuDetails.value!.details!.activities;
    activities.push(
      { id: 'node:10', kind: 'videoEncode', name: null, usedPercent: 0, includedInSummary: true },
      { id: 'node:4', kind: 'copy', name: null, usedPercent: 0, includedInSummary: true },
      { id: 'node:3', kind: 'videoDecode', name: null, usedPercent: 0, includedInSummary: true },
      { id: 'node:14', kind: 'other', name: 'Compute_10', usedPercent: 0, includedInSummary: false },
      { id: 'node:1', kind: 'other', name: 'Compute_2', usedPercent: 0, includedInSummary: false }
    );
    const { wrapper } = render(value);
    await flushPromises();
    const labels = (selector: string) => wrapper.findAll(selector).map(row => row.get('div > span').text());
    const standard = ['3D', enUS.gpuDetails.copy, enUS.gpuDetails.videoDecode, enUS.gpuDetails.videoEncode];
    const custom = ['Compute_2', 'Compute_10', 'Graphics_1'];
    expect(labels('.activity-section .activity-row')).toEqual(standard);
    expect(labels('.custom-activities .activity-row')).toEqual(custom);
    const next = structuredClone(value);
    next.gpuDetails.value!.details!.activities.reverse().forEach((activity, index) => {
      activity.usedPercent = index * 10;
    });
    await wrapper.setProps({ reading: next });
    expect(labels('.activity-section .activity-row')).toEqual(standard);
    expect(labels('.custom-activities .activity-row')).toEqual(custom);
  });
  it('omits unsupported unified memory and displays actual core count', async () => {
    const value = reading();
    value.gpuDetails.value!.details = {
      activities: [{ id: 'renderer', kind: 'renderer', name: null, usedPercent: 12, includedInSummary: false }],
      telemetry: {
        coreCount: 40,
        temperatureCelsius: null,
        coreClockMhz: null,
        memoryClockMhz: null,
        fanPercent: null,
      },
      memoryArchitecture: 'unified',
      memoryStatus: 'unsupported',
      memory: null,
    };
    const { wrapper } = render(value);
    await flushPromises();
    expect(wrapper.text()).toContain('Renderer');
    expect(wrapper.text()).not.toContain('unified memory');
    expect(wrapper.find('.memory-section').exists()).toBe(false);
    expect(wrapper.get('.gpu-facts').text()).toContain('40');
    expect(wrapper.findAll('.memory-row')).toHaveLength(0);
    expect(wrapper.find('.custom-activities').exists()).toBe(false);
  });
  it('hides old device measurements immediately while a new selection is pending', async () => {
    const { wrapper, settings } = render();
    await flushPromises();
    settings.draft!.gpuAdapter = 'b';
    await flushPromises();
    expect(wrapper.get('header strong').text()).toBe('—');
    expect(wrapper.findAll('.activity-row')).toHaveLength(0);
    expect(wrapper.findAll('.memory-row')).toHaveLength(0);
  });
  it('reloads a selection changed in the main window when the panel reopens', async () => {
    const { wrapper, settings } = render();
    await flushPromises();
    const preferences = preferencesFixture();
    preferences.gpuAdapter = 'b';
    vi.mocked(ResidentService.preferences).mockResolvedValue(preferences);
    await wrapper.setProps({ active: true });
    await flushPromises();
    expect(settings.draft!.gpuAdapter).toBe('b');
    expect(wrapper.get('.device-trigger').attributes('aria-label')).toContain('GPU 1');
    expect(wrapper.get('header strong').text()).toBe('—');
    expect(wrapper.findAll('.activity-row')).toHaveLength(0);
  });
  it('keeps cached measurements visible while reopened preferences refresh', async () => {
    const { wrapper } = render();
    await flushPromises();
    let resolve!: (value: ReturnType<typeof preferencesFixture>) => void;
    vi.mocked(ResidentService.preferences).mockReturnValue(
      new Promise(done => {
        resolve = done;
      })
    );
    await wrapper.setProps({ active: true });
    expect(wrapper.get('header strong').text()).toBe('3%');
    expect(wrapper.findAll('.activity-row').length).toBeGreaterThan(0);
    resolve(preferencesFixture());
    await flushPromises();
    expect(wrapper.get('header strong').text()).toBe('3%');
  });
  it('marks retained stale data and keeps memory failures independent', async () => {
    const value = reading();
    value.gpuDetails.value!.details!.memoryStatus = 'failed';
    const { wrapper } = render(value);
    await flushPromises();
    expect(wrapper.findAll('.memory-row')).toHaveLength(0);
    expect(wrapper.get('header strong').text()).toBe('3%');
    value.gpuDetails.status = 'stale';
    await wrapper.setProps({ reading: { ...value } });
    expect(wrapper.get('header strong').text()).toBe('3%');
    expect(wrapper.get('.cache-notice').attributes('aria-label')).toContain('last available data');
    expect(wrapper.get('.cache-notice').attributes('aria-label')).toContain(enUS.systemStatus.stale);
    expect(wrapper.findAll('.activity-row').length).toBeGreaterThan(0);
  });
  it('renders a native cached sample before the initial preferences request completes', async () => {
    let resolve!: (value: ReturnType<typeof preferencesFixture>) => void;
    vi.mocked(ResidentService.preferences).mockReturnValue(
      new Promise(done => {
        resolve = done;
      })
    );
    const { wrapper } = render();
    expect(wrapper.get('header strong').text()).toBe('3%');
    expect(wrapper.findAll('.memory-row')).toHaveLength(1);
    const preferences = preferencesFixture();
    preferences.gpuAdapter = 'b';
    resolve(preferences);
    await flushPromises();
    expect(wrapper.get('header strong').text()).toBe('—');
    expect(wrapper.findAll('.memory-row')).toHaveLength(0);
  });
  it('uses the warm summary while detailed collection starts for the first time', async () => {
    const value = reading();
    value.gpu = { ...value.gpuDetails, value: { ...value.gpuDetails.value!, usedPercent: 7, details: null } };
    value.gpuDetails = { status: 'loading', sampledAtMs: null, value: null };
    value.gpuDetailHistory = [];
    const { wrapper } = render(value);
    await flushPromises();
    expect(wrapper.get('header strong').text()).toBe('7%');
    expect(wrapper.get('.device-name').text()).toBe('RTX 4090');
    expect(wrapper.findAll('.memory-row')).toHaveLength(0);
  });
  it('refreshes the total without discarding cached details of the same adapter', async () => {
    const value = reading();
    value.gpu = { ...value.gpuDetails, value: { ...value.gpuDetails.value!, usedPercent: 8, details: null } };
    value.gpuDetails.status = 'stale';
    const { wrapper } = render(value);
    await flushPromises();
    expect(wrapper.get('header strong').text()).toBe('8%');
    expect(wrapper.findAll('.memory-row')).toHaveLength(1);
    expect(wrapper.find('.cache-notice').exists()).toBe(true);
    value.gpuDetails = { ...value.gpuDetails, status: 'ready' };
    await wrapper.setProps({ reading: { ...value } });
    expect(wrapper.find('.cache-notice').exists()).toBe(false);
  });
  it('does not attach a previous automatic adapter to a new summary', async () => {
    const value = reading();
    value.gpu = {
      ...value.gpuDetails,
      value: { adapterId: 'b', adapterName: 'GPU B', usedPercent: 10, details: null },
    };
    const { wrapper } = render(value);
    await flushPromises();
    expect(wrapper.get('header strong').text()).toBe('10%');
    expect(wrapper.findAll('.memory-row')).toHaveLength(0);
    expect(wrapper.findAll('.activity-row')).toHaveLength(0);
    expect(wrapper.getComponent({ name: 'MdResourceTrend' }).props('history')).toEqual([]);
  });
  it('shows the same-device background curve before the first detailed sample', async () => {
    const value = reading();
    value.gpu = { ...value.gpuDetails, value: { ...value.gpuDetails.value!, details: null } };
    value.gpuDetails = { status: 'loading', sampledAtMs: null, value: null };
    const { wrapper } = render(value);
    await flushPromises();
    expect(wrapper.getComponent({ name: 'MdResourceTrend' }).props('history')).toEqual(value.gpuDetailHistory);
    value.gpuDetailAdapterId = 'b';
    await wrapper.setProps({ reading: { ...value } });
    expect(wrapper.getComponent({ name: 'MdResourceTrend' }).props('history')).toEqual([]);
  });
  it('shows renderer histories only for their actual device and omits removed copy', async () => {
    const value = reading();
    value.gpuDetails.value!.details!.activities = [
      { id: 'renderer', kind: 'renderer', name: null, usedPercent: 12, includedInSummary: false },
    ];
    value.gpuRendererHistory = [{ sampledAtMs: 2000, primary: 12, secondary: null }];
    const { wrapper } = render(value);
    await flushPromises();
    expect(wrapper.findComponent({ name: 'MdGpuEngineHistory' }).exists()).toBe(true);
    expect(wrapper.text()).not.toContain('reported by the system');
    value.gpuDetailAdapterId = 'b';
    await wrapper.setProps({ reading: { ...value } });
    expect(wrapper.findComponent({ name: 'MdGpuEngineHistory' }).exists()).toBe(false);
  });
  it('uses the fixed device label once in its tooltip', async () => {
    const preferences = preferencesFixture();
    preferences.gpuAdapter = 'a';
    vi.mocked(ResidentService.preferences).mockResolvedValue(preferences);
    const { wrapper } = render();
    await flushPromises();
    const tooltip = wrapper
      .findAllComponents({ name: 'MdTooltip' })
      .find(component => String(component.props('text')).includes('RTX 4090'))!;
    expect(tooltip.props('text')).toBe('GPU 0 · RTX 4090');
    expect(tooltip.find('.device-name').exists()).toBe(true);
    expect(tooltip.find('.device-caption').exists()).toBe(false);
  });
  for (const status of ['disconnected', 'unsupported'] as const) {
    it(`hides retained measurements when the GPU becomes ${status}`, async () => {
      const value = reading();
      value.gpuDetails.status = status;
      const { wrapper } = render(value);
      await flushPromises();
      expect(wrapper.get('header strong').text()).toBe('—');
      expect(wrapper.findAll('.memory-row')).toHaveLength(0);
    });
  }
  it('preserves disconnected selections and rolls back a rejected save', async () => {
    const preferences = preferencesFixture();
    preferences.gpuAdapter = 'missing';
    vi.mocked(ResidentService.preferences).mockResolvedValue(preferences);
    const { wrapper, settings } = render();
    await flushPromises();
    expect(wrapper.get('.device-trigger').attributes('aria-label')).toContain('disconnected');
    expect(wrapper.get('header strong').text()).toBe('—');
    vi.mocked(ResidentService.savePreferences).mockRejectedValue(new Error('conflict'));
    await settings.change({ gpuAdapter: 'b' });
    await flushPromises();
    expect(settings.draft!.gpuAdapter).toBe('missing');
    expect(wrapper.find('[role=alert]').exists()).toBe(true);
  });
  for (const locale of Object.keys(messages)) {
    it(`renders localized GPU details in ${locale}`, async () => {
      const { wrapper } = render(reading(), locale);
      await flushPromises();
      expect(wrapper.text()).not.toContain('gpuDetails.');
      expect(wrapper.findAll('h3')).toHaveLength(2);
    });
  }
});

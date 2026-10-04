// @vitest-environment happy-dom

import { flushPromises, mount } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
vi.mock('@tauri-apps/plugin-os', () => ({ platform: () => 'macos' }));
import { i18n } from '@/i18n';
import { Select } from '@/components/ui/select';
import type { DirectoryEntryInfo } from '@/lib/models/analysis';
import MdAnalysisDetailsTable from './md-analysis-details-table.vue';
import MdAnalysisVisualPane from './md-analysis-visual-pane.vue';
import MdAnalysisSunburst from './md-analysis-sunburst.vue';

const entries: DirectoryEntryInfo[] = Array.from({ length: 5000 }, (_, index) => ({
  name: `file-${String(index).padStart(4, '0')}.gguf`,
  path: `/files/${index}.gguf`,
  bytes: 5000 - index,
  fileCount: 1,
  isDirectory: false,
  modifiedAtMs: null,
  contentFingerprint: null,
}));
const global = {
  plugins: [i18n],
  stubs: {
    MdFileEntryContextMenu: {
      template: '<div><slot /><button class="delete-entry" @click="$emit(\'delete\')">Delete</button></div>',
    },
    MdTooltip: { template: '<span><slot /></span>' },
    MdNativeFileIcon: true,
    MdIconAction: true,
    MdIcon: true,
    MdAnalysisTreemap: true,
    MdIconSunburst: true,
  },
};
beforeEach(() => {
  vi.spyOn(HTMLElement.prototype, 'offsetHeight', 'get').mockReturnValue(600);
  vi.spyOn(HTMLElement.prototype, 'clientHeight', 'get').mockReturnValue(600);
  vi.spyOn(HTMLElement.prototype, 'offsetWidth', 'get').mockReturnValue(800);
  vi.spyOn(HTMLElement.prototype, 'scrollHeight', 'get').mockReturnValue(5000 * 44);
});
afterEach(() => vi.restoreAllMocks());

describe('analysis details virtualization', () => {
  it('renders a bounded range before painting a large wheel movement and targets its current entry', async () => {
    const wrapper = mount(MdAnalysisDetailsTable, {
      attachTo: document.body,
      props: { entries, openDisabled: false, deleteDisabled: false },
      global,
    });
    await flushPromises();
    expect(wrapper.findAll('.details-row').length).toBeLessThan(100);
    const scroll = wrapper.get('.result-table-scroll').element;
    const wheel = new WheelEvent('wheel', { deltaY: 40000, cancelable: true });
    scroll.dispatchEvent(wheel);
    await flushPromises();
    expect(wheel.defaultPrevented).toBe(true);
    const rows = wrapper.findAll('.virtual-row');
    expect(Number(rows[0]!.attributes('data-index')) * 44).toBeLessThanOrEqual(40000);
    expect((Number(rows.at(-1)!.attributes('data-index')) + 1) * 44).toBeGreaterThanOrEqual(40600);
    const row = rows[0]!;
    await row.get('.delete-entry').trigger('click');
    expect(wrapper.emitted('delete')?.at(-1)).toEqual([entries[Number(row.attributes('data-index'))]]);
    wrapper.unmount();
  });

  it('caches both charts and keeps the sunburst depth when switching back', async () => {
    const wrapper = mount(MdAnalysisVisualPane, {
      attachTo: document.body,
      props: {
        entries,
        result: {
          scanId: 1,
          root: '/files',
          scannedAtMs: 0,
          totalBytes: 1,
          skippedCount: 0,
          truncated: false,
          entries,
        },
        exclusionsActive: false,
        folderCount: 0,
        viewMode: 'sunburst',
        openDisabled: false,
        deleteDisabled: false,
      },
      global,
    });
    await flushPromises();
    const instance = wrapper.findComponent(MdAnalysisSunburst).vm.$.uid;
    wrapper.getComponent(Select).vm.$emit('update:modelValue', '4');
    await flushPromises();
    await wrapper.setProps({ viewMode: 'treemap' });
    expect(wrapper.find('.sunburst-workspace').exists()).toBe(false);
    await wrapper.setProps({ viewMode: 'sunburst' });
    await flushPromises();
    expect(wrapper.findComponent(MdAnalysisSunburst).vm.$.uid).toBe(instance);
    expect(wrapper.getComponent(Select).props('modelValue')).toBe('4');
    wrapper.unmount();
  });
  it('renders loading only for the deleting entry and clears it without recycling other rows', async () => {
    const wrapper = mount(MdAnalysisDetailsTable, {
      attachTo: document.body,
      props: { entries, openDisabled: false, deleteDisabled: false },
      global,
    });
    await flushPromises();
    const before = wrapper.findAll('.virtual-row').map(row => row.element);
    await wrapper.setProps({ deletingPath: entries[0]!.path, openDisabled: true, deleteDisabled: true });
    expect(wrapper.findAll('.md-spinner')).toHaveLength(1);
    const busy = wrapper.get('[aria-busy="true"]');
    expect(busy.attributes('disabled')).toBeDefined();
    expect(busy.text()).toContain(i18n.global.t('analysis.deleting'));
    expect(wrapper.findAll('.virtual-row').map(row => row.element)).toEqual(before);
    await wrapper.setProps({ deletingPath: null, openDisabled: false, deleteDisabled: false });
    expect(wrapper.findAll('.md-spinner')).toHaveLength(0);
    wrapper.unmount();
  });
});

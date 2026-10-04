// @vitest-environment happy-dom
import { flushPromises, mount } from '@vue/test-utils';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { i18n } from '@/i18n';
import type { AnalysisResult } from '@/lib/models/analysis';
import { ByteSizeService } from '@/lib/services/byte-size-service';
import MdAnalysisSunburst from './md-analysis-sunburst.vue';

vi.mock('@tauri-apps/plugin-os', () => ({ platform: () => 'macos' }));
const result: AnalysisResult = {
  scanId: 1,
  root: '/fixture',
  scannedAtMs: 0,
  totalBytes: 1000,
  skippedCount: 0,
  truncated: false,
  entries: [
    {
      name: 'A',
      path: '/fixture/A',
      bytes: 1000,
      fileCount: 2,
      isDirectory: true,
      modifiedAtMs: null,
      contentFingerprint: null,
    },
  ],
  directoryHierarchy: [
    {
      name: 'A',
      path: '/fixture/A',
      bytes: 1000,
      fileCount: 2,
      children: [{ name: 'B', path: '/fixture/A/B', bytes: 600, fileCount: 1, children: [] }],
    },
  ],
};
afterEach(() => vi.restoreAllMocks());
function mountChart() {
  return mount(MdAnalysisSunburst, {
    props: { result, depth: 3, openDisabled: false, deleteDisabled: false },
    attachTo: document.body,
    global: { plugins: [i18n], stubs: { MdIcon: true } },
  });
}
describe('sunburst context actions', () => {
  it('updates only affected sectors when hover changes and retains branch and disabled states', async () => {
    const wrapper = mountChart();
    try {
      await wrapper.setProps({
        result: {
          ...result,
          totalBytes: 1100,
          entries: [...result.entries, { ...result.entries[0]!, name: 'C', path: '/fixture/C', bytes: 100 }],
        },
      });
      const root = wrapper.get('path[aria-label^="A ·"]');
      const deep = wrapper.get('path[aria-label^="B ·"]');
      const unrelated = wrapper.get('path[aria-label^="C ·"]');
      await root.trigger('pointerenter', { clientX: 100, clientY: 100 });
      await flushPromises();
      const format = vi.spyOn(ByteSizeService, 'bytes');
      await deep.trigger('pointerenter', { clientX: 120, clientY: 120 });
      await flushPromises();
      expect(format).not.toHaveBeenCalledWith(100);
      expect(root.classes()).not.toContain('is-highlighted');
      expect(deep.classes()).toContain('is-highlighted');
      await deep.trigger('pointerleave');
      await wrapper.setProps({ hoveredEntryPath: '/fixture/A' });
      expect(root.classes()).toContain('is-highlighted');
      expect(deep.classes()).toContain('is-highlighted');
      expect(unrelated.classes()).not.toContain('is-highlighted');
      await wrapper.setProps({ openDisabled: true });
      const disabledSector = wrapper.get('path[aria-label^="B ·"]');
      expect(disabledSector.classes()).not.toContain('is-highlighted');
      expect(disabledSector.attributes('aria-disabled')).toBe('true');
    } finally {
      wrapper.unmount();
    }
  });
  it('targets the actual SVG sector and keeps projected children read-only', async () => {
    const wrapper = mountChart();
    try {
      const deep = wrapper.get('path[aria-label^="B ·"]');
      await deep.trigger('pointerenter', { clientX: 100, clientY: 100 });
      await flushPromises();
      expect(document.querySelector('.sunburst-tooltip')).not.toBeNull();
      await deep.trigger('contextmenu', { clientX: 100, clientY: 100 });
      await flushPromises();
      let items = document.querySelectorAll('[role="menuitem"]');
      expect(items).toHaveLength(3);
      expect(items[2]?.getAttribute('aria-disabled')).toBe('true');
      expect(document.querySelector('.sunburst-tooltip')).toBeNull();
      items[0]!.dispatchEvent(new MouseEvent('click', { bubbles: true }));
      await flushPromises();
      expect(wrapper.emitted('openEntry')?.[0]?.[0]).toMatchObject({ path: '/fixture/A/B', isDirectory: true });
      await deep.trigger('contextmenu');
      await flushPromises();
      items = document.querySelectorAll('[role="menuitem"]');
      items[1]!.dispatchEvent(new MouseEvent('click', { bubbles: true }));
      await flushPromises();
      expect(wrapper.emitted('reveal')).toEqual([['/fixture/A/B']]);
      expect(wrapper.emitted('delete')).toBeUndefined();
      await wrapper.get('svg').trigger('contextmenu');
      await flushPromises();
      expect(document.querySelector('[role="menu"]')).toBeNull();
      await wrapper.setProps({ result: { ...result, totalBytes: 1200 } });
      await wrapper.get('path.remainder').trigger('contextmenu');
      await flushPromises();
      expect(document.querySelector('[role="menu"]')).toBeNull();
    } finally {
      wrapper.unmount();
    }
  });
  it('retains direct-entry deletion authority and closes menus when the scan or busy state changes', async () => {
    const wrapper = mountChart();
    try {
      await wrapper.get('path[aria-label^="A ·"]').trigger('contextmenu');
      await flushPromises();
      const items = document.querySelectorAll('[role="menuitem"]');
      expect(items[2]?.getAttribute('aria-disabled')).not.toBe('true');
      items[2]!.dispatchEvent(new MouseEvent('click', { bubbles: true }));
      await flushPromises();
      expect(wrapper.emitted('delete')).toEqual([[result.entries[0]]]);
      await wrapper.get('path[aria-label^="B ·"]').trigger('contextmenu');
      await flushPromises();
      expect(document.querySelector('[role="menu"]')).not.toBeNull();
      await wrapper.setProps({ result: { ...result, scanId: 2 } });
      await flushPromises();
      expect(document.querySelector('[role="menu"]')).toBeNull();
      await wrapper.setProps({ openDisabled: true });
      await wrapper.get('path[aria-label^="B ·"]').trigger('contextmenu');
      await flushPromises();
      expect(document.querySelector('[role="menu"]')).toBeNull();
    } finally {
      wrapper.unmount();
    }
  });
});

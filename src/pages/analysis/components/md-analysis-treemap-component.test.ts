// @vitest-environment happy-dom
import { flushPromises, mount } from '@vue/test-utils';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { i18n } from '@/i18n';
import MdFileEntryContextMenu from '@/components/custom/md-file-entry-context-menu.vue';
import { ByteSizeService } from '@/lib/services/byte-size-service';
import * as TreemapLayoutUtils from '@/lib/utils/hierarchical-treemap-layout';
import type { AnalysisResult } from '@/lib/models/analysis';
import MdAnalysisTreemap from './md-analysis-treemap.vue';

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
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe('nested treemap interaction', () => {
  it('restores hover after a scan change closes an open context menu', async () => {
    vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockReturnValue(1200);
    vi.spyOn(HTMLElement.prototype, 'clientHeight', 'get').mockReturnValue(900);
    const wrapper = mount(MdAnalysisTreemap, {
      props: { result, depth: 6, openDisabled: false, deleteDisabled: false },
      global: { plugins: [i18n], stubs: { MdAnalysisEntryIcon: true, MdNativeFileIcon: true, MdIcon: true } },
    });
    try {
      await flushPromises();
      await wrapper.get('button[aria-label^="B ·"]').trigger('contextmenu');
      await flushPromises();
      expect(document.querySelector('[role="menu"]')).not.toBeNull();
      await wrapper.setProps({ openDisabled: true });
      await flushPromises();
      expect(document.querySelector('[role="menu"]')).toBeNull();
      await wrapper.setProps({ result: { ...result, scanId: 2 }, openDisabled: false });
      await wrapper.get('button[aria-label^="B ·"]').trigger('pointerenter', { clientX: 100, clientY: 100 });
      await flushPromises();
      expect(document.querySelector('.treemap-pointer-tooltip')).not.toBeNull();
      expect(wrapper.get('button[aria-label^="B ·"]').classes()).toContain('is-highlighted');
    } finally {
      wrapper.unmount();
    }
  });
  it('links deep hover and provides read-only child context actions', async () => {
    vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockReturnValue(1200);
    vi.spyOn(HTMLElement.prototype, 'clientHeight', 'get').mockReturnValue(900);
    const wrapper = mount(MdAnalysisTreemap, {
      props: { result, openDisabled: false, deleteDisabled: false },
      global: { plugins: [i18n], stubs: { MdAnalysisEntryIcon: true, MdNativeFileIcon: true, MdIcon: true } },
    });
    try {
      await flushPromises();
      expect(wrapper.findAll('.treemap-tile')).toHaveLength(1);
      await wrapper.get('button.treemap-tile').trigger('pointerenter', { clientX: 100, clientY: 100 });
      const outline = wrapper.get('.treemap-hover-outline');
      expect(outline.attributes('style')).toContain('width: 1198px');
      expect(outline.attributes('style')).toContain('height: 898px');
      expect(outline.attributes('style')).toContain('border-radius: var(--radius)');
      await wrapper.setProps({ depth: 6 });
      await flushPromises();
      const deep = wrapper
        .findAll('button.treemap-tile')
        .find(tile => tile.attributes('aria-label')?.startsWith('B ·'))!;
      await deep.trigger('pointerenter', { clientX: 100, clientY: 100 });
      expect(wrapper.emitted('hoverEntry')?.at(-1)).toEqual(['/fixture/A']);
      const tooltip = document.querySelector('.treemap-pointer-tooltip') as HTMLElement;
      expect(tooltip.textContent).toContain('/fixture/A/B');
      expect(tooltip.style.visibility).toBe('visible');
      expect(deep.classes()).toContain('is-highlighted');
      expect(wrapper.findAllComponents(MdFileEntryContextMenu).map(menu => menu.props('enabled'))).toEqual([true]);
      await deep.trigger('contextmenu');
      await flushPromises();
      const items = document.querySelectorAll('[role="menuitem"]');
      expect(items).toHaveLength(3);
      expect(items[2]?.getAttribute('aria-disabled')).toBe('true');
      expect(deep.classes()).toContain('is-highlighted');
      expect(document.querySelector('.treemap-pointer-tooltip')).toBeNull();
      items[0]!.dispatchEvent(new MouseEvent('click', { bubbles: true }));
      await flushPromises();
      expect(wrapper.emitted('openEntry')?.[0]?.[0]).toMatchObject({ path: '/fixture/A/B', isDirectory: true });
      await deep.trigger('click');
      expect(wrapper.emitted('navigate')).toEqual([['/fixture/A/B']]);
      expect(wrapper.emitted('openEntry')).toHaveLength(1);
      expect(wrapper.emitted('delete')).toBeUndefined();
      await wrapper.setProps({ result: { ...result, scannedAtMs: 1 }, openDisabled: true });
      expect(wrapper.props('depth')).toBe(6);
      expect(wrapper.findAll('button.treemap-tile').every(tile => tile.attributes('disabled') !== undefined)).toBe(
        true
      );
    } finally {
      wrapper.unmount();
    }
  });
  it('keeps stationary geometry and unrelated tile labels out of pointer updates', async () => {
    let width = 1200;
    vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockImplementation(() => width);
    vi.spyOn(HTMLElement.prototype, 'clientHeight', 'get').mockReturnValue(900);
    const layout = vi.spyOn(TreemapLayoutUtils, 'layout');
    const format = vi.spyOn(ByteSizeService, 'bytes');
    const wrapper = mount(MdAnalysisTreemap, {
      props: { result, depth: 6, openDisabled: false, deleteDisabled: false },
      global: { plugins: [i18n], stubs: { MdAnalysisEntryIcon: true, MdNativeFileIcon: true, MdIcon: true } },
    });
    try {
      await flushPromises();
      const deep = wrapper.get('button[aria-label^="B ·"]');
      await deep.trigger('pointerenter', { clientX: 100, clientY: 100 });
      await flushPromises();
      format.mockClear();
      await deep.trigger('pointermove', { clientX: 120, clientY: 120 });
      await flushPromises();
      expect(format).not.toHaveBeenCalledWith(1000);
      layout.mockClear();
      window.dispatchEvent(new Event('resize'));
      await flushPromises();
      expect(layout).not.toHaveBeenCalled();
      expect(document.querySelector('.treemap-pointer-tooltip')).not.toBeNull();
      width = 800;
      window.dispatchEvent(new Event('resize'));
      await flushPromises();
      expect(layout).toHaveBeenCalledOnce();
      expect(document.querySelector('.treemap-pointer-tooltip')).toBeNull();
    } finally {
      wrapper.unmount();
    }
  });
  it('keeps observing the current canvas after a scan replaces the menu trigger', async () => {
    const observers: { targets: Set<Element>; callback: ResizeObserverCallback }[] = [];
    vi.stubGlobal(
      'ResizeObserver',
      class {
        targets = new Set<Element>();
        constructor(callback: ResizeObserverCallback) {
          observers.push({ targets: this.targets, callback });
        }
        observe(target: Element) {
          this.targets.add(target);
        }
        unobserve(target: Element) {
          this.targets.delete(target);
        }
        disconnect() {
          this.targets.clear();
        }
      }
    );
    vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockReturnValue(800);
    vi.spyOn(HTMLElement.prototype, 'clientHeight', 'get').mockReturnValue(900);
    const wrapper = mount(MdAnalysisTreemap, {
      props: { result, depth: 6, openDisabled: false, deleteDisabled: false },
      global: { plugins: [i18n], stubs: { MdAnalysisEntryIcon: true, MdNativeFileIcon: true, MdIcon: true } },
    });
    try {
      await flushPromises();
      await wrapper.setProps({ openDisabled: true });
      await wrapper.setProps({ openDisabled: false, result: { ...result, scanId: 2 } });
      await flushPromises();
      const canvas = wrapper.get('.treemap').element;
      const observer = observers.find(item => item.targets.has(canvas));
      expect(observer, 'the live canvas must remain observed after a scan').toBeDefined();
      for (const width of [1400, 800, 1400]) {
        observer!.callback(
          [{ target: canvas, contentRect: { width, height: 900 } } as ResizeObserverEntry],
          {} as ResizeObserver
        );
        await flushPromises();
        expect((wrapper.get('button[aria-label^="A ·"]').element as HTMLElement).style.width).toBe(`${width - 2}px`);
      }
    } finally {
      wrapper.unmount();
    }
    expect(observers.every(item => item.targets.size === 0)).toBe(true);
  });
  it('retargets the shared menu and never reuses it on empty canvas or other tiles', async () => {
    vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockReturnValue(1200);
    vi.spyOn(HTMLElement.prototype, 'clientHeight', 'get').mockReturnValue(900);
    const wrapper = mount(MdAnalysisTreemap, {
      props: { result, depth: 6, openDisabled: false, deleteDisabled: false },
      global: { plugins: [i18n], stubs: { MdAnalysisEntryIcon: true, MdNativeFileIcon: true, MdIcon: true } },
    });
    try {
      await flushPromises();
      await wrapper.get('button[aria-label^="B ·"]').trigger('contextmenu');
      await flushPromises();
      let items = document.querySelectorAll('[role="menuitem"]');
      items[1]!.dispatchEvent(new MouseEvent('click', { bubbles: true }));
      await flushPromises();
      expect(wrapper.emitted('reveal')).toEqual([['/fixture/A/B']]);
      await wrapper.get('button[aria-label^="A ·"]').trigger('contextmenu');
      await flushPromises();
      items = document.querySelectorAll('[role="menuitem"]');
      expect(items[2]!.getAttribute('aria-disabled')).not.toBe('true');
      items[2]!.dispatchEvent(new MouseEvent('click', { bubbles: true }));
      await flushPromises();
      expect(wrapper.emitted('delete')).toEqual([[result.entries[0]]]);
      await wrapper.get('.treemap').trigger('contextmenu');
      await flushPromises();
      expect(document.querySelector('[role="menu"]')).toBeNull();
      await wrapper.get('button.remainder').trigger('contextmenu');
      await flushPromises();
      expect(document.querySelector('[role="menu"]')).toBeNull();
      await wrapper.get('button[aria-label^="B ·"]').trigger('contextmenu');
      await flushPromises();
      await wrapper.setProps({ depth: 1 });
      await flushPromises();
      expect(document.querySelector('[role="menu"]')).toBeNull();
    } finally {
      wrapper.unmount();
    }
  });
  it('labels nested aggregates with counts in every supported locale', async () => {
    vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockReturnValue(1200);
    vi.spyOn(HTMLElement.prototype, 'clientHeight', 'get').mockReturnValue(900);
    const originalLocale = i18n.global.locale.value;
    const wrapper = mount(MdAnalysisTreemap, {
      props: {
        result: {
          ...result,
          directoryHierarchy: [
            {
              name: 'A',
              path: '/fixture/A',
              bytes: 600,
              fileCount: 5000,
              totalEntryCount: 123,
              children: [{ name: 'child', path: '/fixture/A/child', bytes: 200, fileCount: 4999, children: [] }],
            },
          ],
        },
        depth: 3,
        openDisabled: false,
        deleteDisabled: false,
      },
      global: { plugins: [i18n], stubs: { MdAnalysisEntryIcon: true, MdNativeFileIcon: true, MdIcon: true } },
    });
    try {
      for (const locale of i18n.global.availableLocales) {
        i18n.global.locale.value = locale;
        await flushPromises();
        expect(wrapper.text()).toContain(i18n.global.t('analysis.treemapRemainder', { count: '122' }, 122));
      }
    } finally {
      i18n.global.locale.value = originalLocale;
      wrapper.unmount();
    }
  });
  it('labels the aggregate with its complete count and opens the exact omitted selection', async () => {
    vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockReturnValue(1200);
    vi.spyOn(HTMLElement.prototype, 'clientHeight', 'get').mockReturnValue(900);
    const wrapper = mount(MdAnalysisTreemap, {
      props: {
        result: { ...result, totalEntryCount: 21, truncated: true, entries: [{ ...result.entries[0]!, bytes: 800 }] },
        openDisabled: false,
        deleteDisabled: false,
      },
      global: { plugins: [i18n], stubs: { MdAnalysisEntryIcon: true, MdNativeFileIcon: true, MdIcon: true } },
    });
    try {
      await flushPromises();
      const other = wrapper.findAll('button.treemap-tile').find(tile => tile.text().includes('20'))!;
      expect(other.text()).toContain(i18n.global.t('analysis.treemapRemainder', { count: '20' }, 20));
      await other.trigger('click');
      expect(wrapper.emitted('showRemainder')).toEqual([
        [{ parentPath: '/fixture', bytes: 200, visiblePaths: ['/fixture/A'] }],
      ]);
      await wrapper.setProps({ result: { ...result, entries: [{ ...result.entries[0]!, bytes: 800 }] } });
      expect(
        wrapper
          .findAll('button.treemap-tile')
          .find(tile => tile.text().includes('200'))!
          .text()
      ).toContain(i18n.global.t('analysis.other'));
    } finally {
      wrapper.unmount();
    }
  });
});

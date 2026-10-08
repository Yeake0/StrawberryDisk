// @vitest-environment happy-dom

import { flushPromises, mount } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/plugin-os', () => ({ platform: () => 'macos' }));

import { i18n } from '@/i18n';
import type { LargeFileEntry } from '@/lib/models/large-file';

import MdLargeFileList from './md-large-file-list.vue';

const resizeCallbacks = new Set<ResizeObserverCallback>();

function mountList() {
  const entries: LargeFileEntry[] = Array.from({ length: 1200 }, (_, index) => ({
    name: `file-${index}.gguf`,
    path: `/files/file-${index}.gguf`,
    parentPath: '/files',
    bytes: 1200 - index,
    modifiedAtMs: null,
  }));
  return mount(MdLargeFileList, {
    attachTo: document.body,
    props: { entries, selectedPaths: [], openDisabled: false, deleteDisabled: false },
    global: {
      plugins: [i18n],
      stubs: {
        MdFileEntryContextMenu: {
          template: '<div><slot /><button class="delete-entry" @click="$emit(\'delete\')">Delete</button></div>',
        },
        MdResultTableRow: { template: '<div class="file-row"><slot /></div>' },
        MdResultCheckbox: true,
        MdNativeFileIcon: true,
        MdMiddleEllipsis: true,
        MdTooltip: { template: '<div><slot /></div>' },
        MdAiAction: {
          props: ['name', 'disabled'],
          template: '<button class="explain-entry" :disabled="disabled" @click="$emit(\'explain\')">Explain</button>',
        },
        MdIconAction: true,
        MdIcon: true,
        MdLoadMoreButton: { template: '<button class="load-more" @click="$emit(\'loadMore\')">More</button>' },
      },
    },
  });
}

beforeEach(() => {
  // happy-dom has no layout engine. Keep the real scroll element and virtualizer
  // but supply the viewport dimensions that a WebView normally reports.
  vi.spyOn(HTMLElement.prototype, 'offsetHeight', 'get').mockReturnValue(600);
  vi.spyOn(HTMLElement.prototype, 'clientHeight', 'get').mockReturnValue(600);
  vi.spyOn(HTMLElement.prototype, 'scrollHeight', 'get').mockReturnValue(200000);
  vi.spyOn(HTMLElement.prototype, 'offsetWidth', 'get').mockReturnValue(800);
  vi.stubGlobal(
    'ResizeObserver',
    class {
      constructor(callback: ResizeObserverCallback) {
        resizeCallbacks.add(callback);
      }
      observe() {}
      unobserve() {}
      disconnect() {}
    }
  );
});

afterEach(() => {
  resizeCallbacks.clear();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe('large file list', () => {
  it('reuses a bounded row pool and covers the viewport after large scroll jumps', async () => {
    const wrapper = mountList();
    await flushPromises();
    for (let batch = 0; batch < 12; batch += 1) await wrapper.get('.load-more').trigger('click');
    const scroll = wrapper.get('.result-table-scroll');
    const pool = new Set(wrapper.findAll('.virtual-row').map(row => row.element));
    expect(pool.size).toBeGreaterThan(14);
    expect(pool.size).toBeLessThan(100);

    for (const offset of [9000, 44000, 18000, 0]) {
      const wheel = new WheelEvent('wheel', { deltaY: offset - scroll.element.scrollTop, cancelable: true });
      scroll.element.dispatchEvent(wheel);
      expect(wheel.defaultPrevented).toBe(true);
      await flushPromises();
      const rows = wrapper.findAll('.virtual-row');
      const firstIndex = Number(rows[0]?.attributes('data-index'));
      const lastIndex = Number(rows.at(-1)?.attributes('data-index'));
      expect(firstIndex * 44).toBeLessThanOrEqual(offset);
      expect((lastIndex + 1) * 44).toBeGreaterThanOrEqual(offset + 600);
      expect(rows.every(row => pool.has(row.element))).toBe(true);
    }

    // Reused controls must address their current file, including destructive actions.
    const previouslyFocused = wrapper.findAll('.virtual-row')[0]!.get('.delete-entry').element as HTMLButtonElement;
    previouslyFocused.focus();
    expect(document.activeElement).toBe(previouslyFocused);
    scroll.element.scrollTop = 20000;
    await scroll.trigger('scroll');
    await flushPromises();
    expect(document.activeElement).not.toBe(previouslyFocused);
    const row = wrapper.findAll('.virtual-row')[0]!;
    const index = Number(row.attributes('data-index'));
    await row.get('.explain-entry').trigger('click');
    expect(wrapper.emitted('explain')?.at(-1)).toEqual([wrapper.props('entries')[index]]);
    await row.get('.delete-entry').trigger('click');
    expect(wrapper.emitted('delete')?.at(-1)).toEqual([wrapper.props('entries')[index]]);
    wrapper.unmount();
  });

  it('retains rendered rows when a cached page reports a zero-sized viewport', async () => {
    const wrapper = mountList();
    await flushPromises();
    const pool = wrapper.findAll('.virtual-row').map(row => row.element);
    const target = wrapper.get('.result-table-scroll').element;
    const entry = {
      target,
      borderBoxSize: [{ inlineSize: 0, blockSize: 0 }],
    } as unknown as ResizeObserverEntry;
    for (const callback of resizeCallbacks) callback([entry], {} as ResizeObserver);
    await flushPromises();
    expect(wrapper.findAll('.virtual-row').map(row => row.element)).toEqual(pool);
    expect(pool.length).toBeGreaterThan(0);
    wrapper.unmount();
  });
});

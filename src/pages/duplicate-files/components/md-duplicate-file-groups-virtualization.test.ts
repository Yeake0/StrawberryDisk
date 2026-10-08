// @vitest-environment happy-dom

import { flushPromises, mount } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/plugin-os', () => ({ platform: () => 'macos' }));

import { i18n } from '@/i18n';
import type { DuplicateGroup } from '@/lib/models/duplicate-file';
import MdDuplicateFileGroups from './md-duplicate-file-groups.vue';

function mountGroups() {
  const groups: DuplicateGroup[] = Array.from({ length: 40 }, (_, index) => ({
    id: `group-${index}`,
    hash: `hash-${index}`,
    kind: 'file',
    bytesPerFile: 1024,
    fileCountPerEntry: 1,
    reclaimableBytes: 1024 * 159,
    entries: Array.from({ length: 160 }, (_, member) => ({
      name: `file-${index}-${member}.gguf`,
      path: `/files/file-${index}-${member}.gguf`,
      parentPath: '/files',
      bytes: 1024,
      allocatedBytes: 1024,
      modifiedAtMs: null,
      deletePolicy: member === 0 ? 'protected' : 'cleanable',
    })),
  }));
  return mount(MdDuplicateFileGroups, {
    attachTo: document.body,
    props: {
      scanId: 1,
      category: 'all',
      groups,
      keeperRule: 'shortestPath',
      selectedPaths: [],
      selectionDisabled: false,
      openDisabled: false,
      deleteDisabled: false,
      hasMore: false,
      loadingMore: false,
      remainingGroupCount: 0,
    },
    global: {
      plugins: [i18n],
      stubs: {
        MdFileEntryContextMenu: {
          props: ['deleteDisabled'],
          template:
            '<div><slot /><button class="delete-entry" :disabled="deleteDisabled" @click="$emit(\'delete\')">Delete</button></div>',
        },
        MdResultTableRow: { template: '<div><slot /></div>' },
        MdResultCheckbox: {
          props: ['disabled', 'checked'],
          template:
            '<button class="checkbox" :disabled="disabled" @click="$emit(\'update:checked\', !checked)">Select</button>',
        },
        MdMiddleEllipsis: true,
        MdNativeFileIcon: true,
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
  // Keep real scroll events and virtualization; happy-dom needs explicit layout dimensions.
  vi.spyOn(HTMLElement.prototype, 'offsetHeight', 'get').mockReturnValue(600);
  vi.spyOn(HTMLElement.prototype, 'clientHeight', 'get').mockReturnValue(600);
  vi.spyOn(HTMLElement.prototype, 'scrollHeight', 'get').mockReturnValue(200000);
  vi.spyOn(HTMLElement.prototype, 'offsetWidth', 'get').mockReturnValue(800);
});

afterEach(() => vi.restoreAllMocks());

describe('duplicate member virtualization', () => {
  it('bounds mounted members independently of group size and retargets recycled actions safely', async () => {
    const wrapper = mountGroups();
    await flushPromises();
    const scroll = wrapper.get('.result-table-scroll');
    const pool = new Set(wrapper.findAll('.virtual-row').map(row => row.element));
    expect(pool.size).toBeLessThan(100);
    expect(pool.size).toBeGreaterThan(15);
    const oldButton = wrapper.findAll('.delete-entry')[1]!.element as HTMLButtonElement;
    oldButton.focus();
    for (const offset of [9000, 44000, 18000, 0]) {
      const wheel = new WheelEvent('wheel', { deltaY: offset - scroll.element.scrollTop, cancelable: true });
      scroll.element.dispatchEvent(wheel);
      expect(wheel.defaultPrevented).toBe(true);
      await flushPromises();
      const rows = wrapper.findAll('.virtual-row');
      // Header/footer counts can change at a window boundary; member controls
      // should still be reused instead of mounting entire newly visible groups.
      expect(rows.filter(row => pool.has(row.element)).length).toBeGreaterThan(rows.length - 8);
      for (const row of rows) pool.add(row.element);
      expect(rows.length).toBeLessThan(100);
      const top = Number.parseFloat(
        wrapper
          .get('.virtual-window')
          .attributes('style')!
          .match(/top: ([\d.]+)/)![1]!
      );
      const height = rows.reduce((sum, row) => sum + Number.parseFloat((row.element as HTMLElement).style.height), 0);
      expect(top).toBeLessThanOrEqual(offset);
      expect(top + height).toBeGreaterThanOrEqual(offset + 600);
    }
    expect(document.activeElement).not.toBe(oldButton);
    scroll.element.scrollTop = 22000;
    await scroll.trigger('scroll');
    await flushPromises();
    const member = wrapper
      .findAll('.virtual-row-member')
      .find(row => row.get('.delete-entry').attributes('disabled') === undefined)!;
    const path = member.get('.checkbox').attributes('aria-label')!;
    const group = wrapper.props('groups').find(candidate => candidate.entries.some(entry => entry.path === path))!;
    const entry = group.entries.find(candidate => candidate.path === path)!;
    await member.get('.explain-entry').trigger('click');
    expect(wrapper.emitted('explain')?.at(-1)).toEqual([group, entry]);
    await member.get('.delete-entry').trigger('click');
    expect(wrapper.emitted('delete')?.at(-1)).toEqual([entry]);
    await member.get('.checkbox').trigger('click');
    expect(wrapper.emitted('update:selectedPaths')?.at(-1)).toEqual([[path]]);
    wrapper.unmount();
  });

  it('covers the viewport after thumb jumps across the entire loaded list', async () => {
    const wrapper = mountGroups();
    await flushPromises();
    const scroll = wrapper.get('.result-table-scroll').element as HTMLElement;
    const total = Number.parseFloat((wrapper.get('.virtual-content').element as HTMLElement).style.height);
    Object.defineProperty(scroll, 'scrollHeight', { value: total });
    scroll.dispatchEvent(new Event('scroll'));
    await flushPromises();
    const track = wrapper.get('[role="scrollbar"]').element as HTMLElement;
    track.setPointerCapture = vi.fn();
    track.hasPointerCapture = vi.fn(() => false);
    track.dispatchEvent(new PointerEvent('pointerdown', { button: 0, pointerId: 1, clientY: 4 }));
    for (const clientY of [598, 4, 570, 30, 598, 4]) {
      track.dispatchEvent(new PointerEvent('pointermove', { pointerId: 1, clientY }));
      await flushPromises();
      const rows = wrapper.findAll('.virtual-row');
      const top = Number.parseFloat((wrapper.get('.virtual-window').element as HTMLElement).style.top);
      const height = rows.reduce((sum, row) => sum + Number.parseFloat((row.element as HTMLElement).style.height), 0);
      expect(top).toBeLessThanOrEqual(scroll.scrollTop);
      expect(top + height).toBeGreaterThanOrEqual(scroll.scrollTop + 600);
      expect(rows.length).toBeLessThan(100);
    }
    track.dispatchEvent(new PointerEvent('pointerup', { pointerId: 1 }));
    wrapper.unmount();
  });

  it('keeps exact offsets when expanding members or collapsing groups and preserves protected controls', async () => {
    const wrapper = mountGroups();
    await flushPromises();
    const height = () => Number.parseFloat((wrapper.get('.virtual-content').element as HTMLElement).style.height);
    const initialHeight = height();
    const protectedRow = wrapper.findAll('.virtual-row-member')[0]!;
    expect(protectedRow.get('.checkbox').attributes('disabled')).toBeDefined();
    expect(protectedRow.get('.delete-entry').attributes('disabled')).toBeDefined();
    await wrapper.get('.group-disclosure').trigger('click');
    expect(height()).toBe(initialHeight - 80 * 40 - 42);
    await wrapper.get('.group-disclosure').trigger('click');
    expect(height()).toBe(initialHeight);
    const scroll = wrapper.get('.result-table-scroll');
    scroll.element.scrollTop = 3000;
    await scroll.trigger('scroll');
    await flushPromises();
    await wrapper.get('.virtual-row-more .load-more').trigger('click');
    expect(height()).toBe(initialHeight + 80 * 40 - 42);
    expect(wrapper.findAll('.virtual-row').length).toBeLessThan(100);
    wrapper.unmount();
  });
});

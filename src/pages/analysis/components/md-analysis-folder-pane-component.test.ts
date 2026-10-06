// @vitest-environment happy-dom

import { flushPromises, mount } from '@vue/test-utils';
import { defineComponent, h, KeepAlive, ref } from 'vue';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/plugin-os', () => ({ platform: () => 'macos' }));

import { i18n } from '@/i18n';
import { LANGUAGE_IDS } from '@/lib/models/settings';

import MdAnalysisFolderPane from './md-analysis-folder-pane.vue';

describe('analysis folder pane', () => {
  it('places the limit help immediately after the visible counts', async () => {
    const wrapper = mount(MdAnalysisFolderPane, {
      props: {
        entries: largeEntries.slice(0, 100),
        totalBytes: 125250,
        folderCount: 100,
        fileCount: 4997,
        truncated: false,
        openDisabled: false,
        deleteDisabled: false,
      },
      global: {
        plugins: [i18n],
        stubs: {
          MdIcon: true,
          MdTooltip: { template: '<span><slot /></span>', props: ['text'] },
        },
      },
    });

    expect(wrapper.find('.md-help-action').exists()).toBe(false);
    await wrapper.setProps({ truncated: true });
    const header = wrapper.get('header');
    expect(header.get('p').text()).toBe(`100 folders · ${new Intl.NumberFormat().format(4997)} files`);
    expect(header.get('.md-help-action').attributes('aria-label')).toBe('Showing up to 100 largest items');
    expect(header.element.children[1]?.contains(header.get('.md-help-action').element)).toBe(true);
  });
});

const largeEntries = Array.from({ length: 500 }, (_, index) => ({
  name: `file-${index}.bin`,
  path: `/fixture/${index}.bin`,
  bytes: 500 - index,
  fileCount: 1,
  isDirectory: false,
  modifiedAtMs: null,
  contentFingerprint: null,
}));

describe('analysis folder pane virtual scrolling', () => {
  beforeEach(() => {
    vi.spyOn(HTMLElement.prototype, 'offsetHeight', 'get').mockReturnValue(600);
    vi.spyOn(HTMLElement.prototype, 'offsetWidth', 'get').mockReturnValue(400);
    vi.spyOn(HTMLElement.prototype, 'clientHeight', 'get').mockReturnValue(600);
    vi.spyOn(HTMLElement.prototype, 'scrollHeight', 'get').mockReturnValue(500 * 52);
    vi.spyOn(HTMLElement.prototype, 'scrollTo').mockImplementation(function (
      this: HTMLElement,
      options: ScrollToOptions | number
    ) {
      if (typeof options === 'object') this.scrollTop = options.top ?? this.scrollTop;
      this.dispatchEvent(new Event('scroll'));
    });
  });
  afterEach(() => vi.restoreAllMocks());
  function createPane(realTriggers = false) {
    return mount(MdAnalysisFolderPane, {
      attachTo: document.body,
      props: {
        entries: largeEntries,
        totalBytes: 125250,
        folderCount: 0,
        fileCount: 500,
        truncated: true,
        openDisabled: false,
        deleteDisabled: false,
      },
      global: {
        plugins: [i18n],
        stubs: {
          MdAnalysisEntryIcon: true,
          MdIcon: true,
          MdTooltip: realTriggers ? false : { template: '<span><slot /></span>' },
          MdFileEntryContextMenu: realTriggers
            ? false
            : {
                template: '<div><slot /><button class="test-delete" @click="$emit(\'delete\')">delete</button></div>',
              },
        },
      },
    });
  }
  it('bounds mounted rows and retargets recycled hover, click and delete actions after a large scroll', async () => {
    const wrapper = createPane();
    try {
      await flushPromises();
      expect(wrapper.findAll('.folder-row').length).toBeLessThan(45);
      const scroll = wrapper.get('.result-table-scroll');
      scroll.element.scrollTop = 24000;
      await scroll.trigger('scroll');
      await flushPromises();
      const row = wrapper.get('[data-index="462"]');
      const expected = largeEntries[462]!;
      expect(row.text()).toContain(expected.name);
      await row.get('.folder-row').trigger('pointerenter');
      expect(wrapper.emitted('hoverEntry')?.at(-1)).toEqual([expected.path]);
      await row.get('.folder-entry').trigger('click');
      expect(wrapper.emitted('activate')?.at(-1)).toEqual([expected]);
      await row.get('.test-delete').trigger('click');
      expect(wrapper.emitted('delete')?.at(-1)).toEqual([expected]);
      expect(wrapper.findAll('.folder-row').length).toBeLessThan(45);
    } finally {
      wrapper.unmount();
    }
  });
  it('updates rows in the same input task for rapid wheel and scrollbar jumps', async () => {
    const wrapper = createPane();
    try {
      await flushPromises();
      const viewport = wrapper.get('.result-table-scroll');
      for (const offset of [24000, -20000, 14000, -18000]) {
        const wheel = new WheelEvent('wheel', { deltaY: offset, cancelable: true });
        viewport.element.dispatchEvent(wheel);
        expect(wheel.defaultPrevented).toBe(true);
        await flushPromises();
        const firstVisible = Math.floor(viewport.element.scrollTop / 52);
        expect(wrapper.find(`[data-index="${firstVisible}"]`).exists()).toBe(true);
        expect(wrapper.findAll('.folder-row').length).toBeLessThan(45);
      }
      const scrollbar = wrapper.get('[role="scrollbar"]');
      await scrollbar.trigger('keydown', { key: 'End' });
      await flushPromises();
      expect(wrapper.find('[data-index="499"]').exists()).toBe(true);
      await scrollbar.trigger('keydown', { key: 'Home' });
      await flushPromises();
      expect(wrapper.find('[data-index="0"]').exists()).toBe(true);
    } finally {
      wrapper.unmount();
    }
  });

  it('restores the viewport after WebKit resets its offset during cached page navigation', async () => {
    const visible = ref(true);
    const host = mount(
      defineComponent({
        setup: () => () =>
          h(KeepAlive, {}, () =>
            visible.value
              ? h(MdAnalysisFolderPane, {
                  entries: largeEntries,
                  totalBytes: 125250,
                  folderCount: 0,
                  fileCount: 500,
                  truncated: true,
                  openDisabled: false,
                  deleteDisabled: false,
                })
              : h('div')
          ),
      }),
      {
        attachTo: document.body,
        global: { plugins: [i18n], stubs: { MdAnalysisEntryIcon: true, MdIcon: true } },
      }
    );
    try {
      await flushPromises();
      const viewport = host.get('.result-table-scroll').element;
      viewport.scrollTop = 24000;
      viewport.dispatchEvent(new Event('scroll'));
      await flushPromises();
      visible.value = false;
      await flushPromises();
      viewport.scrollTop = 0;
      viewport.dispatchEvent(new Event('scroll'));
      visible.value = true;
      await flushPromises();
      expect(host.get('.result-table-scroll').element.scrollTop).toBe(24000);
      expect(host.find('[data-index="462"]').exists()).toBe(true);
    } finally {
      host.unmount();
    }
  });

  it('retains keyboard focus through real context-menu trigger recycling', async () => {
    const wrapper = createPane(true);
    try {
      await flushPromises();
      const first = wrapper.get('.folder-entry');
      await first.trigger('click');
      expect(document.activeElement).toBe(first.element);
      await first.trigger('keydown', { key: 'End' });
      await flushPromises();
      const last = wrapper.get('[data-index="499"] .folder-entry');
      expect(document.activeElement).toBe(last.element);
      await last.trigger('keydown', { key: 'ArrowUp' });
      await flushPromises();
      expect(document.activeElement).toBe(wrapper.get('[data-index="498"] .folder-entry').element);
    } finally {
      wrapper.unmount();
    }
  });

  it('waits for a delayed native scroll event before focusing the last row', async () => {
    vi.mocked(HTMLElement.prototype.scrollTo).mockImplementation(function (
      this: HTMLElement,
      options: ScrollToOptions | number
    ) {
      if (typeof options === 'object') this.scrollTop = options.top ?? this.scrollTop;
    });
    const wrapper = createPane();
    try {
      await flushPromises();
      await wrapper.get('.folder-entry').trigger('keydown', { key: 'End' });
      await flushPromises();
      expect(wrapper.find('[data-index="499"]').exists()).toBe(false);
      await wrapper.get('.result-table-scroll').trigger('scroll');
      await flushPromises();
      expect(document.activeElement).toBe(wrapper.get('[data-index="499"] .folder-entry').element);
    } finally {
      wrapper.unmount();
    }
  });

  it('interpolates the visible limit in every supported locale without changing the row pool', async () => {
    const previous = i18n.global.locale.value;
    const wrapper = createPane();
    try {
      for (const locale of Object.values(LANGUAGE_IDS)) {
        i18n.global.locale.value = locale;
        await flushPromises();
        const label = wrapper.get('.md-help-action').attributes('aria-label');
        expect(label).not.toContain('{count}');
        expect(label).not.toContain('analysis.limitedEntries');
        expect(wrapper.findAll('.folder-row').length).toBeLessThan(45);
        expect(wrapper.get('.item-metrics small').text()).toBe('<1%');
      }
    } finally {
      i18n.global.locale.value = previous;
      wrapper.unmount();
    }
  });

  it('supports keyboard navigation to the last row and resets the scroll position for a new result', async () => {
    const wrapper = createPane();
    try {
      await flushPromises();
      await wrapper.get('.folder-entry').trigger('keydown', { key: 'End' });
      await flushPromises();
      const last = wrapper.get('[data-index="499"] .folder-entry');
      expect(document.activeElement).toBe(last.element);
      await last.trigger('keydown', { key: 'ArrowUp' });
      await flushPromises();
      expect(document.activeElement).toBe(wrapper.get('[data-index="498"] .folder-entry').element);
      await wrapper.setProps({ entries: largeEntries.slice(0, 2) });
      await flushPromises();
      expect(wrapper.get('.result-table-scroll').element.scrollTop).toBe(0);
      expect(wrapper.findAll('.folder-row')).toHaveLength(2);
      expect(wrapper.get('.md-help-action').attributes('aria-label')).toContain('2');
    } finally {
      wrapper.unmount();
    }
  });
});

// @vitest-environment happy-dom

import { flushPromises, mount } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { i18n } from '@/i18n';
import { LANGUAGE_IDS } from '@/lib/models/settings';
import type { DirectoryEntryInfo } from '@/lib/models/analysis';
import MdAnalysisOtherDialog from './md-analysis-other-dialog.vue';

const { listMock, releaseMock } = vi.hoisted(() => ({ listMock: vi.fn(), releaseMock: vi.fn() }));
vi.mock('@/lib/services/analysis-service', () => ({
  AnalysisService: { listRemainder: listMock, releaseRemainder: releaseMock },
}));
vi.mock('@/lib/services/logger-service', () => ({ LoggerService: { warn: vi.fn() } }));
vi.mock('@tauri-apps/plugin-os', () => ({ platform: () => 'macos' }));
const passthrough = { template: '<div><slot /></div>' };
const dialogContent = { template: '<div data-slot="dialog-content"><slot /></div>' };
let frames: Map<number, FrameRequestCallback>;
let frameId = 0;
const animationsMock = vi.fn();

async function paintFrame() {
  const pending = [...frames.values()];
  frames.clear();
  for (const callback of pending) callback(performance.now());
  await flushPromises();
}

async function finishOpening() {
  await flushPromises();
  await paintFrame();
  await paintFrame();
}
const selection = { parentPath: '/fixture', bytes: 64, visiblePaths: ['/fixture/large'] };
const entry: DirectoryEntryInfo = {
  name: 'nested',
  path: '/fixture/nested',
  bytes: 32,
  fileCount: 1,
  isDirectory: true,
  modifiedAtMs: null,
  contentFingerprint: null,
};
const page = {
  schemaVersion: 2,
  snapshotId: 101,
  parentPath: '/fixture',
  totalBytes: 64,
  totalCount: 2,
  entries: [entry],
  nextOffset: 1,
};
function mountDialog() {
  return mount(MdAnalysisOtherDialog, {
    attachTo: document.body,
    props: { scanId: 7, selection },
    global: {
      plugins: [i18n],
      stubs: {
        Dialog: passthrough,
        DialogTitle: passthrough,
        DialogDescription: passthrough,
        MdDialogContent: dialogContent,
        MdDialogHeader: passthrough,
        MdDialogFooter: passthrough,
        MdTooltip: passthrough,
        MdAnalysisEntryIcon: true,
      },
    },
  });
}

describe('analysis other details', () => {
  const originalLocale = i18n.global.locale.value;
  beforeEach(() => {
    vi.spyOn(HTMLElement.prototype, 'offsetHeight', 'get').mockReturnValue(400);
    vi.spyOn(HTMLElement.prototype, 'offsetWidth', 'get').mockReturnValue(600);
    vi.spyOn(HTMLElement.prototype, 'clientHeight', 'get').mockReturnValue(400);
    listMock.mockReset();
    releaseMock.mockReset().mockResolvedValue(undefined);
    frames = new Map();
    vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => {
      const id = ++frameId;
      frames.set(id, callback);
      return id;
    });
    vi.stubGlobal('cancelAnimationFrame', (id: number) => frames.delete(id));
    animationsMock.mockReset().mockReturnValue([]);
    vi.spyOn(Element.prototype, 'getAnimations').mockImplementation(animationsMock);
    i18n.global.locale.value = LANGUAGE_IDS.ptBR;
  });
  afterEach(() => {
    i18n.global.locale.value = originalLocale;
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it('shows loading immediately and waits for the opening animation before reading files', async () => {
    let finishAnimation!: () => void;
    animationsMock.mockReturnValue([
      {
        finished: new Promise<void>(resolve => {
          finishAnimation = resolve;
        }),
      },
    ]);
    listMock.mockResolvedValue({ ...page, nextOffset: null });
    const wrapper = mountDialog();
    try {
      await flushPromises();
      expect(wrapper.get('.other-body').attributes('aria-busy')).toBe('true');
      expect(wrapper.find('[role="status"]').exists()).toBe(true);
      expect(listMock).not.toHaveBeenCalled();
      await paintFrame();
      expect(listMock).not.toHaveBeenCalled();
      finishAnimation();
      await flushPromises();
      expect(listMock).not.toHaveBeenCalled();
      await paintFrame();
      expect(listMock).toHaveBeenCalledExactlyOnceWith(7, selection, 0, null);
      expect(wrapper.get('.other-name').text()).toBe('nested');
    } finally {
      wrapper.unmount();
    }
  });

  it('cancels a queued opening read when closed before the next paint', async () => {
    const wrapper = mountDialog();
    try {
      await flushPromises();
      await wrapper.setProps({ selection: null });
      await finishOpening();
      expect(listMock).not.toHaveBeenCalled();
      expect(frames.size).toBe(0);
    } finally {
      wrapper.unmount();
    }
  });

  it('starts after a paint without a timer when opening animations are disabled', async () => {
    listMock.mockResolvedValue({ ...page, nextOffset: null });
    const wrapper = mountDialog();
    try {
      await flushPromises();
      expect(listMock).not.toHaveBeenCalled();
      await paintFrame();
      expect(listMock).not.toHaveBeenCalled();
      await paintFrame();
      expect(listMock).toHaveBeenCalledExactlyOnceWith(7, selection, 0, null);
    } finally {
      wrapper.unmount();
    }
  });

  it('does not read a previous scope after its opening animation finishes', async () => {
    let finishAnimation!: () => void;
    animationsMock.mockReturnValueOnce([
      {
        finished: new Promise<void>(resolve => {
          finishAnimation = resolve;
        }),
      },
    ]);
    listMock.mockResolvedValue({ ...page, nextOffset: null });
    const wrapper = mountDialog();
    try {
      await flushPromises();
      await paintFrame();
      const current = { ...selection, parentPath: '/second' };
      await wrapper.setProps({ selection: current });
      await finishOpening();
      finishAnimation();
      await flushPromises();
      await paintFrame();
      expect(listMock).toHaveBeenCalledExactlyOnceWith(7, current, 0, null);
    } finally {
      wrapper.unmount();
    }
  });

  it('localizes recovery and retries the same scope without empty column headers', async () => {
    listMock.mockRejectedValueOnce(new Error('expired')).mockResolvedValueOnce({ ...page, nextOffset: null });
    const wrapper = mountDialog();
    try {
      await finishOpening();
      expect(wrapper.text()).toContain('Tentar novamente');
      expect(wrapper.text()).not.toContain('common.retry');
      expect(wrapper.find('.other-columns').exists()).toBe(false);
      await wrapper.get('.other-recovery button').trigger('click');
      await flushPromises();
      expect(wrapper.get('.other-name').text()).toBe('nested');
      expect(listMock).toHaveBeenLastCalledWith(7, selection, 0, null);
    } finally {
      wrapper.unmount();
    }
  });

  it('uses the listed total when the directory has changed since the scan', async () => {
    listMock.mockResolvedValue({ ...page, totalBytes: 4096, nextOffset: null });
    const wrapper = mountDialog();
    try {
      await finishOpening();
      expect(wrapper.text()).toContain('4.10 KB');
      expect(wrapper.find('[role="alert"]').exists()).toBe(false);
      expect(wrapper.get('.other-name').text()).toBe('nested');
    } finally {
      wrapper.unmount();
    }
  });

  it('appends a page and leaves files read-only while allowing folder navigation', async () => {
    listMock.mockResolvedValueOnce(page).mockResolvedValueOnce({
      ...page,
      entries: [{ ...entry, name: 'small.bin', path: '/fixture/small.bin', isDirectory: false }],
      nextOffset: null,
    });
    const wrapper = mountDialog();
    try {
      await finishOpening();
      await wrapper.get('.other-status button').trigger('click');
      await flushPromises();
      expect(listMock).toHaveBeenLastCalledWith(7, selection, 1, page.snapshotId);
      expect(wrapper.findAll('.other-entry')).toHaveLength(2);
      expect(wrapper.findAll('.other-entry')[1]!.attributes('disabled')).toBeDefined();
      await wrapper.get('.other-entry').trigger('click');
      expect(wrapper.emitted('navigate')).toEqual([['/fixture/nested']]);
      expect(wrapper.emitted('close')).toHaveLength(1);
    } finally {
      wrapper.unmount();
    }
  });

  it('keeps rendered rows bounded as thousands of remainder entries are loaded', async () => {
    const all = Array.from({ length: 2000 }, (_, index) => ({
      ...entry,
      name: `file-${index}`,
      path: `/fixture/file-${index}`,
    }));
    listMock.mockImplementation((_scanId, _selection, offset: number) =>
      Promise.resolve({
        ...page,
        totalCount: all.length,
        entries: all.slice(offset, offset + 200),
        nextOffset: offset + 200 < all.length ? offset + 200 : null,
      })
    );
    const wrapper = mountDialog();
    try {
      await finishOpening();
      for (let index = 1; index < 10; index++) {
        await wrapper.get('.other-status button').trigger('click');
        await flushPromises();
      }
      expect(wrapper.text()).toContain(new Intl.NumberFormat().format(2000));
      expect(wrapper.findAll('.other-entry').length).toBeLessThanOrEqual(60);
      const viewport = wrapper.get('.other-body');
      for (const index of [1980, 0, 1000, 300]) {
        viewport.element.scrollTop = index * 36;
        await viewport.trigger('scroll');
        await flushPromises();
        expect(wrapper.text()).toContain(`file-${index}`);
        expect(wrapper.findAll('.other-entry').length).toBeLessThanOrEqual(60);
      }
    } finally {
      wrapper.unmount();
    }
  });

  it('releases the native listing when the dialog closes', async () => {
    listMock.mockResolvedValue({ ...page, nextOffset: null });
    const wrapper = mountDialog();
    try {
      await finishOpening();
      await wrapper.setProps({ selection: null });
      expect(releaseMock).toHaveBeenCalledExactlyOnceWith(page.snapshotId);
    } finally {
      wrapper.unmount();
    }
    expect(releaseMock).toHaveBeenCalledTimes(1);
  });

  it('releases a listing that finishes loading after the dialog closes', async () => {
    let finishRead!: (value: typeof page) => void;
    listMock.mockImplementationOnce(
      () =>
        new Promise(resolve => {
          finishRead = resolve;
        })
    );
    const wrapper = mountDialog();
    try {
      await finishOpening();
      await wrapper.setProps({ selection: null });
      finishRead(page);
      await flushPromises();
      expect(releaseMock).toHaveBeenCalledExactlyOnceWith(page.snapshotId);
      expect(wrapper.findAll('.other-entry')).toHaveLength(0);
    } finally {
      wrapper.unmount();
    }
  });

  it('replaces rows when an evicted listing restarts instead of mixing two snapshots', async () => {
    listMock.mockResolvedValueOnce(page).mockResolvedValueOnce({
      ...page,
      snapshotId: 102,
      entries: [{ ...entry, name: 'replacement', path: '/fixture/replacement' }],
      totalCount: 1,
      nextOffset: null,
    });
    const wrapper = mountDialog();
    try {
      await finishOpening();
      await wrapper.get('.other-status button').trigger('click');
      await flushPromises();
      expect(wrapper.findAll('.other-entry')).toHaveLength(1);
      expect(wrapper.get('.other-name').text()).toBe('replacement');
      expect(releaseMock).toHaveBeenCalledWith(101);
    } finally {
      wrapper.unmount();
    }
    expect(releaseMock).toHaveBeenCalledWith(102);
  });

  it('ignores an earlier scope response after opening another remainder', async () => {
    let resolveFirst!: (value: typeof page) => void;
    listMock
      .mockImplementationOnce(
        () =>
          new Promise(resolve => {
            resolveFirst = resolve;
          })
      )
      .mockResolvedValueOnce({ ...page, snapshotId: 102, entries: [{ ...entry, name: 'current' }], nextOffset: null });
    const wrapper = mountDialog();
    try {
      await finishOpening();
      await wrapper.setProps({ selection: { ...selection, parentPath: '/second' } });
      await finishOpening();
      resolveFirst(page);
      await flushPromises();
      expect(wrapper.get('.other-name').text()).toBe('current');
      expect(releaseMock).toHaveBeenCalledWith(101);
      expect(releaseMock).not.toHaveBeenCalledWith(102);
    } finally {
      wrapper.unmount();
    }
  });
});

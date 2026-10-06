// @vitest-environment happy-dom
import { flushPromises, mount } from '@vue/test-utils';
import { defineComponent, h, ref } from 'vue';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';
import { i18n } from '@/i18n';
import { FolderSelectionService } from '@/lib/services/folder-selection-service';
import MdStorageScopeSelect from './md-storage-scope-select.vue';

afterEach(() => vi.restoreAllMocks());

function mountSelector(multiple: boolean, protectionEnabled = false, initialProtectedPaths: string[] = []) {
  const selected = ref<string | string[]>(multiple ? [] : 'E:\\');
  const protectedPaths = ref<string[]>([...initialProtectedPaths]);
  const wrapper = mount(
    defineComponent({
      setup: () => () =>
        h(
          TooltipProvider,
          {},
          {
            default: () =>
              h(MdStorageScopeSelect, {
                multiple,
                modelValue: selected.value,
                disks: ['E:\\', 'F:\\'].map(mountPoint => ({
                  name: mountPoint,
                  mountPoint,
                  totalBytes: 100,
                  availableBytes: 50,
                  usedBytes: 50,
                })),
                recentFolders: ['E:\\Work', 'F:\\Chat'],
                protectionEnabled,
                protectedPaths: protectedPaths.value,
                'onUpdate:modelValue': value => {
                  selected.value = value;
                },
                'onUpdate:protectedPaths': value => {
                  protectedPaths.value = value;
                },
              }),
          }
        ),
    }),
    { attachTo: document.body, global: { plugins: [i18n], stubs: { MdNativeFileIcon: true } } }
  );
  async function open() {
    await wrapper.get('[role="combobox"]').trigger('keydown', { key: 'ArrowDown' });
    await flushPromises();
  }
  async function choose(text: string) {
    const option = [...document.querySelectorAll<HTMLElement>('[role="option"]')].find(
      el => el.textContent?.trim() === text
    );
    expect(option, `option ${text} should be present`).toBeDefined();
    option!.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    await flushPromises();
  }
  return { wrapper, selected, protectedPaths, open, choose };
}

describe('storage scope selection', () => {
  it('keeps multi-select open while adding disks and folders and allows deselecting all', async () => {
    const view = mountSelector(true);
    try {
      await view.open();
      await view.choose('E:\\');
      await view.choose('Chat');
      expect(view.selected.value).toEqual(['E:\\', 'F:\\Chat']);
      expect(document.querySelector('[role="listbox"]')).not.toBeNull();
      expect(view.wrapper.get('[role="combobox"]').text()).toContain('2 locations');
      await view.choose('E:\\');
      await view.choose('Chat');
      expect(view.selected.value).toEqual([]);
    } finally {
      view.wrapper.unmount();
    }
  });

  it('appends native folder selections, preserves selection on cancel, and never emits the action value', async () => {
    const select = vi
      .spyOn(FolderSelectionService, 'select')
      .mockResolvedValueOnce(['E:\\Work', 'F:\\Chat'])
      .mockResolvedValueOnce([]);
    vi.spyOn(FolderSelectionService, 'filterExistingDirectories').mockResolvedValue(['E:\\Work', 'F:\\Chat']);
    const view = mountSelector(true);
    try {
      await view.open();
      await view.choose('E:\\');
      await view.choose('Choose folder…');
      expect(select).toHaveBeenCalledWith(true, 'Choose folder…', 'E:\\');
      expect(view.selected.value).toEqual(['E:\\', 'E:\\Work', 'F:\\Chat']);
      expect(document.querySelector('[role="listbox"]')).toBeNull();
      await view.open();
      await view.choose('Choose folder…');
      expect(view.selected.value).toEqual(['E:\\', 'E:\\Work', 'F:\\Chat']);
    } finally {
      view.wrapper.unmount();
    }
  });

  it('preserves single-selection behavior on other pages', async () => {
    const view = mountSelector(false);
    try {
      await view.open();
      await view.choose('F:\\');
      expect(view.selected.value).toBe('F:\\');
      expect(document.querySelector('[role="listbox"]')).toBeNull();
    } finally {
      view.wrapper.unmount();
    }
  });

  it('toggles protection without changing the selected locations or closing the menu', async () => {
    const view = mountSelector(true, true);
    try {
      await view.open();
      await view.choose('Work');
      await view.choose('Chat');
      const cleanableHint =
        'Currently cleanable: duplicates can be selected for deletion. Click to keep scanning but prevent deletion';
      const protect = document.querySelector<HTMLButtonElement>(`button[aria-label="${cleanableHint}"]`);
      expect(protect).not.toBeNull();
      protect!.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, pointerType: 'mouse' }));
      await vi.waitFor(() => expect(document.querySelector('[role="tooltip"]')?.textContent).toContain(cleanableHint));
      protect!.click();
      await flushPromises();

      expect(view.selected.value).toEqual(['E:\\Work', 'F:\\Chat']);
      expect(view.protectedPaths.value).toEqual(['E:\\Work']);
      expect(document.querySelector('[role="listbox"]')).not.toBeNull();
      expect(view.wrapper.get('[role="combobox"]').text()).toContain('2 locations · 1 protected');
      const protectedHint =
        'Currently protected: files are still scanned but never deleted. Click to allow selection and deletion';
      const protectedToggle = document.querySelector<HTMLButtonElement>(`button[aria-label="${protectedHint}"]`);
      expect(protectedToggle).not.toBeNull();
    } finally {
      view.wrapper.unmount();
    }
  });

  it('explains the protected state on hover', async () => {
    const view = mountSelector(true, true, ['E:\\Work']);
    try {
      await view.open();
      await view.choose('Work');
      const protectedHint =
        'Currently protected: files are still scanned but never deleted. Click to allow selection and deletion';
      const protectedToggle = document.querySelector<HTMLButtonElement>(`button[aria-label="${protectedHint}"]`);
      expect(protectedToggle).not.toBeNull();
      protectedToggle!.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, pointerType: 'mouse' }));
      await vi.waitFor(() => expect(document.querySelector('[role="tooltip"]')?.textContent).toContain(protectedHint));
    } finally {
      view.wrapper.unmount();
    }
  });

  it('explains inherited protection while keeping the child action unavailable', async () => {
    const view = mountSelector(true, true);
    try {
      await view.open();
      await view.choose('E:\\');
      await view.choose('Work');
      const diskRow = [...document.querySelectorAll<HTMLElement>('.scope-history-option')].find(row =>
        row.textContent?.includes('E:\\')
      );
      const diskToggle = diskRow?.querySelector<HTMLButtonElement>('.scope-protection-toggle');
      expect(diskToggle).not.toBeNull();
      diskToggle!.click();
      await flushPromises();

      const inheritedHint =
        'Protected by a parent: files are still scanned but never deleted. Unprotect the parent first';
      const inheritedToggle = document.querySelector<HTMLButtonElement>(`button[aria-label="${inheritedHint}"]`);
      expect(inheritedToggle?.getAttribute('aria-disabled')).toBe('true');
      inheritedToggle!.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, pointerType: 'mouse' }));
      await vi.waitFor(() => expect(document.querySelector('[role="tooltip"]')?.textContent).toContain(inheritedHint));
    } finally {
      view.wrapper.unmount();
    }
  });
});

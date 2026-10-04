// @vitest-environment happy-dom

import { flushPromises, mount } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { defineComponent, ref } from 'vue';
import { TooltipProvider } from 'reka-ui';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import MdDialogContent from '@/components/custom/md-dialog-content.vue';
import { i18n } from '@/i18n';
import { NativeDragDropService } from '@/lib/services/native-drag-drop-service';
import { OperatingSystemService } from '@/lib/services/operating-system-service';
import { useCustomCleanupStore } from '@/stores/custom-cleanup-store';

import MdCleanupVolumeDialog from './md-cleanup-volume-dialog.vue';
import MdCustomCleanupDialog from './md-custom-cleanup-dialog.vue';

describe('volume capacity platform hint', () => {
  afterEach(() => vi.restoreAllMocks());

  it.each(['macos', 'linux', 'windows'] as const)(
    'shows the reclaimable-space explanation only on macOS: %s',
    platform => {
      vi.spyOn(OperatingSystemService, 'currentPlatform').mockReturnValue(platform);
      const wrapper = mount(MdCleanupVolumeDialog, {
        props: { modelValue: true, disks: [], initialMountPoints: [], systemDisk: null },
        global: {
          plugins: [i18n],
          stubs: {
            Dialog: { template: '<div><slot /></div>' },
            DialogTitle: { template: '<h2><slot /></h2>' },
            DialogDescription: { template: '<p><slot /></p>' },
            MdDialogContent: { template: '<div><slot /></div>' },
          },
        },
      });
      const hint = i18n.global.t('systemStatus.diskCapacityHint');
      if (platform === 'macos') expect(wrapper.text()).toContain(hint);
      else expect(wrapper.text()).not.toContain(hint);
      expect(wrapper.text()).toContain(i18n.global.t('cleanup.scanMode.volumeDialogDescription'));
      wrapper.unmount();
    }
  );
});

describe.each(['volumes', 'custom'] as const)('%s cleanup dialog dismissal', kind => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.spyOn(OperatingSystemService, 'currentPlatform').mockReturnValue('macos');
    vi.spyOn(NativeDragDropService, 'listen').mockResolvedValue(() => undefined);
    const store = useCustomCleanupStore();
    store.initialized = true;
    store.rules = [];
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it.each(['cancel', 'close', 'escape'] as const)(
    'preserves edits after backdrop clicks and still supports %s',
    async action => {
      const wrapper = mount(
        defineComponent({
          components: { MdCleanupVolumeDialog, MdCustomCleanupDialog, TooltipProvider },
          setup: () => ({
            custom: kind === 'custom',
            open: ref(false),
            disks: [{ name: 'Test disk', mountPoint: '/fixture', totalBytes: 100, availableBytes: 50, usedBytes: 50 }],
          }),
          template: `<TooltipProvider>
            <MdCustomCleanupDialog v-if="custom" v-model="open" />
            <MdCleanupVolumeDialog v-else v-model="open" :disks="disks" :initial-mount-points="[]" :system-disk="null" />
          </TooltipProvider>`,
        }),
        { attachTo: document.body, global: { plugins: [i18n] } }
      );
      try {
        wrapper.vm.open = true;
        await flushPromises();
        // Reka installs its outside-pointer listener on the next task so the
        // pointer event that opened a dialog cannot immediately dismiss it.
        await new Promise(resolve => setTimeout(resolve, 0));
        const content = wrapper.findComponent(MdDialogContent);
        const dialog = document.querySelector<HTMLElement>('[data-slot="dialog-content"]')!;
        const overlay = document.querySelector<HTMLElement>('[data-slot="dialog-overlay"]')!;
        const input = dialog.querySelector<HTMLInputElement>('input[id^="custom-rule-name-"]');
        const checkbox = dialog.querySelector<HTMLElement>('#cleanup-volume-0');
        if (kind === 'custom') {
          expect(input).not.toBeNull();
          input!.value = 'Unsaved test rule';
          input!.dispatchEvent(new Event('input', { bubbles: true }));
        } else {
          expect(checkbox).not.toBeNull();
          checkbox!.click();
        }
        await flushPromises();

        for (const pointerType of ['mouse', 'touch']) {
          overlay.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, pointerType, button: 0 }));
          overlay.click();
          await flushPromises();
          expect(wrapper.vm.open).toBe(true);
        }
        // Assert the real primitive handled both events, not merely that a
        // stub or an unregistered listener left the model unchanged.
        expect(content.emitted('pointerDownOutside')).toHaveLength(2);
        if (kind === 'custom') expect(input!.value).toBe('Unsaved test rule');
        else expect(checkbox!.getAttribute('aria-checked')).toBe('true');

        if (action === 'escape') {
          dialog.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
        } else {
          const label = i18n.global.t(action === 'cancel' ? 'common.cancel' : 'common.close');
          const button = [...dialog.querySelectorAll('button')].find(element => element.textContent?.trim() === label);
          expect(button).toBeDefined();
          button!.click();
        }
        await flushPromises();
        expect(wrapper.vm.open).toBe(false);
      } finally {
        wrapper.unmount();
      }
    }
  );
});

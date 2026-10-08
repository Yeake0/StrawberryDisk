// @vitest-environment happy-dom

import { flushPromises, mount } from '@vue/test-utils';
import { afterEach, describe, expect, it } from 'vitest';

import { i18n } from '@/i18n';
import { ICON_NAMES } from '@/lib/models/ui';

import MdDialogContent from './md-dialog-content.vue';
import MdOperationDialog from './md-operation-dialog.vue';

const wrappers: ReturnType<typeof mount>[] = [];
afterEach(() => {
  for (const wrapper of wrappers.splice(0)) wrapper.unmount();
});

async function mountOperation(cancelable = true) {
  const wrapper = mount(MdOperationDialog, {
    attachTo: document.body,
    props: {
      open: true,
      title: 'Active operation',
      description: 'Processing fixture items',
      iconName: ICON_NAMES.deepCleanup,
      progressLabel: 'Operation progress',
      progress: 42,
      stats: [{ key: 'items', label: 'Items', value: '21 / 50' }],
      cancelable,
      cancelLabel: 'Request cancellation',
    },
    global: { plugins: [i18n] },
  });
  wrappers.push(wrapper);
  await flushPromises();
  // Outside-pointer listeners are installed after the opening task.
  await new Promise(resolve => setTimeout(resolve, 0));
  return wrapper;
}

describe('operation dialog', () => {
  it('keeps active work visible after Escape and mouse or touch backdrop clicks', async () => {
    const wrapper = await mountOperation();
    const content = wrapper.findComponent(MdDialogContent);
    const dialog = document.querySelector<HTMLElement>('[data-slot="dialog-content"]')!;
    const overlay = document.querySelector<HTMLElement>('[data-slot="dialog-overlay"]')!;

    dialog.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    for (const pointerType of ['mouse', 'touch']) {
      overlay.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, pointerType, button: 0 }));
      overlay.click();
      await flushPromises();
    }

    expect(content.emitted('escapeKeyDown')).toHaveLength(1);
    expect(content.emitted('interactOutside')).toHaveLength(2);
    expect(document.querySelector('[role="dialog"]')).not.toBeNull();
    expect(wrapper.emitted('cancel')).toBeUndefined();
    expect(dialog.querySelector('[data-slot="dialog-close"]')).toBeNull();
    expect(dialog.querySelectorAll('button')).toHaveLength(1);
  });

  it('limits dragging to the header and preserves reactive progress and cancellation controls', async () => {
    const wrapper = await mountOperation();
    const dialog = document.querySelector<HTMLElement>('[data-slot="dialog-content"]')!;
    const header = dialog.querySelector('[data-slot="dialog-header"]')!;
    const overlay = document.querySelector('[data-slot="dialog-overlay"]')!;
    expect(header.querySelector('[data-tauri-drag-region][aria-hidden="true"]')).not.toBeNull();
    expect(overlay.hasAttribute('data-tauri-drag-region')).toBe(false);
    expect(dialog.hasAttribute('data-tauri-drag-region')).toBe(false);
    expect(dialog.querySelector('[role="progressbar"]')?.getAttribute('aria-valuenow')).toBe('42');
    expect(dialog.textContent).toContain('21 / 50');

    const cancel = dialog.querySelector('button')!;
    cancel.click();
    expect(wrapper.emitted('cancel')).toHaveLength(1);
    await wrapper.setProps({ cancelDisabled: true, cancelLabel: 'Cancelling', progress: 65, title: 'Cancelling work' });
    cancel.click();
    expect(wrapper.emitted('cancel')).toHaveLength(1);
    expect(cancel.disabled).toBe(true);
    expect(cancel.textContent).toContain('Cancelling');
    expect(dialog.querySelector('[data-slot="dialog-title"]')?.textContent).toBe('Cancelling work');
    expect(dialog.querySelector('[role="progressbar"]')?.getAttribute('aria-valuenow')).toBe('65');

    await wrapper.setProps({ open: false });
    await flushPromises();
    expect(document.querySelector('[role="dialog"]')).toBeNull();
    expect(document.body.style.pointerEvents).not.toBe('none');
  });

  it('does not imply measured progress or expose cancellation for a preview', async () => {
    const wrapper = await mountOperation(false);
    await wrapper.setProps({ progress: undefined, stats: [] });
    const dialog = document.querySelector<HTMLElement>('[data-slot="dialog-content"]')!;
    expect(dialog.querySelector('[role="progressbar"]')?.hasAttribute('aria-valuenow')).toBe(false);
    expect(dialog.querySelector('.operation-dialog-stats')).toBeNull();
    expect(dialog.querySelector('button')).toBeNull();
  });
});

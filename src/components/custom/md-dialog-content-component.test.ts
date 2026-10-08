// @vitest-environment happy-dom

import { mount } from '@vue/test-utils';
import { defineComponent, nextTick } from 'vue';
import { describe, expect, it } from 'vitest';

import { i18n } from '@/i18n';
import { Dialog as UiDialog, DialogDescription, DialogTitle } from '@/components/ui/dialog';

import MdDialogContent from './md-dialog-content.vue';
import MdDialogHeader from './md-dialog-header.vue';
import dialogContentSource from './md-dialog-content.vue?raw';

describe('dialog content component', () => {
  it('applies the bounded-height contract to the teleported dialog node', async () => {
    const wrapper = mount(
      defineComponent({
        components: { DialogDescription, DialogTitle, MdDialogContent, MdDialogHeader, UiDialog },
        template:
          '<UiDialog :open="true"><MdDialogContent height="tall"><MdDialogHeader><DialogTitle>Title</DialogTitle><DialogDescription>Description</DialogDescription></MdDialogHeader><div>Content</div></MdDialogContent></UiDialog>',
      }),
      {
        attachTo: document.body,
        global: { plugins: [i18n] },
      }
    );
    await nextTick();

    const content = document.body.querySelector<HTMLElement>('[data-slot="dialog-content"]');
    const header = document.body.querySelector<HTMLElement>('[data-slot="dialog-header"]');
    expect(content).not.toBeNull();
    expect(content?.classList.contains('md-dialog-content--tall')).toBe(true);
    expect(header?.hasAttribute('data-tauri-drag-region')).toBe(true);
    expect(header?.querySelector('[aria-hidden="true"]')?.hasAttribute('data-tauri-drag-region')).toBe(true);
    expect(dialogContentSource).toContain('<style>');
    expect(dialogContentSource).not.toContain('<style scoped>');
    wrapper.unmount();
  });
});

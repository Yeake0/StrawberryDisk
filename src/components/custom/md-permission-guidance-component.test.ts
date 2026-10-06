// @vitest-environment happy-dom

import { mount } from '@vue/test-utils';
import { describe, expect, it, vi } from 'vitest';

import MdPermissionGuidance from './md-permission-guidance.vue';

const dialogStub = {
  props: ['open'],
  emits: ['update:open'],
  template: '<div v-if="open" class="dialog-stub"><slot /></div>',
};
const passthroughStub = { template: '<div><slot /></div>' };
const buttonStub = {
  props: ['disabled'],
  template: '<button type="button" :disabled="disabled"><slot /></button>',
};
const iconStub = { template: '<span class="icon-stub" />' };

function mountGuidance(openSettings = vi.fn<() => Promise<boolean>>().mockResolvedValue(true)) {
  return mount(MdPermissionGuidance, {
    props: {
      modelValue: true,
      summary: 'Permission required',
      title: 'Some data is hidden',
      description: 'Enable permission to scan more data',
      instructions: 'Enable the permission in System Settings',
      skipLabel: 'Not now',
      openSettingsLabel: 'Open System Settings',
      openSettings,
    },
    global: {
      stubs: {
        Button: buttonStub,
        Dialog: dialogStub,
        DialogDescription: passthroughStub,
        DialogTitle: passthroughStub,
        MdDialogContent: passthroughStub,
        MdDialogFooter: passthroughStub,
        MdDialogHeader: passthroughStub,
        MdIcon: iconStub,
        MdInlineNotice: passthroughStub,
      },
    },
  });
}

describe('permission guidance component', () => {
  it('reopens the explanation from the summary before offering System Settings', async () => {
    const openSettings = vi.fn<() => Promise<boolean>>().mockResolvedValue(true);
    const wrapper = mountGuidance(openSettings);
    await wrapper.setProps({ modelValue: false });

    await wrapper.get('.permission-summary').trigger('click');

    expect(openSettings).not.toHaveBeenCalled();
    expect(wrapper.get('.permission-summary').attributes('aria-haspopup')).toBe('dialog');
    expect(wrapper.emitted('update:modelValue')?.at(-1)).toEqual([true]);
  });

  it('closes the dialog after System Settings opens successfully', async () => {
    const wrapper = mountGuidance();
    const openButton = wrapper.findAll('.dialog-stub button').find(button => button.text().includes('Open'))!;

    await openButton.trigger('click');

    expect(wrapper.emitted('update:modelValue')?.at(-1)).toEqual([false]);
  });

  it('keeps the explanation open when System Settings cannot be opened', async () => {
    const wrapper = mountGuidance(vi.fn<() => Promise<boolean>>().mockResolvedValue(false));
    const openButton = wrapper.findAll('.dialog-stub button').find(button => button.text().includes('Open'))!;

    await openButton.trigger('click');

    expect(wrapper.emitted('update:modelValue')).toBeUndefined();
    expect(wrapper.find('.dialog-stub').exists()).toBe(true);
  });
});

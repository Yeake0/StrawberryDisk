// @vitest-environment happy-dom

import { mount } from '@vue/test-utils';
import { describe, expect, it } from 'vitest';

import MdSplitActionButton from './md-split-action-button.vue';

const passthroughStub = { template: '<div><slot /></div>' };
const itemStub = {
  emits: ['select'],
  template: '<button type="button" @click="$emit(\'select\')"><slot /></button>',
};

function mountButton(items = [{ value: 'quick', icon: 'search' as const, label: 'Quick', description: 'Fast scan' }]) {
  return mount(MdSplitActionButton, {
    props: {
      accessibleLabel: 'Choose scan mode',
      primaryIcon: 'scan',
      primaryLabel: 'Start scan',
      items,
    },
    global: {
      stubs: {
        DropdownMenuContent: passthroughStub,
        DropdownMenuItem: itemStub,
        DropdownMenuPortal: passthroughStub,
        DropdownMenuRoot: passthroughStub,
        DropdownMenuTrigger: passthroughStub,
        MdIcon: true,
      },
    },
  });
}

describe('split action button', () => {
  it('keeps the primary action separate from menu choices', async () => {
    const wrapper = mountButton();

    await wrapper.get('.md-split-action__primary').trigger('click');
    await wrapper.get('.md-split-action__item').trigger('click');

    expect(wrapper.emitted('primary')).toHaveLength(1);
    expect(wrapper.emitted('select')).toEqual([['quick']]);
  });

  it('uses one joined divider for every split variant', () => {
    const wrapper = mountButton();

    expect(wrapper.get('.md-split-action').classes()).toContain('md-split-action--joined');
    expect(wrapper.get('.md-split-action__menu').classes()).toContain('rounded-l-none');
  });

  it('renders a normal action when no menu choices are available', () => {
    const wrapper = mountButton([]);

    expect(wrapper.find('.md-split-action__menu').exists()).toBe(false);
    expect(wrapper.get('.md-split-action__primary').classes()).not.toContain('rounded-r-none');
  });
});

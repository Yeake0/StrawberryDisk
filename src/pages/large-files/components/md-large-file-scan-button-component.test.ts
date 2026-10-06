// @vitest-environment happy-dom

import { mount } from '@vue/test-utils';
import { describe, expect, it } from 'vitest';

import { i18n } from '@/i18n';

import MdLargeFileScanButton from './md-large-file-scan-button.vue';

const passthroughStub = { template: '<div><slot /></div>' };
const itemStub = {
  emits: ['select'],
  template: '<button type="button" @click="$emit(\'select\')"><slot /></button>',
};

function mountButton(selectableModes: boolean) {
  return mount(MdLargeFileScanButton, {
    props: {
      busy: false,
      mode: 'quick',
      selectableModes,
    },
    global: {
      plugins: [i18n],
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

describe('large-file scan button', () => {
  it('starts the current mode from the primary action', async () => {
    const wrapper = mountButton(true);

    await wrapper.get('.md-split-action__primary').trigger('click');

    expect(wrapper.emitted('scan')).toEqual([['quick']]);
  });

  it('offers both macOS scan modes with concise explanations', async () => {
    const wrapper = mountButton(true);
    const items = wrapper.findAll('.md-split-action__item');

    expect(items).toHaveLength(2);
    expect(items[0]?.text()).toContain('Quick');
    expect(items[1]?.text()).toContain('Complete');

    await items[1]?.trigger('click');
    expect(wrapper.emitted('scan')?.at(-1)).toEqual(['complete']);
  });

  it('keeps Windows on one authoritative scan action', () => {
    const wrapper = mountButton(false);

    expect(wrapper.find('.md-split-action__menu').exists()).toBe(false);
    expect(wrapper.findAll('.md-split-action__item')).toHaveLength(0);
  });
});

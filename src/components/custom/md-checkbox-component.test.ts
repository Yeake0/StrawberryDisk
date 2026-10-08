// @vitest-environment happy-dom

import { mount } from '@vue/test-utils';
import { ref } from 'vue';
import { describe, expect, it } from 'vitest';

import MdCheckbox from './md-checkbox.vue';

describe('shared checkbox', () => {
  it('toggles through its associated label', async () => {
    const wrapper = mount(
      {
        components: { MdCheckbox },
        setup() {
          return { checked: ref(false) };
        },
        template:
          '<label for="shared-checkbox">Select item</label><MdCheckbox id="shared-checkbox" v-model="checked" />',
      },
      { attachTo: document.body }
    );

    wrapper.get('label').element.click();
    await wrapper.vm.$nextTick();

    expect(wrapper.get('[role="checkbox"]').attributes('aria-checked')).toBe('true');
    wrapper.unmount();
  });

  it('toggles once through a wrapping label', async () => {
    const wrapper = mount(
      {
        components: { MdCheckbox },
        setup() {
          return { checked: ref(false) };
        },
        template: '<label><MdCheckbox v-model="checked" />Select item</label>',
      },
      { attachTo: document.body }
    );

    wrapper.get('label').element.click();
    await wrapper.vm.$nextTick();

    expect(wrapper.get('[role="checkbox"]').attributes('aria-checked')).toBe('true');
    wrapper.unmount();
  });

  it('changes a partial selection to checked and ignores clicks while disabled', async () => {
    const wrapper = mount(MdCheckbox, { props: { modelValue: 'indeterminate' } });
    const checkbox = wrapper.get('[role="checkbox"]');

    expect(checkbox.attributes('aria-checked')).toBe('mixed');
    await checkbox.trigger('click');
    expect(wrapper.emitted('update:modelValue')).toEqual([[true]]);

    await wrapper.setProps({ modelValue: false, disabled: true });
    await checkbox.trigger('click');
    expect(wrapper.emitted('update:modelValue')).toEqual([[true]]);
  });
});

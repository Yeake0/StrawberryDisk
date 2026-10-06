// @vitest-environment happy-dom

import { mount } from '@vue/test-utils';
import { describe, expect, it } from 'vitest';

import MdSwitch from './md-switch.vue';

describe('shared switch', () => {
  it('emits a boolean change when activated', async () => {
    const wrapper = mount(MdSwitch, { props: { modelValue: false } });

    await wrapper.get('[role="switch"]').trigger('click');

    expect(wrapper.emitted('update:modelValue')).toEqual([[true]]);
    wrapper.unmount();
  });

  it('keeps its state and shows progress while an operation is pending', async () => {
    const wrapper = mount(MdSwitch, { props: { modelValue: true, loading: true } });
    const control = wrapper.get('[role="switch"]');

    expect(control.attributes('aria-checked')).toBe('true');
    expect(control.attributes('aria-busy')).toBe('true');
    expect(control.attributes('disabled')).toBeDefined();
    expect(wrapper.find('.md-switch-spinner').exists()).toBe(true);
    await control.trigger('click');
    expect(wrapper.emitted('update:modelValue')).toBeUndefined();
    wrapper.unmount();
  });
});

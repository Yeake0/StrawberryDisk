// @vitest-environment happy-dom
import { flushPromises, mount } from '@vue/test-utils';
import { h } from 'vue';
import { expect, it, vi } from 'vitest';
import MdTooltip from './md-tooltip.vue';

it('preserves a standalone trigger and shows a dismissible hint without a native title', async () => {
  const clicked = vi.fn();
  const wrapper = mount(MdTooltip, {
    attachTo: document.body,
    props: { text: 'C:\\Applications\\Example.exe' },
    attrs: { class: 'caller-class', 'aria-label': 'Open location' },
    slots: { default: () => h('button', { onClick: clicked }, 'Open') },
  });
  try {
    const button = wrapper.get('button');
    expect(Array.from(wrapper.element.children)).toEqual([button.element]);
    expect(button.classes()).toContain('caller-class');
    expect(button.attributes('aria-label')).toBe('Open location');
    expect(button.attributes('title')).toBeUndefined();
    await button.trigger('pointermove', { pointerType: 'mouse' });
    await vi.waitFor(() => expect(document.querySelector('[role="tooltip"]')?.textContent).toContain('Example.exe'));
    await button.trigger('click');
    expect(clicked).toHaveBeenCalledTimes(1);
    await flushPromises();
    await vi.waitFor(() => expect(document.querySelector('[role="tooltip"]')).toBeNull());
    await wrapper.setProps({ text: '' });
    expect(wrapper.get('button').element).toBe(button.element);
  } finally {
    wrapper.unmount();
  }
});

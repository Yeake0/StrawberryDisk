// @vitest-environment happy-dom

import { mount } from '@vue/test-utils';
import { describe, expect, it, vi } from 'vitest';

import { i18n } from '@/i18n';
import MdFileEntryContextMenu from './md-file-entry-context-menu.vue';

describe('file entry context menu', () => {
  it('closes an open menu before a recycled row targets a different file', async () => {
    const menuStateChange = vi.fn();
    const wrapper = mount(MdFileEntryContextMenu, {
      props: { entryKey: '/files/first', onMenuStateChange: menuStateChange },
      slots: { default: '<span>File</span>' },
      global: {
        plugins: [i18n],
        stubs: {
          ContextMenu: {
            props: ['open'],
            template:
              '<div :data-open="open"><button @click="$emit(\'update:open\', true)">Open</button><slot /></div>',
          },
          ContextMenuTrigger: { template: '<div><slot /></div>' },
          ContextMenuContent: true,
        },
      },
    });
    await wrapper.get('button').trigger('click');
    expect(wrapper.get('[data-open]').attributes('data-open')).toBe('true');
    await wrapper.setProps({ entryKey: '/files/second' });
    expect(wrapper.get('[data-open]').attributes('data-open')).toBe('false');
    expect(wrapper.emitted('menuStateChange')).toEqual([[true], [false]]);
    await wrapper.get('button').trigger('click');
    wrapper.unmount();
    expect(menuStateChange.mock.calls).toEqual([[true], [false], [true], [false]]);
  });
});

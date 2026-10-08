// @vitest-environment happy-dom

import { mount } from '@vue/test-utils';
import { describe, expect, it } from 'vitest';

import { ICON_NAMES } from '@/lib/models/ui';

import MdResultMasterDetail from './md-result-master-detail.vue';
import MdResultCategoryItem from './md-result-category-item.vue';
import MdResultDetailHeader from './md-result-detail-header.vue';

const iconStub = { template: '<span class="icon-stub" />' };
const passthroughStub = { template: '<span><slot /></span>' };
const checkboxStub = {
  props: ['checked', 'indeterminate', 'disabled'],
  emits: ['update:checked'],
  template: '<button class="checkbox-stub" @click="$emit(\'update:checked\', !checked)" />',
};

describe('result navigation components', () => {
  it('keeps navigation and detail content in the shared master-detail layout', () => {
    const wrapper = mount(MdResultMasterDetail, {
      props: { navigationLabel: 'Cleanup categories' },
      slots: {
        navigation: '<button class="fixture-navigation">Browser activity</button>',
        default: '<section class="fixture-details">Details</section>',
      },
    });

    expect(wrapper.get('.result-master-detail-navigation').attributes('aria-label')).toBe('Cleanup categories');
    expect(wrapper.get('.fixture-navigation').text()).toBe('Browser activity');
    expect(wrapper.get('.fixture-details').text()).toBe('Details');
  });

  it('renders and selects the shared category item', async () => {
    const wrapper = mount(MdResultCategoryItem, {
      props: {
        active: true,
        title: 'Browser activity',
        description: '3 items · 592 traces',
        iconName: ICON_NAMES.globe,
        selectedSummary: '12',
        selectedAriaLabel: '12 selected',
      },
      global: { stubs: { MdIcon: iconStub } },
    });

    expect(wrapper.get('button').attributes('aria-current')).toBe('page');
    expect(wrapper.text()).toContain('3 items · 592 traces');
    expect(wrapper.get('.result-category-selected').attributes('aria-label')).toBe('12 selected');

    await wrapper.get('button').trigger('click');
    expect(wrapper.emitted('select')).toHaveLength(1);
  });

  it('emits the shared detail selection while preserving the metric slot', async () => {
    const wrapper = mount(MdResultDetailHeader, {
      props: {
        title: 'Browser activity',
        description: 'Browsing records',
        selection: 'partial',
        selectLabel: 'Select category',
      },
      slots: { metric: '<strong>12</strong><i>/ 592</i>' },
      global: {
        stubs: {
          MdIcon: iconStub,
          MdIconAction: passthroughStub,
          MdResultCheckbox: checkboxStub,
        },
      },
    });

    expect(wrapper.get('.result-detail-metric').text()).toBe('12/ 592');
    await wrapper.get('.checkbox-stub').trigger('click');
    expect(wrapper.emitted('update:selected')?.at(-1)).toEqual([true]);
  });
});

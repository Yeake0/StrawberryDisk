// @vitest-environment happy-dom

import { mount } from '@vue/test-utils';
import { describe, expect, it } from 'vitest';

import MdSelectionActionBar from './md-selection-action-bar.vue';

const buttonStub = {
  template: '<button type="button"><slot /></button>',
};

describe('selection action bar component', () => {
  it('reserves the intrinsic width of long localized options before distributing free space', () => {
    const wrapper = mount(MdSelectionActionBar, {
      props: {
        selectedLabel: 'Selected items',
        selectedValue: '58 items',
        spaceLabel: 'Estimated space to free',
        spaceValue: '56.4 GB',
        actionLabel: 'Clean',
      },
      slots: {
        options: '<div class="localized-option">Selection · Smart recommendation</div>',
      },
      global: { stubs: { Button: buttonStub, MdActionBarContainer: false } },
    });

    const options = wrapper.get('.localized-option').element.parentElement;
    expect(options?.classList.contains('flex-auto')).toBe(true);
    expect(options?.classList.contains('flex-1')).toBe(false);
  });
});

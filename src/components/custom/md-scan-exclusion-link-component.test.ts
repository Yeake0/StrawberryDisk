// @vitest-environment happy-dom

import { mount } from '@vue/test-utils';
import { afterEach, describe, expect, it } from 'vitest';

import { i18n } from '@/i18n';
import { LANGUAGE_IDS } from '@/lib/models/settings';

import MdScanExclusionLink from './md-scan-exclusion-link.vue';

const initialLocale = i18n.global.locale.value;

afterEach(() => {
  i18n.global.locale.value = initialLocale;
});

describe('scan exclusion link', () => {
  it.each(Object.values(LANGUAGE_IDS))('uses the shared label in %s', async locale => {
    i18n.global.locale.value = locale;
    const wrapper = mount(MdScanExclusionLink, {
      props: { hint: 'Scan snapshot hint' },
      global: {
        plugins: [i18n],
        stubs: { MdIcon: true, MdTooltip: { template: '<span><slot /></span>' } },
      },
    });

    const button = wrapper.get('button');
    expect(button.text()).toBe(i18n.global.t('storageScanExclusions.activeLabel'));
    await button.trigger('click');
    expect(wrapper.emitted('open')).toHaveLength(1);
    wrapper.unmount();
  });
});

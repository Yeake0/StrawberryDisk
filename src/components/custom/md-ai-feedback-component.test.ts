// @vitest-environment jsdom
import { mount } from '@vue/test-utils';
import { expect, it } from 'vitest';
import { i18n } from '@/i18n';
import MdAiFeedback from './md-ai-feedback.vue';

it('exposes two direct accessible controls and preserves their labels while saving', async () => {
  const feedback = {
    schemaVersion: 1 as const,
    requestId: '2c7fa490-6ab3-4c3f-b4b4-d6cb6e3d135f',
    rating: null,
    busy: false,
    error: null,
  };
  const wrapper = mount(MdAiFeedback, { props: { feedback }, global: { plugins: [i18n] } });
  const buttons = wrapper.findAll('button');
  expect(buttons).toHaveLength(2);
  const labels = buttons.map(button => button.text());
  await buttons[0]!.trigger('click');
  expect(wrapper.emitted('rate')).toEqual([['positive']]);
  await wrapper.setProps({ feedback: { ...feedback, rating: 'positive', busy: true } });
  expect(wrapper.findAll('button').map(button => button.text())).toEqual(labels);
  expect(wrapper.get('[aria-pressed="true"]').attributes('disabled')).toBeDefined();
  expect(wrapper.find('[role="menu"]').exists()).toBe(false);
  await wrapper.setProps({ feedback: { ...feedback, rating: 'positive', error: 'failed' } });
  expect(wrapper.find('[role="alert"]').exists()).toBe(true);
  expect(wrapper.get('[aria-pressed="true"]').attributes('disabled')).toBeUndefined();
  await wrapper.setProps({ feedback: { ...feedback, error: 'expired' } });
  expect(wrapper.findAll('button').every(button => button.attributes('disabled') !== undefined)).toBe(true);
  wrapper.unmount();
});

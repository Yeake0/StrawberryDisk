// @vitest-environment happy-dom

import { mount } from '@vue/test-utils';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { i18n } from '@/i18n';
import type { CleanupReadFailureDetail } from '@/lib/models/cleanup';
import { LANGUAGE_IDS } from '@/lib/models/settings';
import { ICON_NAMES } from '@/lib/models/ui';
import { ClipboardService } from '@/lib/services/clipboard-service';
import MdSwitch from '@/components/custom/md-switch.vue';

import MdIncompleteScanGuidance from './md-incomplete-scan-guidance.vue';

const passthroughStub = { template: '<div><slot /></div>' };
const dialogStub = {
  props: ['open'],
  emits: ['update:open'],
  template: '<div v-if="open" class="dialog-stub"><slot /></div>',
};
const buttonStub = {
  props: ['disabled'],
  template: '<button type="button" :disabled="disabled"><slot /></button>',
};

function mountGuidance(failureDetails: CleanupReadFailureDetail[] = []) {
  const wrapper = mount(MdIncompleteScanGuidance, {
    props: {
      modelValue: false,
      failureCount: 2,
      failureDetails,
      retryDisabled: false,
    },
    global: {
      plugins: [i18n],
      stubs: {
        Button: buttonStub,
        Dialog: dialogStub,
        DialogDescription: passthroughStub,
        DialogTitle: passthroughStub,
        MdDialogContent: passthroughStub,
        MdDialogFooter: passthroughStub,
        MdDialogHeader: passthroughStub,
        MdIcon: { props: ['name'], template: '<span :data-icon="name" />' },
        MdTooltip: { props: ['text'], template: '<span><slot /></span>' },
        MdSwitch: true,
      },
    },
  });
  return { wrapper };
}

function failureDetail(path: string): CleanupReadFailureDetail {
  return {
    path,
    stage: 'openDirectory',
    reason: 'permissionDenied',
    osError: 13,
    error: 'Permission denied (os error 13)',
    privacyRestrictionPossible: false,
  };
}

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
  i18n.global.locale.value = LANGUAGE_IDS.enUS;
});

describe('incomplete cleanup scan guidance', () => {
  it('opens details from the summary and offers a retry', async () => {
    const { wrapper } = mountGuidance();

    await wrapper.get('.scan-warning-trigger').trigger('click');
    expect(wrapper.emitted('update:modelValue')?.at(-1)).toEqual([true]);
    await wrapper.setProps({ modelValue: true });
    expect(wrapper.get('.dialog-stub').text()).toContain(
      i18n.global.t('cleanup.permission.otherDescription', { count: 2 })
    );
    expect(wrapper.get('.dialog-stub').text()).toContain(i18n.global.t('cleanup.permission.detailsUnavailable'));
    expect(wrapper.get('.dialog-stub').text()).not.toContain(i18n.global.t('fullDiskAccessGuidance.openSettings'));

    const buttons = wrapper.findAll('.dialog-stub button');
    expect(buttons[0]?.text()).toBe(i18n.global.t('settings.feedbackDialog.openLogFolder'));
    await buttons[0]!.trigger('click');
    expect(wrapper.emitted('openLogs')).toHaveLength(1);
    expect(wrapper.emitted('update:modelValue')?.at(-1)).toEqual([true]);

    await buttons[1]!.trigger('click');
    expect(wrapper.emitted('retry')).toHaveLength(1);
    expect(wrapper.emitted('update:modelValue')?.at(-1)).toEqual([false]);
    wrapper.unmount();
  });

  it('keeps retry disabled while another operation is running', async () => {
    const { wrapper } = mountGuidance();
    await wrapper.setProps({ modelValue: true, retryDisabled: true });
    const retryButton = wrapper.findAll('.dialog-stub button')[1]!;
    expect(retryButton.attributes('disabled')).toBeDefined();
    await retryButton.trigger('click');
    expect(wrapper.emitted('retry')).toBeUndefined();
    wrapper.unmount();
  });

  it('offers a read-failure alert choice without dismissing the current dialog', async () => {
    const { wrapper } = mountGuidance();
    await wrapper.setProps({ modelValue: true });
    const control = wrapper.getComponent(MdSwitch);
    expect(control.props('modelValue')).toBe(false);
    control.vm.$emit('update:modelValue', true);
    expect(wrapper.emitted('update:hideReadFailureAlerts')).toEqual([[true]]);
    expect(wrapper.emitted('update:modelValue')).toBeUndefined();
    await wrapper.setProps({ hideReadFailureAlerts: true });
    expect(control.props('modelValue')).toBe(true);
    wrapper.unmount();
  });

  it.each(Object.values(LANGUAGE_IDS))('shows paths, native errors and failure reasons in %s', async locale => {
    i18n.global.locale.value = locale;
    const { wrapper } = mountGuidance([failureDetail('/fixture/cache')]);
    await wrapper.setProps({ modelValue: true, failureCount: 1 });
    expect(wrapper.get('.scan-failure-path').text()).toBe('/fixture/cache');
    expect(wrapper.get('summary').text()).toBe(i18n.global.t('cleanup.permission.reasons.permissionDenied'));
    expect(wrapper.get('.scan-failure-native-error').text()).toBe('Permission denied (os error 13)');
    expect(wrapper.find('.scan-failure-list').attributes('tabindex')).toBe('0');
    expect(wrapper.get('details').attributes('open')).toBeUndefined();
    expect(wrapper.text()).not.toContain(i18n.global.t('cleanup.permission.detailsUnavailable'));
    wrapper.unmount();
  });

  it('shows at most 50 records while retaining the full failure total', async () => {
    const { wrapper } = mountGuidance(Array.from({ length: 80 }, (_, index) => failureDetail(`/fixture/${index}`)));
    await wrapper.setProps({ modelValue: true, failureCount: 80 });
    expect(wrapper.findAll('.scan-failure-row')).toHaveLength(50);
    expect(wrapper.text()).toContain(i18n.global.t('cleanup.permission.detailsShown', { count: 50, total: 80 }));
    expect(wrapper.findAll('.scan-failure-path').at(-1)?.text()).toBe('/fixture/49');
    wrapper.unmount();
  });

  it('copies the complete original path and escapes control characters only for display', async () => {
    const path = '/fixture/cache\nwith-control';
    const copy = vi.spyOn(ClipboardService, 'writeText').mockResolvedValue();
    const { wrapper } = mountGuidance([failureDetail(path)]);
    await wrapper.setProps({ modelValue: true });
    expect(wrapper.get('.scan-failure-path').text()).toBe('/fixture/cache\\u000awith-control');
    await wrapper.get('.scan-failure-path-row button').trigger('click');
    expect(copy).toHaveBeenCalledWith(path);
    await wrapper.setProps({ modelValue: false });
    await wrapper.setProps({ modelValue: true });
    expect(wrapper.get('.scan-failure-path-row button').attributes('aria-label')).toBe(
      i18n.global.t('cleanup.permission.copyPath', { path })
    );
    wrapper.unmount();
  });

  it('reports clipboard failures without dismissing the dialog', async () => {
    const error = new Error('clipboard unavailable');
    vi.spyOn(ClipboardService, 'writeText').mockRejectedValue(error);
    const { wrapper } = mountGuidance([failureDetail('/fixture/cache')]);
    await wrapper.setProps({ modelValue: true });
    await wrapper.get('.scan-failure-path-row button').trigger('click');
    expect(wrapper.emitted('error')).toEqual([[error]]);
    expect(wrapper.emitted('update:modelValue')).toBeUndefined();
    wrapper.unmount();
  });

  it('restores the copy icon one second after the most recent successful copy', async () => {
    vi.useFakeTimers();
    vi.spyOn(ClipboardService, 'writeText').mockResolvedValue();
    const { wrapper } = mountGuidance([failureDetail('/fixture/cache')]);
    await wrapper.setProps({ modelValue: true });
    const icon = () => wrapper.get('.scan-failure-path-row [data-icon]').attributes('data-icon');
    expect(icon()).toBe(ICON_NAMES.copy);
    await wrapper.get('.scan-failure-path-row button').trigger('click');
    expect(icon()).toBe(ICON_NAMES.check);
    await vi.advanceTimersByTimeAsync(500);
    await wrapper.get('.scan-failure-path-row button').trigger('click');
    await vi.advanceTimersByTimeAsync(999);
    expect(icon()).toBe(ICON_NAMES.check);
    await vi.advanceTimersByTimeAsync(1);
    expect(icon()).toBe(ICON_NAMES.copy);
    wrapper.unmount();
  });

  it('clears copy feedback timers when the dialog closes or unmounts', async () => {
    vi.useFakeTimers();
    vi.spyOn(ClipboardService, 'writeText').mockResolvedValue();
    const { wrapper } = mountGuidance([failureDetail('/fixture/cache')]);
    await wrapper.setProps({ modelValue: true });
    await wrapper.get('.scan-failure-path-row button').trigger('click');
    expect(vi.getTimerCount()).toBe(1);
    await wrapper.setProps({ modelValue: false });
    expect(vi.getTimerCount()).toBe(0);
    await wrapper.setProps({ modelValue: true });
    expect(wrapper.get('.scan-failure-path-row [data-icon]').attributes('data-icon')).toBe(ICON_NAMES.copy);
    await wrapper.get('.scan-failure-path-row button').trigger('click');
    wrapper.unmount();
    expect(vi.getTimerCount()).toBe(0);
  });
});

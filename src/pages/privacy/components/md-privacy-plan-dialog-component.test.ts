// @vitest-environment happy-dom

import { flushPromises, mount } from '@vue/test-utils';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { i18n } from '@/i18n';
import type { PrivacyExecutionPlan } from '@/lib/models/privacy';

import MdPrivacyPlanDialog from './md-privacy-plan-dialog.vue';

const passthroughStub = { template: '<div><slot /></div>' };
const footerStub = { template: '<footer><slot /></footer>' };
const buttonStub = { template: '<button type="button"><slot /></button>' };
const applicationClosePanelStub = {
  props: ['selectedIds', 'items'],
  emits: ['update:selectedIds'],
  template:
    '<button class="close-panel-stub" type="button" @click="$emit(\'update:selectedIds\', items.slice(0, 2).map(item => item.id))">{{ items[0]?.iconPath }} select app</button>',
};

const plan: PrivacyExecutionPlan = {
  schemaVersion: 2,
  planId: 'plan-identifier-fixture',
  scanId: 'scan',
  createdAtMs: 1,
  expiresAtMs: 2,
  requiresConfirmation: true,
  requiresBrowserClose: true,
  browserCloseRequirements: [{ sourceId: 'chrome', sourceName: 'Google Chrome', processes: ['Google Chrome'] }],
  items: [
    {
      token: 'token',
      sourceId: 'chrome',
      sourceName: 'Google Chrome',
      profileName: 'Default',
      kind: 'downloadHistory',
      impact: 'low',
      itemCount: 3,
      estimatedBytes: 10,
      requiresBrowserClose: true,
      synchronizationMayPropagate: false,
    },
  ],
};

function mountDialog(value: PrivacyExecutionPlan = plan) {
  return mount(MdPrivacyPlanDialog, {
    props: {
      modelValue: true,
      plan: value,
      busy: false,
      closingBrowsers: false,
      closeResult: null,
      browserStatusResult: null,
      sourceIconPaths: { chrome: '/Applications/Google Chrome.app' },
    },
    global: {
      plugins: [i18n],
      stubs: {
        Button: buttonStub,
        Checkbox: passthroughStub,
        Dialog: passthroughStub,
        DialogDescription: passthroughStub,
        DialogTitle: passthroughStub,
        MdApplicationClosePanel: applicationClosePanelStub,
        MdApplicationIcon: { props: ['src'], template: '<i class="application-icon-stub" :data-src="src" />' },
        MdDialogContent: passthroughStub,
        MdDialogFooter: footerStub,
        MdDialogHeader: passthroughStub,
        MdInlineNotice: passthroughStub,
      },
    },
  });
}

describe('privacy plan dialog component', () => {
  afterEach(() => vi.useRealTimers());

  it('shows a generic running-app prompt and allows optional close selection', async () => {
    const wrapper = mountDialog();

    expect(wrapper.text()).toContain('Some apps are running');
    expect(wrapper.get('.close-panel-stub').text()).toContain('/Applications/Google Chrome.app');
    expect(wrapper.text()).toContain('Skip running apps and continue');
    expect(wrapper.get('.confirmation-item-badge').text()).toBe('Low risk');
    expect(wrapper.text()).not.toContain('Low impact');

    await wrapper.get('.close-panel-stub').trigger('click');
    const primary = wrapper.findAll('button').at(-1)!;
    expect(primary.text()).toContain('Close 1 app and continue');
    await primary.trigger('click');
    expect(wrapper.emitted('closeBrowsers')).toEqual([[['chrome'], 'graceful']]);
  });

  it('skips only unselected and still-running browsers after a partial graceful close', async () => {
    const multiBrowserPlan = structuredClone(plan);
    multiBrowserPlan.browserCloseRequirements = [
      { sourceId: 'chrome', sourceName: 'Google Chrome', processes: ['Google Chrome'] },
      { sourceId: 'edge', sourceName: 'Microsoft Edge', processes: ['msedge.exe'] },
      { sourceId: 'firefox', sourceName: 'Firefox', processes: ['firefox'] },
    ];
    const wrapper = mountDialog(multiBrowserPlan);

    await wrapper.get('.close-panel-stub').trigger('click');
    await wrapper.setProps({
      closeResult: {
        mode: 'graceful',
        matchedProcessCount: 2,
        requestedProcessCount: 2,
        remainingProcessCount: 1,
        failedTargetCount: 1,
        elapsedMs: 4,
        targets: [
          {
            targetId: 'edge',
            status: 'failed',
            matchedProcessCount: 2,
            requestedProcessCount: 2,
            remainingProcesses: ['msedge.exe'],
          },
        ],
      },
    });

    const skip = wrapper.findAll('button').find(button => button.text().includes('Skip and continue'))!;
    await skip.trigger('click');
    expect(wrapper.emitted('continue')).toEqual([[['firefox', 'edge']]]);
    wrapper.unmount();
  });

  it('automatically refreshes the force-close list and continues when every selected app has stopped', async () => {
    vi.useFakeTimers();
    const wrapper = mountDialog();
    await wrapper.get('.close-panel-stub').trigger('click');
    await wrapper.setProps({
      closeResult: {
        mode: 'graceful',
        matchedProcessCount: 1,
        requestedProcessCount: 1,
        remainingProcessCount: 1,
        failedTargetCount: 0,
        elapsedMs: 5_000,
        targets: [
          {
            targetId: 'chrome',
            status: 'completed',
            matchedProcessCount: 1,
            requestedProcessCount: 1,
            remainingProcesses: ['chrome.exe'],
          },
        ],
      },
    });

    await vi.advanceTimersByTimeAsync(1_000);
    expect(wrapper.emitted('refreshBrowserStatus')).toEqual([[['chrome']]]);

    await wrapper.setProps({
      browserStatusResult: {
        runningProcessCount: 0,
        elapsedMs: 2,
        targets: [{ sourceId: 'chrome', runningProcesses: [] }],
      },
    });
    await flushPromises();

    expect(wrapper.emitted('continue')).toEqual([[[]]]);
    wrapper.unmount();
  });

  it('requires explicit risk acceptance for personal browser data', () => {
    const personalDataPlan = structuredClone(plan);
    personalDataPlan.browserCloseRequirements = [];
    personalDataPlan.requiresBrowserClose = false;
    personalDataPlan.items = [
      {
        ...personalDataPlan.items[0]!,
        kind: 'savedPasswords',
        impact: 'dataLoss',
        synchronizationMayPropagate: true,
      },
    ];
    const wrapper = mountDialog(personalDataPlan);

    expect(wrapper.text()).not.toContain('saved passwords or autofill history');
    expect(wrapper.text()).not.toContain('StrawberryDisk clears only selected items');
    expect(wrapper.get('footer .risk-acceptance').text()).toContain('permanently delete personal data');
    expect(wrapper.findAll('button').at(-1)?.attributes('disabled')).toBeDefined();
    wrapper.unmount();
  });

  it('keeps the same cleanup kind separated by application and profile', () => {
    const groupedPlan = structuredClone(plan);
    groupedPlan.browserCloseRequirements = [];
    groupedPlan.requiresBrowserClose = false;
    groupedPlan.items.push({
      ...groupedPlan.items[0]!,
      token: 'chrome-history',
      kind: 'browsingHistory',
      itemCount: 2,
    });
    groupedPlan.items.push({
      ...groupedPlan.items[0]!,
      token: 'second-profile',
      sourceName: 'Microsoft Edge',
      profileName: 'Work',
      itemCount: 5,
    });

    const wrapper = mountDialog(groupedPlan);
    const groups = wrapper.findAll('.plan-source-group');
    const edgeGroup = groups.find(group => group.text().includes('Microsoft Edge'))!;
    const chromeGroup = groups.find(group => group.text().includes('Google Chrome'))!;

    expect(groups).toHaveLength(2);
    expect(edgeGroup.get('.plan-source-header').text()).toContain('Work');
    expect(edgeGroup.text()).toContain('5 traces');
    expect(chromeGroup.get('.plan-source-header').text()).toContain('Default');
    expect(chromeGroup.findAll('.confirmation-item-list > div')).toHaveLength(2);
    expect(chromeGroup.text()).toContain('Download history');
    expect(chromeGroup.text()).toContain('Browsing history');
    wrapper.unmount();
  });
});

// @vitest-environment happy-dom

import { mount } from '@vue/test-utils';
import { describe, expect, it, vi } from 'vitest';

import { i18n } from '@/i18n';
import type { PresentedCleanupActionResult } from '@/lib/models/cleanup-action';
import type { PresentedCleanupResult } from '@/lib/models/cleanup';

import MdCleanupResultDialog from './md-cleanup-result-dialog.vue';

vi.mock('@tauri-apps/plugin-os', () => ({ platform: () => 'windows' }));

const passthroughStub = { template: '<div><slot /></div>' };
const dialogContentStub = {
  props: {
    height: {
      type: String,
      default: 'auto',
    },
  },
  template: '<section class="dialog-content-stub" :data-dialog-height="height"><slot /></section>',
};

function createResult(actionCount: number): PresentedCleanupResult {
  const actions: PresentedCleanupActionResult[] = Array.from({ length: actionCount }, (_, index) => ({
    ruleId: `fixture.rule-${index + 1}`,
    actionKind: 'delete',
    status: 'completed',
    reasonCode: null,
    bytesExpected: 1024,
    releasedBytes: 1024,
    affectedItemCount: 1,
    failedItemCount: 0,
    runningProcesses: [],
    name: `Cleanup rule ${index + 1}`,
    message: 'Cleanup completed',
  }));
  const releasedBytes = actionCount * 1024;

  return {
    planId: 'fixture-plan',
    planHash: 'fixture-hash',
    expectedBytes: releasedBytes,
    releasedBytes,
    affectedItemCount: actionCount,
    failedItemCount: 0,
    dryRun: false,
    actions,
    record: {
      schemaVersion: 1,
      operationId: 'fixture-operation',
      category: 'deepCleanup',
      startedAtMs: 1,
      finishedAtMs: 2,
      outcome: 'completed',
      dryRun: false,
      selectedItemCount: actionCount,
      affectedItemCount: actionCount,
      expectedBytes: releasedBytes,
      releasedBytes,
      releasedBytesIsEstimate: false,
      failedItemCount: 0,
      details: {
        type: 'deepCleanup',
        payload: {
          cleanup: {
            selectedRuleIds: actions.map(action => action.ruleId),
            expectedBytes: releasedBytes,
            actions,
          },
          applicationLeftovers: null,
        },
      },
    },
    historySaved: true,
  };
}

function mountDialog(actionCount: number) {
  return mount(MdCleanupResultDialog, {
    props: {
      modelValue: true,
      result: createResult(actionCount),
      leftoverResult: null,
    },
    global: {
      plugins: [i18n],
      stubs: {
        Button: passthroughStub,
        Dialog: passthroughStub,
        DialogDescription: passthroughStub,
        DialogTitle: passthroughStub,
        MdDialogContent: dialogContentStub,
        MdDialogFooter: passthroughStub,
        MdDialogHeader: passthroughStub,
        MdIcon: true,
      },
    },
  });
}

describe('cleanup result dialog component', () => {
  it('uses a bounded tall layout when the result list is long', () => {
    const wrapper = mountDialog(10);

    expect(wrapper.get('.dialog-content-stub').attributes('data-dialog-height')).toBe('tall');
    expect(wrapper.findAll('.operation-result-items > div')).toHaveLength(10);
    wrapper.unmount();
  });

  it('keeps a short result content-sized', () => {
    const wrapper = mountDialog(3);

    expect(wrapper.get('.dialog-content-stub').attributes('data-dialog-height')).toBe('auto');
    wrapper.unmount();
  });
});

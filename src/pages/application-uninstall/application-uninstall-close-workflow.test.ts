import { describe, expect, it } from 'vitest';

import type { ApplicationCloseBatchResult, ApplicationCloseTargetResult } from '@/lib/models/application-close';

import {
  applicationCloseRequestIds,
  applyApplicationCloseResult,
  beginApplicationCloseWorkflow,
  createApplicationUninstallCloseWorkflow,
  finishApplicationCloseWorkflow,
} from './application-uninstall-close-workflow';

function target(
  targetId: string,
  status: ApplicationCloseTargetResult['status'] = 'completed',
  remainingProcesses: string[] = []
): ApplicationCloseTargetResult {
  return {
    targetId,
    status,
    matchedProcessCount: 1,
    requestedProcessCount: 1,
    remainingProcesses,
  };
}

function closeResult(targets: ApplicationCloseTargetResult[]): ApplicationCloseBatchResult {
  return {
    mode: 'graceful',
    matchedProcessCount: targets.length,
    requestedProcessCount: targets.length,
    remainingProcessCount: targets.reduce((count, item) => count + item.remainingProcesses.length, 0),
    failedTargetCount: targets.filter(item => item.status === 'failed').length,
    targets,
    elapsedMs: 10,
  };
}

describe('application uninstall close workflow', () => {
  it('starts from a fresh process snapshot', () => {
    expect(beginApplicationCloseWorkflow(['app-a', 'app-b'])).toEqual({
      open: true,
      phase: 'selection',
      pendingApplicationIds: ['app-a', 'app-b'],
      remainingApplicationIds: [],
      selectedApplicationIds: [],
    });
  });

  it('uses the selected applications for graceful close and remaining applications for force close', () => {
    const workflow = {
      ...beginApplicationCloseWorkflow(['app-a', 'app-b']),
      remainingApplicationIds: ['app-b'],
      selectedApplicationIds: ['app-a', 'app-b'],
    };

    expect(applicationCloseRequestIds(workflow, 'graceful')).toEqual(['app-a', 'app-b']);
    expect(applicationCloseRequestIds(workflow, 'force')).toEqual(['app-b']);
  });

  it('moves only failed or still-running applications into force-close review', () => {
    const workflow = beginApplicationCloseWorkflow(['app-a', 'app-b', 'app-c']);
    const transition = applyApplicationCloseResult(
      workflow,
      closeResult([target('app-a'), target('app-b', 'failed'), target('app-c', 'completed', ['worker'])])
    );

    expect(transition.completed).toBe(false);
    expect(transition.workflow.phase).toBe('force');
    expect(transition.workflow.remainingApplicationIds).toEqual(['app-b', 'app-c']);
  });

  it('marks a close batch complete when no processes remain', () => {
    const workflow = beginApplicationCloseWorkflow(['app-a']);
    const transition = applyApplicationCloseResult(workflow, closeResult([target('app-a')]));

    expect(transition).toEqual({ completed: true, workflow });
  });

  it('removes skipped and unselected running applications before uninstall preparation', () => {
    const workflow = {
      ...beginApplicationCloseWorkflow(['app-a', 'app-b', 'app-c']),
      selectedApplicationIds: ['app-a', 'app-b'],
    };
    const selection = {
      applicationIds: ['app-a', 'app-b', 'app-c', 'app-d'],
      componentIds: {
        'app-a': ['component-a'],
        'app-b': ['component-b'],
        'app-c': ['component-c'],
        'app-d': ['component-d'],
      },
    };

    expect(finishApplicationCloseWorkflow(selection, workflow, ['app-b'])).toEqual({
      applicationIds: ['app-a', 'app-d'],
      componentIds: {
        'app-a': ['component-a'],
        'app-d': ['component-d'],
      },
    });
  });

  it('creates an isolated empty workflow for every reset', () => {
    const first = createApplicationUninstallCloseWorkflow();
    first.pendingApplicationIds.push('app-a');

    expect(createApplicationUninstallCloseWorkflow().pendingApplicationIds).toEqual([]);
  });
});

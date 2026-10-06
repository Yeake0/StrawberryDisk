import type { ApplicationCloseBatchResult, ApplicationCloseMode } from '@/lib/models/application-close';

import type { ApplicationUninstallSelection } from './application-uninstall-selection';

export interface ApplicationUninstallCloseWorkflow {
  open: boolean;
  phase: 'selection' | 'force';
  pendingApplicationIds: string[];
  remainingApplicationIds: string[];
  selectedApplicationIds: string[];
}

export interface ApplicationCloseResultTransition {
  completed: boolean;
  workflow: ApplicationUninstallCloseWorkflow;
}

export function createApplicationUninstallCloseWorkflow(): ApplicationUninstallCloseWorkflow {
  return {
    open: false,
    phase: 'selection',
    pendingApplicationIds: [],
    remainingApplicationIds: [],
    selectedApplicationIds: [],
  };
}

export function beginApplicationCloseWorkflow(applicationIds: readonly string[]): ApplicationUninstallCloseWorkflow {
  return {
    ...createApplicationUninstallCloseWorkflow(),
    open: true,
    pendingApplicationIds: [...applicationIds],
  };
}

export function applicationCloseRequestIds(
  workflow: ApplicationUninstallCloseWorkflow,
  mode: ApplicationCloseMode
): string[] {
  return mode === 'force' ? workflow.remainingApplicationIds : workflow.selectedApplicationIds;
}

export function applyApplicationCloseResult(
  workflow: ApplicationUninstallCloseWorkflow,
  result: ApplicationCloseBatchResult
): ApplicationCloseResultTransition {
  const remainingApplicationIds = result.targets
    .filter(target => target.status === 'failed' || target.remainingProcesses.length > 0)
    .map(target => target.targetId);
  if (!remainingApplicationIds.length) {
    return { completed: true, workflow };
  }
  return {
    completed: false,
    workflow: {
      ...workflow,
      phase: 'force',
      remainingApplicationIds,
    },
  };
}

export function finishApplicationCloseWorkflow(
  selection: ApplicationUninstallSelection,
  workflow: ApplicationUninstallCloseWorkflow,
  skippedApplicationIds: readonly string[]
): ApplicationUninstallSelection {
  const requested = new Set(workflow.selectedApplicationIds);
  const skipped = new Set(skippedApplicationIds);
  const removed = new Set(
    workflow.pendingApplicationIds.filter(applicationId => !requested.has(applicationId) || skipped.has(applicationId))
  );
  return {
    applicationIds: selection.applicationIds.filter(applicationId => !removed.has(applicationId)),
    componentIds: Object.fromEntries(
      Object.entries(selection.componentIds).filter(([applicationId]) => !removed.has(applicationId))
    ),
  };
}

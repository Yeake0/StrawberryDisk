import type { StartupChangeSelection, StartupDesiredState } from '@/lib/models/startup';

export type StartupQueuedChange = Omit<StartupChangeSelection, 'scanId'> & {
  requiresReview?: boolean;
};

export interface StartupChangeWorkflow {
  activeChange: StartupQueuedChange | null;
  queuedChanges: StartupQueuedChange[];
}

export interface StartupChangeDispatch {
  change: StartupQueuedChange | null;
  workflow: StartupChangeWorkflow;
}

export interface StartupQueueCancellation {
  cancelledBatchCount: number;
  cancelledItemCount: number;
  workflow: StartupChangeWorkflow;
}

export function createStartupChangeWorkflow(): StartupChangeWorkflow {
  return { activeChange: null, queuedChanges: [] };
}

export function enqueueStartupChange(
  queue: readonly StartupQueuedChange[],
  itemIds: readonly string[],
  desiredState: StartupDesiredState,
  maximumBatchSize: number,
  requiresReview = false
): StartupQueuedChange[] {
  const uniqueItemIds = [...new Set(itemIds)];
  if (!uniqueItemIds.length) return [...queue];

  const replacedIds = new Set(uniqueItemIds);
  const next = queue
    .map(change => ({ ...change, itemIds: change.itemIds.filter(itemId => !replacedIds.has(itemId)) }))
    .filter(change => change.itemIds.length > 0);
  const batchSize = Number.isSafeInteger(maximumBatchSize) && maximumBatchSize > 0 ? maximumBatchSize : 1;

  for (let offset = 0; offset < uniqueItemIds.length; offset += batchSize) {
    const incoming = uniqueItemIds.slice(offset, offset + batchSize);
    let mergeTarget: StartupQueuedChange | undefined;
    for (let index = next.length - 1; index >= 0; index -= 1) {
      const candidate = next[index]!;
      if (
        candidate.desiredState === desiredState &&
        Boolean(candidate.requiresReview) === requiresReview &&
        candidate.itemIds.length + incoming.length <= batchSize
      ) {
        mergeTarget = candidate;
        break;
      }
    }
    if (mergeTarget) {
      mergeTarget.itemIds = [...mergeTarget.itemIds, ...incoming];
    } else {
      next.push({ itemIds: incoming, desiredState, ...(requiresReview ? { requiresReview: true } : {}) });
    }
  }

  return next;
}

export function queuedStartupItemIds(workflow: StartupChangeWorkflow): ReadonlySet<string> {
  return new Set([
    ...(workflow.activeChange?.itemIds ?? []),
    ...workflow.queuedChanges.flatMap(change => change.itemIds),
  ]);
}

export function enqueueStartupWorkflow(
  workflow: StartupChangeWorkflow,
  itemIds: readonly string[],
  desiredState: StartupDesiredState,
  maximumBatchSize: number,
  requiresReview = false
): StartupChangeWorkflow {
  return {
    ...workflow,
    queuedChanges: enqueueStartupChange(
      workflow.queuedChanges,
      itemIds,
      desiredState,
      maximumBatchSize,
      requiresReview
    ),
  };
}

export function dispatchNextStartupChange(workflow: StartupChangeWorkflow): StartupChangeDispatch {
  if (workflow.activeChange) return { change: null, workflow };
  const [change, ...queuedChanges] = workflow.queuedChanges;
  if (!change) return { change: null, workflow };
  return {
    change,
    workflow: {
      activeChange: change,
      queuedChanges,
    },
  };
}

export function completeStartupChange(workflow: StartupChangeWorkflow): StartupChangeWorkflow {
  return { ...workflow, activeChange: null };
}

export function cancelQueuedStartupChanges(workflow: StartupChangeWorkflow): StartupQueueCancellation {
  return {
    cancelledBatchCount: workflow.queuedChanges.length,
    cancelledItemCount: workflow.queuedChanges.reduce((count, change) => count + change.itemIds.length, 0),
    workflow: { ...workflow, queuedChanges: [] },
  };
}

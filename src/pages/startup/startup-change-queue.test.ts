import { describe, expect, it } from 'vitest';

import {
  cancelQueuedStartupChanges,
  completeStartupChange,
  createStartupChangeWorkflow,
  dispatchNextStartupChange,
  enqueueStartupChange,
  enqueueStartupWorkflow,
  queuedStartupItemIds,
  type StartupQueuedChange,
} from './startup-change-queue';

describe('startup change queue', () => {
  it('coalesces compatible changes with the same desired state', () => {
    const first = enqueueStartupChange([], ['a'], 'disabled', 3);
    const second = enqueueStartupChange(first, ['b', 'c'], 'disabled', 3);

    expect(second).toEqual([{ itemIds: ['a', 'b', 'c'], desiredState: 'disabled' }]);
  });

  it('keeps opposite desired states in separate batches', () => {
    const first = enqueueStartupChange([], ['a'], 'disabled', 3);
    const second = enqueueStartupChange(first, ['b'], 'enabled', 3);

    expect(second).toEqual([
      { itemIds: ['a'], desiredState: 'disabled' },
      { itemIds: ['b'], desiredState: 'enabled' },
    ]);
  });

  it('retains only the latest queued intent for an item', () => {
    const queue: StartupQueuedChange[] = [
      { itemIds: ['a', 'b'], desiredState: 'disabled' },
      { itemIds: ['c'], desiredState: 'enabled' },
    ];

    expect(enqueueStartupChange(queue, ['b'], 'enabled', 3)).toEqual([
      { itemIds: ['a'], desiredState: 'disabled' },
      { itemIds: ['c', 'b'], desiredState: 'enabled' },
    ]);
  });

  it('splits work at the backend batch limit and reports every pending item', () => {
    const queue = enqueueStartupChange([], ['a', 'b', 'c', 'd'], 'disabled', 3);

    expect(queue).toEqual([
      { itemIds: ['a', 'b', 'c'], desiredState: 'disabled' },
      { itemIds: ['d'], desiredState: 'disabled' },
    ]);
    expect(queuedStartupItemIds({ activeChange: queue[0]!, queuedChanges: queue.slice(1) })).toEqual(
      new Set(['a', 'b', 'c', 'd'])
    );
  });

  it('deduplicates item identifiers without mutating the existing queue', () => {
    const existing: StartupQueuedChange[] = [{ itemIds: ['a'], desiredState: 'disabled' }];

    expect(enqueueStartupChange(existing, ['b', 'b'], 'disabled', 3)).toEqual([
      { itemIds: ['a', 'b'], desiredState: 'disabled' },
    ]);
    expect(existing).toEqual([{ itemIds: ['a'], desiredState: 'disabled' }]);
  });

  it('uses single-item batches when the configured limit is invalid', () => {
    expect(enqueueStartupChange([], ['a', 'b'], 'disabled', 0)).toEqual([
      { itemIds: ['a'], desiredState: 'disabled' },
      { itemIds: ['b'], desiredState: 'disabled' },
    ]);
  });

  it('does not merge a grouped change that requires review into ordinary queued work', () => {
    const ordinary = enqueueStartupChange([], ['a'], 'disabled', 3);

    expect(enqueueStartupChange(ordinary, ['b', 'c'], 'disabled', 3, true)).toEqual([
      { itemIds: ['a'], desiredState: 'disabled' },
      { itemIds: ['b', 'c'], desiredState: 'disabled', requiresReview: true },
    ]);
  });

  it('owns enqueue, dispatch, completion, and pending identifiers as one workflow', () => {
    const queued = enqueueStartupWorkflow(createStartupChangeWorkflow(), ['a', 'b'], 'disabled', 1);
    const dispatched = dispatchNextStartupChange(queued);

    expect(dispatched.change).toEqual({ itemIds: ['a'], desiredState: 'disabled' });
    expect(queuedStartupItemIds(dispatched.workflow)).toEqual(new Set(['a', 'b']));
    expect(completeStartupChange(dispatched.workflow)).toEqual({
      activeChange: null,
      queuedChanges: [{ itemIds: ['b'], desiredState: 'disabled' }],
    });
  });

  it('does not dispatch while another startup change is active', () => {
    const workflow = {
      activeChange: { itemIds: ['a'], desiredState: 'enabled' as const },
      queuedChanges: [{ itemIds: ['b'], desiredState: 'disabled' as const }],
    };

    expect(dispatchNextStartupChange(workflow)).toEqual({ change: null, workflow });
  });

  it('cancels only queued batches and reports their exact size', () => {
    const workflow = {
      activeChange: { itemIds: ['a'], desiredState: 'enabled' as const },
      queuedChanges: [
        { itemIds: ['b', 'c'], desiredState: 'disabled' as const },
        { itemIds: ['d'], desiredState: 'removed' as const },
      ],
    };

    expect(cancelQueuedStartupChanges(workflow)).toEqual({
      cancelledBatchCount: 2,
      cancelledItemCount: 3,
      workflow: {
        activeChange: workflow.activeChange,
        queuedChanges: [],
      },
    });
  });
});

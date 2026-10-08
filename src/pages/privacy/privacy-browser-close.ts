import type { ApplicationCloseBatchResult, ApplicationCloseItem } from '@/lib/models/application-close';
import type { PrivacyBrowserCloseRequirement, PrivacyBrowserStatusResult } from '@/lib/models/privacy';

export interface PrivacyBrowserCloseRetry {
  items: ApplicationCloseItem[];
  sourceIds: string[];
}

export function privacyBrowserCloseItems(
  requirements: readonly PrivacyBrowserCloseRequirement[],
  sourceIconPaths: Readonly<Record<string, string>> = {}
): ApplicationCloseItem[] {
  return requirements.map(requirement => ({
    id: requirement.sourceId,
    name: requirement.sourceName,
    processes: requirement.processes,
    iconPath: sourceIconPaths[requirement.sourceId],
  }));
}

/**
 * Keeps a force-close retry bounded to the exact browser sources Core could
 * not confirm as stopped. The original UI selection must not be replayed after
 * a partial success because that would broaden the second destructive action.
 */
export function privacyBrowserCloseRetry(
  requirements: readonly PrivacyBrowserCloseRequirement[],
  result: ApplicationCloseBatchResult,
  sourceIconPaths: Readonly<Record<string, string>> = {}
): PrivacyBrowserCloseRetry {
  const remaining = result.targets.filter(target => target.status === 'failed' || target.remainingProcesses.length > 0);
  const sourceIds = remaining.map(target => target.targetId);
  const remainingBySource = new Map(remaining.map(target => [target.targetId, target.remainingProcesses] as const));

  return {
    sourceIds,
    items: requirements
      .filter(requirement => remainingBySource.has(requirement.sourceId))
      .map(requirement => {
        const processes = remainingBySource.get(requirement.sourceId) ?? [];
        return {
          id: requirement.sourceId,
          name: requirement.sourceName,
          processes: processes.length ? processes : requirement.processes,
          iconPath: sourceIconPaths[requirement.sourceId],
        };
      }),
  };
}

/**
 * Rebuilds the force-close list from a read-only process refresh. Missing
 * target results remain visible so a partial native response can never make
 * the confirmation flow assume that an application has stopped.
 */
export function privacyBrowserStatusRetry(
  requirements: readonly PrivacyBrowserCloseRequirement[],
  result: PrivacyBrowserStatusResult,
  sourceIconPaths: Readonly<Record<string, string>> = {}
): PrivacyBrowserCloseRetry {
  const runningBySource = new Map(result.targets.map(target => [target.sourceId, target.runningProcesses] as const));
  const remaining = requirements.filter(requirement => {
    const processes = runningBySource.get(requirement.sourceId);
    return !processes || processes.length > 0;
  });
  return {
    sourceIds: remaining.map(requirement => requirement.sourceId),
    items: remaining.map(requirement => ({
      id: requirement.sourceId,
      name: requirement.sourceName,
      processes: runningBySource.get(requirement.sourceId) ?? requirement.processes,
      iconPath: sourceIconPaths[requirement.sourceId],
    })),
  };
}

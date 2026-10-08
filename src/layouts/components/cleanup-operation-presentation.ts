import {
  CLEANUP_OPERATION_IDS,
  CLEANUP_RULE_IDS,
  type CleanupExecutionProgress,
  type CleanupOperationId,
} from '@/lib/models/cleanup';

type CleanupProgressIdentity = Pick<CleanupExecutionProgress, 'currentRuleId' | 'stage'>;

/**
 * Identifies the native Windows cleanup step that can pause for a system confirmation dialog.
 * Keeping this decision based on stable protocol IDs prevents the UI from inferring behavior from
 * localized rule names or backend diagnostic messages.
 */
export function isWaitingForPreviousWindowsInstallationCleanup(
  operation: CleanupOperationId,
  progress: CleanupProgressIdentity | null | undefined
): boolean {
  return (
    operation === CLEANUP_OPERATION_IDS.cleaning &&
    progress?.stage === 'cleaning' &&
    progress.currentRuleId === CLEANUP_RULE_IDS.windowsPreviousInstallations
  );
}

import { describe, expect, it } from 'vitest';

import { CLEANUP_OPERATION_IDS, CLEANUP_RULE_IDS, type CleanupExecutionProgress } from '@/lib/models/cleanup';

import { isWaitingForPreviousWindowsInstallationCleanup } from './cleanup-operation-presentation';

const activeProgress = {
  currentRuleId: CLEANUP_RULE_IDS.windowsPreviousInstallations,
  stage: 'cleaning',
} satisfies Pick<CleanupExecutionProgress, 'currentRuleId' | 'stage'>;

describe('isWaitingForPreviousWindowsInstallationCleanup', () => {
  it('identifies only active destructive cleanup of previous Windows installations', () => {
    expect(isWaitingForPreviousWindowsInstallationCleanup(CLEANUP_OPERATION_IDS.cleaning, activeProgress)).toBe(true);
    expect(isWaitingForPreviousWindowsInstallationCleanup(CLEANUP_OPERATION_IDS.previewing, activeProgress)).toBe(
      false
    );
    expect(
      isWaitingForPreviousWindowsInstallationCleanup(CLEANUP_OPERATION_IDS.cleaning, {
        ...activeProgress,
        stage: 'validating',
      })
    ).toBe(false);
    expect(
      isWaitingForPreviousWindowsInstallationCleanup(CLEANUP_OPERATION_IDS.cleaning, {
        ...activeProgress,
        currentRuleId: CLEANUP_RULE_IDS.windowsRecycleBin,
      })
    ).toBe(false);
  });
});

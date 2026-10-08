export interface CleanupActionResult {
  ruleId: string;
  actionKind: 'delete' | 'command' | 'optimize';
  status: 'blocked' | 'previewed' | 'completed' | 'partial' | 'failed';
  reasonCode:
    | 'runningProcesses'
    | 'itemsSkipped'
    | 'requiredToolUnavailable'
    | 'preflightFailed'
    | 'executionFailed'
    | 'verificationFailed'
    | 'cleanerUnavailable'
    | 'nameExclusionsUnsupported'
    | 'cancelled'
    | null;
  bytesExpected: number;
  releasedBytes: number;
  affectedItemCount: number;
  failedItemCount: number;
  runningProcesses: string[];
}

export type PresentedCleanupActionResult = CleanupActionResult & {
  name: string;
  message: string;
};

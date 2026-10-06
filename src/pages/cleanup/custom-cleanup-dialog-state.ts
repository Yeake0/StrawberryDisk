import type { CustomCleanupRule } from '@/lib/models/custom-cleanup';

/**
 * Produces the stable comparison used by the dialog's Save button. Keeping
 * this page-specific state transition pure makes the non-closing Save flow
 * testable without mounting the native folder picker or Tauri dialog shell.
 */
export function customCleanupDraftFingerprint(rules: CustomCleanupRule[], includeStandardRules: boolean): string {
  return JSON.stringify({ includeStandardRules, rules });
}

export function customCleanupPersistedState(
  rules: CustomCleanupRule[],
  includeStandardRules: boolean,
  scan: boolean
): { fingerprint: string; closeDialog: boolean; startScan: boolean } {
  return {
    fingerprint: customCleanupDraftFingerprint(rules, includeStandardRules),
    closeDialog: scan,
    startScan: scan,
  };
}

import { describe, expect, it } from 'vitest';

import { CLEANUP_SCAN_SCOPE_MODES, STANDARD_CLEANUP_SCAN_SCOPE } from '@/lib/models/cleanup';
import * as CleanupScanScopeUtils from '@/lib/utils/cleanup-scan-scope';
import * as CustomCleanupPreferenceUtils from '@/lib/utils/custom-cleanup-preference';

describe('CleanupScanScopeUtils', () => {
  it('includes standard cleanup for standard and selected-volume scans', () => {
    expect(CleanupScanScopeUtils.includesStandardCleanup(STANDARD_CLEANUP_SCAN_SCOPE)).toBe(true);
    expect(
      CleanupScanScopeUtils.includesStandardCleanup({
        mode: CLEANUP_SCAN_SCOPE_MODES.selectedVolumes,
        volumeMountPoints: ['/'],
      })
    ).toBe(true);
  });

  it('uses the explicit standard-cleanup choice for custom scans', () => {
    const rules = [CustomCleanupPreferenceUtils.create()];
    expect(
      CleanupScanScopeUtils.includesStandardCleanup({
        mode: CLEANUP_SCAN_SCOPE_MODES.custom,
        includeStandardRules: true,
        rules,
      })
    ).toBe(true);
    expect(
      CleanupScanScopeUtils.includesStandardCleanup({
        mode: CLEANUP_SCAN_SCOPE_MODES.custom,
        includeStandardRules: false,
        rules,
      })
    ).toBe(false);
  });
});

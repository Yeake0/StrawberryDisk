import { describe, expect, it } from 'vitest';

import * as CustomCleanupPreferenceUtils from '@/lib/utils/custom-cleanup-preference';

import { customCleanupDraftFingerprint, customCleanupPersistedState } from './custom-cleanup-dialog-state';

describe('custom cleanup dialog persistence state', () => {
  it('keeps the dialog open and makes the unchanged draft clean after Save', () => {
    const rules = [CustomCleanupPreferenceUtils.create()];
    const persisted = customCleanupPersistedState(rules, true, false);

    expect(persisted.closeDialog).toBe(false);
    expect(persisted.startScan).toBe(false);
    expect(customCleanupDraftFingerprint(rules, true)).toBe(persisted.fingerprint);
  });

  it('closes and starts scanning only for Save and Scan', () => {
    const persisted = customCleanupPersistedState([CustomCleanupPreferenceUtils.create()], false, true);

    expect(persisted.closeDialog).toBe(true);
    expect(persisted.startScan).toBe(true);
  });
});

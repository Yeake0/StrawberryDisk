import { describe, expect, it } from 'vitest';

import {
  CUSTOM_CLEANUP_PREFERENCES_SCHEMA_VERSION,
  CUSTOM_CLEANUP_RULE_SCHEMA_VERSION,
  type CustomCleanupPreferences,
} from '@/lib/models/custom-cleanup';
import * as CustomCleanupPreferenceUtils from '@/lib/utils/custom-cleanup-preference';
import { CustomCleanupPreferenceError } from '@/lib/utils/custom-cleanup-preference';

function fixture(): CustomCleanupPreferences {
  return {
    schemaVersion: CUSTOM_CLEANUP_PREFERENCES_SCHEMA_VERSION,
    includeStandardRules: false,
    rules: [
      {
        schemaVersion: CUSTOM_CLEANUP_RULE_SCHEMA_VERSION,
        id: 'fixture-rule',
        name: 'Temporary downloads',
        roots: ['/tmp/custom-cleanup'],
        namePatterns: ['*.tmp', '*.part'],
        minimumBytes: 0,
        maximumBytes: null,
        modifiedTime: { mode: 'olderThan', days: 30 },
        recursive: true,
        removeEmptyDirectories: true,
      },
    ],
  };
}

describe('CustomCleanupPreferenceUtils', () => {
  it('normalizes a valid versioned rule document', () => {
    expect(CustomCleanupPreferenceUtils.parse(fixture())).toEqual(fixture());
  });

  it('defaults existing preferences to a standard scan and preserves an explicit choice', () => {
    const existing = fixture();
    delete (existing as Partial<typeof existing>).includeStandardRules;
    expect(CustomCleanupPreferenceUtils.parse(existing).includeStandardRules).toBe(true);
    expect(CustomCleanupPreferenceUtils.parse(fixture()).includeStandardRules).toBe(false);
  });

  it('keeps older rules compatible and disables empty-folder removal by default', () => {
    const existing = fixture();
    delete (existing.rules[0] as Partial<(typeof existing.rules)[number]>).removeEmptyDirectories;

    expect(CustomCleanupPreferenceUtils.parse(existing).rules[0]!.removeEmptyDirectories).toBe(false);
  });

  it('rejects an invalid empty-folder removal preference', () => {
    const invalid = fixture();
    (invalid.rules[0] as unknown as Record<string, unknown>).removeEmptyDirectories = 'yes';

    expect(() => CustomCleanupPreferenceUtils.parse(invalid)).toThrow();
  });

  it('rejects path patterns and duplicate rule identifiers', () => {
    const pathPattern = fixture();
    pathPattern.rules[0]!.namePatterns = ['nested/*.tmp'];
    expect(() => CustomCleanupPreferenceUtils.parse(pathPattern)).toThrow();

    const duplicate = fixture();
    duplicate.rules.push({ ...duplicate.rules[0]!, roots: ['/tmp/other-custom-cleanup'] });
    expect(() => CustomCleanupPreferenceUtils.parse(duplicate)).toThrow();
  });

  it('rejects a rule without a display name', () => {
    const preferences = fixture();
    preferences.rules[0]!.name = '   ';

    try {
      CustomCleanupPreferenceUtils.parse(preferences);
      expect.fail('Expected the invalid rule name to be rejected');
    } catch (error) {
      expect(error).toBeInstanceOf(CustomCleanupPreferenceError);
      expect((error as CustomCleanupPreferenceError).code).toBe('ruleNameInvalid');
    }
  });

  it('returns a privacy-safe diagnostic reason for unexpected errors', () => {
    expect(CustomCleanupPreferenceUtils.errorCode(new Error('private details'))).toBe('unexpected');
  });

  it('rejects inconsistent size limits and invalid modification ages', () => {
    const sizes = fixture();
    sizes.rules[0]!.minimumBytes = 2;
    sizes.rules[0]!.maximumBytes = 1;
    expect(() => CustomCleanupPreferenceUtils.parse(sizes)).toThrow();

    const age = fixture();
    age.rules[0]!.modifiedTime = { mode: 'olderThan', days: 0 };
    expect(() => CustomCleanupPreferenceUtils.parse(age)).toThrow();
  });

  it('collapses redundant nested roots before persisting a rule', () => {
    const preferences = fixture();
    preferences.rules[0]!.roots = [
      '/tmp/custom-cleanup/nested/cache',
      '/tmp/custom-cleanup',
      '/tmp/custom-cleanup-other',
    ];

    expect(CustomCleanupPreferenceUtils.parse(preferences).rules[0]!.roots).toEqual([
      '/tmp/custom-cleanup',
      '/tmp/custom-cleanup-other',
    ]);
  });
});

export const CUSTOM_CLEANUP_RULE_SCHEMA_VERSION = 1;
export const CUSTOM_CLEANUP_PREFERENCES_SCHEMA_VERSION = 1;
export const MAX_CUSTOM_CLEANUP_RULES = 20;
export const MAX_CUSTOM_CLEANUP_ROOTS_PER_RULE = 8;
export const MAX_CUSTOM_CLEANUP_PATTERNS_PER_RULE = 16;
export const MAX_CUSTOM_CLEANUP_TEXT_LENGTH = 80;
export const MAX_CUSTOM_CLEANUP_FILTER_DAYS = 3_650;

export type CustomCleanupModifiedTime =
  { mode: 'any' } | { mode: 'olderThan'; days: number } | { mode: 'newerThan'; days: number };

export interface CustomCleanupRule {
  schemaVersion: typeof CUSTOM_CLEANUP_RULE_SCHEMA_VERSION;
  id: string;
  name: string;
  roots: string[];
  namePatterns: string[];
  minimumBytes: number | null;
  maximumBytes: number | null;
  modifiedTime: CustomCleanupModifiedTime;
  recursive: boolean;
  removeEmptyDirectories: boolean;
}

export interface CustomCleanupPreferences {
  schemaVersion: typeof CUSTOM_CLEANUP_PREFERENCES_SCHEMA_VERSION;
  includeStandardRules: boolean;
  rules: CustomCleanupRule[];
}

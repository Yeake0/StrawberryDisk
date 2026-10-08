import {
  CUSTOM_CLEANUP_PREFERENCES_SCHEMA_VERSION,
  CUSTOM_CLEANUP_RULE_SCHEMA_VERSION,
  MAX_CUSTOM_CLEANUP_FILTER_DAYS,
  MAX_CUSTOM_CLEANUP_PATTERNS_PER_RULE,
  MAX_CUSTOM_CLEANUP_RULES,
  MAX_CUSTOM_CLEANUP_ROOTS_PER_RULE,
  MAX_CUSTOM_CLEANUP_TEXT_LENGTH,
  type CustomCleanupModifiedTime,
  type CustomCleanupPreferences,
  type CustomCleanupRule,
} from '@/lib/models/custom-cleanup';
import * as PathUtils from '@/lib/utils/path';
export type CustomCleanupPreferenceErrorCode =
  | 'schemaVersionMismatch'
  | 'scanScopeInvalid'
  | 'ruleCountInvalid'
  | 'duplicateRuleId'
  | 'ruleInvalid'
  | 'ruleNameInvalid'
  | 'directoryInvalid'
  | 'patternInvalid'
  | 'sizeLimitInvalid'
  | 'sizeRangeInvalid'
  | 'modifiedTimeInvalid';
export class CustomCleanupPreferenceError extends Error {
  constructor(readonly code: CustomCleanupPreferenceErrorCode) {
    super(`Invalid custom cleanup preferences: ${code}`);
    this.name = 'CustomCleanupPreferenceError';
  }
}
export function empty(): CustomCleanupPreferences {
  return {
    schemaVersion: CUSTOM_CLEANUP_PREFERENCES_SCHEMA_VERSION,
    includeStandardRules: true,
    rules: [],
  };
}
export function parse(value: unknown): CustomCleanupPreferences {
  if (!isRecord(value) || value.schemaVersion !== CUSTOM_CLEANUP_PREFERENCES_SCHEMA_VERSION) {
    throw new CustomCleanupPreferenceError('schemaVersionMismatch');
  }
  if (!Array.isArray(value.rules) || value.rules.length > MAX_CUSTOM_CLEANUP_RULES) {
    throw new CustomCleanupPreferenceError('ruleCountInvalid');
  }
  if (value.includeStandardRules !== undefined && typeof value.includeStandardRules !== 'boolean') {
    throw new CustomCleanupPreferenceError('scanScopeInvalid');
  }
  const rules = value.rules.map(rule => parseRule(rule));
  if (new Set(rules.map(rule => rule.id)).size !== rules.length) {
    throw new CustomCleanupPreferenceError('duplicateRuleId');
  }
  return {
    schemaVersion: CUSTOM_CLEANUP_PREFERENCES_SCHEMA_VERSION,
    includeStandardRules: value.includeStandardRules ?? true,
    rules,
  };
}
export function create(): CustomCleanupRule {
  return {
    schemaVersion: CUSTOM_CLEANUP_RULE_SCHEMA_VERSION,
    id: crypto.randomUUID(),
    name: '',
    roots: [],
    namePatterns: ['*.tmp'],
    minimumBytes: null,
    maximumBytes: null,
    modifiedTime: { mode: 'any' },
    recursive: true,
    removeEmptyDirectories: false,
  };
}
function parseRule(value: unknown): CustomCleanupRule {
  if (
    !isRecord(value) ||
    value.schemaVersion !== CUSTOM_CLEANUP_RULE_SCHEMA_VERSION ||
    typeof value.id !== 'string' ||
    !/^[a-zA-Z0-9-]{1,64}$/u.test(value.id) ||
    !Array.isArray(value.roots) ||
    !Array.isArray(value.namePatterns) ||
    typeof value.recursive !== 'boolean' ||
    (value.removeEmptyDirectories !== undefined && typeof value.removeEmptyDirectories !== 'boolean')
  ) {
    throw new CustomCleanupPreferenceError('ruleInvalid');
  }
  if (
    typeof value.name !== 'string' ||
    !value.name.trim() ||
    value.name.trim().length > MAX_CUSTOM_CLEANUP_TEXT_LENGTH
  ) {
    throw new CustomCleanupPreferenceError('ruleNameInvalid');
  }
  if (!value.roots.length || value.roots.length > MAX_CUSTOM_CLEANUP_ROOTS_PER_RULE) {
    throw new CustomCleanupPreferenceError('directoryInvalid');
  }
  if (!value.namePatterns.length || value.namePatterns.length > MAX_CUSTOM_CLEANUP_PATTERNS_PER_RULE) {
    throw new CustomCleanupPreferenceError('patternInvalid');
  }
  const roots = PathUtils.collapseOverlappingRoots(uniquePaths(value.roots));
  const namePatterns = value.namePatterns.map(normalizePattern);
  const minimumBytes = optionalBytes(value.minimumBytes);
  const maximumBytes = optionalBytes(value.maximumBytes);
  if (minimumBytes !== null && maximumBytes !== null && minimumBytes > maximumBytes) {
    throw new CustomCleanupPreferenceError('sizeRangeInvalid');
  }
  return {
    schemaVersion: CUSTOM_CLEANUP_RULE_SCHEMA_VERSION,
    id: value.id,
    name: value.name.trim(),
    roots,
    namePatterns,
    minimumBytes,
    maximumBytes,
    modifiedTime: modifiedTime(value.modifiedTime),
    recursive: value.recursive,
    // This additive field intentionally defaults to false so rules saved by
    // earlier releases remain safe and compatible without a schema migration.
    removeEmptyDirectories: value.removeEmptyDirectories ?? false,
  };
}
function uniquePaths(values: unknown[]): string[] {
  const keys = new Set<string>();
  return values.map(value => {
    if (typeof value !== 'string' || !value.trim()) throw new CustomCleanupPreferenceError('directoryInvalid');
    const path = PathUtils.display(value.trim());
    const key = PathUtils.comparisonKey(path);
    if (!key || keys.has(key)) throw new CustomCleanupPreferenceError('directoryInvalid');
    keys.add(key);
    return path;
  });
}
function normalizePattern(value: unknown): string {
  if (
    typeof value !== 'string' ||
    !value.trim() ||
    value.trim().length > MAX_CUSTOM_CLEANUP_TEXT_LENGTH ||
    /[/\\]/u.test(value) ||
    value.includes('**')
  ) {
    throw new CustomCleanupPreferenceError('patternInvalid');
  }
  return value.trim();
}
function optionalBytes(value: unknown): number | null {
  if (value === null) return null;
  if (typeof value !== 'number' || !Number.isSafeInteger(value) || value < 0) {
    throw new CustomCleanupPreferenceError('sizeLimitInvalid');
  }
  return value;
}
function modifiedTime(value: unknown): CustomCleanupModifiedTime {
  if (!isRecord(value) || typeof value.mode !== 'string') {
    throw new CustomCleanupPreferenceError('modifiedTimeInvalid');
  }
  if (value.mode === 'any') return { mode: 'any' };
  if (
    !['olderThan', 'newerThan'].includes(value.mode) ||
    typeof value.days !== 'number' ||
    !Number.isSafeInteger(value.days) ||
    value.days < 1 ||
    value.days > MAX_CUSTOM_CLEANUP_FILTER_DAYS
  ) {
    throw new CustomCleanupPreferenceError('modifiedTimeInvalid');
  }
  return { mode: value.mode, days: value.days } as CustomCleanupModifiedTime;
}
function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
export function errorCode(error: unknown): CustomCleanupPreferenceErrorCode | 'unexpected' {
  return error instanceof CustomCleanupPreferenceError ? error.code : 'unexpected';
}

import {
  MAX_RECENT_STORAGE_FOLDERS,
  STORAGE_SCOPE_IDS,
  type StorageScopeId,
  type StorageScopePreferences,
} from '@/lib/models/storage-scope';
import * as PathUtils from './path';
const STORAGE_SCOPE_ID_VALUES = new Set<string>(Object.values(STORAGE_SCOPE_IDS));
export function parse(value: unknown): StorageScopePreferences {
  const legacyDocument =
    isRecord(value) &&
    (hasExactKeys(value, ['selectedPaths', 'recentFolders']) ||
      ((value.schemaVersion === 1 || value.schemaVersion === 2) &&
        hasExactKeys(value, ['schemaVersion', 'selectedPaths', 'recentFolders'])));
  const currentDocument =
    isRecord(value) &&
    value.schemaVersion === 3 &&
    hasExactKeys(value, ['schemaVersion', 'selectedPaths', 'recentFolders', 'duplicateFileProtectedPaths']);
  if (!isRecord(value) || (!legacyDocument && !currentDocument)) {
    throw new Error('Invalid storage scope preferences');
  }
  if (!isRecord(value.selectedPaths) || !Array.isArray(value.recentFolders)) {
    throw new Error('Invalid storage scope preferences');
  }
  const selectedPaths: StorageScopePreferences['selectedPaths'] = {};
  for (const [scopeId, selection] of Object.entries(value.selectedPaths)) {
    const paths = Array.isArray(selection) ? selection : [selection];
    const multiple = scopeId === STORAGE_SCOPE_IDS.duplicateFiles || scopeId === STORAGE_SCOPE_IDS.largeFiles;
    const arrayAllowed =
      value.schemaVersion === 2 || value.schemaVersion === 3
        ? multiple
        : value.schemaVersion === 1 && scopeId === STORAGE_SCOPE_IDS.duplicateFiles;
    if (
      !STORAGE_SCOPE_ID_VALUES.has(scopeId) ||
      (Array.isArray(selection) && !arrayAllowed) ||
      paths.some(path => typeof path !== 'string' || !path.trim())
    ) {
      throw new Error('Invalid storage scope selection');
    }
    selectedPaths[scopeId as StorageScopeId] = multiple
      ? uniquePaths(paths, Infinity)
      : PathUtils.display((selection as string).trim());
  }
  if (
    value.recentFolders.length > MAX_RECENT_STORAGE_FOLDERS ||
    value.recentFolders.some(path => typeof path !== 'string' || !path.trim())
  ) {
    throw new Error('Invalid recent storage folders');
  }
  const recentFolders = uniquePaths(value.recentFolders);
  if (recentFolders.length !== value.recentFolders.length) {
    throw new Error('Duplicate recent storage folders');
  }
  const duplicateFileProtectedPaths =
    value.schemaVersion === 3 && Array.isArray(value.duplicateFileProtectedPaths)
      ? uniquePaths(value.duplicateFileProtectedPaths, Infinity)
      : [];
  if (
    value.schemaVersion === 3 &&
    (!Array.isArray(value.duplicateFileProtectedPaths) ||
      duplicateFileProtectedPaths.length !== value.duplicateFileProtectedPaths.length)
  ) {
    throw new Error('Invalid duplicate-file protected paths');
  }
  return {
    schemaVersion: 3,
    selectedPaths,
    recentFolders,
    duplicateFileProtectedPaths,
  };
}
export function addRecentFolder(folders: readonly string[], path: string): string[] {
  return uniquePaths([path, ...folders]);
}
export function removePath(paths: readonly string[], path: string): string[] {
  const removedKey = PathUtils.comparisonKey(path);
  return paths.filter(item => PathUtils.comparisonKey(item) !== removedKey);
}
export function empty(): StorageScopePreferences {
  return { schemaVersion: 3, selectedPaths: {}, recentFolders: [], duplicateFileProtectedPaths: [] };
}
function uniquePaths(values: readonly unknown[], limit = MAX_RECENT_STORAGE_FOLDERS): string[] {
  const keys = new Set<string>();
  const paths: string[] = [];
  for (const value of values) {
    if (typeof value !== 'string' || !value.trim()) continue;
    const path = PathUtils.display(value.trim());
    const key = PathUtils.comparisonKey(path);
    if (!key || keys.has(key)) continue;
    keys.add(key);
    paths.push(path);
    if (paths.length === limit) break;
  }
  return paths;
}
function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
function hasExactKeys<const Keys extends readonly string[]>(value: unknown, expectedKeys: Keys): boolean {
  if (!isRecord(value)) return false;
  const actualKeys = Object.keys(value);
  return actualKeys.length === expectedKeys.length && expectedKeys.every(key => actualKeys.includes(key));
}

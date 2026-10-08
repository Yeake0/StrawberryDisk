import {
  MAX_SCAN_EXCLUDED_FOLDERS,
  MAX_SCAN_EXCLUDED_NAMES,
  type ScanExcludedName,
  type ScanNameExclusion,
  SCAN_EXCLUSION_PREFERENCES_SCHEMA_VERSION,
  SCAN_EXCLUSION_SCOPES,
  type ScanExcludedFolder,
  type ScanExclusionPreferences,
  type ScanExclusionScope,
} from '@/lib/models/storage-scan';
import * as PathUtils from '@/lib/utils/path';

export type ScanExclusionPreferenceErrorCode = 'schemaVersionMismatch' | 'foldersInvalid' | 'namesInvalid';

export class ScanExclusionPreferenceError extends Error {
  constructor(readonly code: ScanExclusionPreferenceErrorCode) {
    super(`Invalid scan-exclusion preferences: ${code}`);
    this.name = 'ScanExclusionPreferenceError';
  }
}

const scopeOrder = Object.values(SCAN_EXCLUSION_SCOPES);

export function empty(): ScanExclusionPreferences {
  return { schemaVersion: SCAN_EXCLUSION_PREFERENCES_SCHEMA_VERSION, folders: [], names: [] };
}

export function parse(value: unknown): ScanExclusionPreferences {
  if (
    !isRecord(value) ||
    (value.schemaVersion !== 2 && value.schemaVersion !== SCAN_EXCLUSION_PREFERENCES_SCHEMA_VERSION)
  ) {
    throw new ScanExclusionPreferenceError('schemaVersionMismatch');
  }
  if (!Array.isArray(value.folders) || value.folders.length > MAX_SCAN_EXCLUDED_FOLDERS) {
    throw new ScanExclusionPreferenceError('foldersInvalid');
  }

  const seen = new Set<string>();
  const folders = value.folders.map(value => {
    if (!isRecord(value) || typeof value.path !== 'string' || !value.path.trim() || !Array.isArray(value.scopes)) {
      throw new ScanExclusionPreferenceError('foldersInvalid');
    }
    const path = PathUtils.display(value.path.trim());
    const key = PathUtils.comparisonKey(path);
    if (!key || seen.has(key) || !value.scopes.length || value.scopes.length > scopeOrder.length) {
      throw new ScanExclusionPreferenceError('foldersInvalid');
    }
    const requestedScopes = value.scopes;
    const scopes = scopeOrder.filter(scope => requestedScopes.includes(scope));
    if (scopes.length !== requestedScopes.length) throw new ScanExclusionPreferenceError('foldersInvalid');
    seen.add(key);
    return { path, scopes };
  });

  // Version 2 contains paths only. Preserve their scopes without enabling new rules.
  const rawNames = value.schemaVersion === 2 ? [] : value.names;
  if (!Array.isArray(rawNames) || rawNames.length > MAX_SCAN_EXCLUDED_NAMES) {
    throw new ScanExclusionPreferenceError('namesInvalid');
  }
  const seenNames = new Set<string>();
  const names: ScanExcludedName[] = rawNames.map(item => {
    if (
      !isRecord(item) ||
      typeof item.name !== 'string' ||
      !validExcludedName(item.name) ||
      (item.kind !== 'file' && item.kind !== 'folder') ||
      !Array.isArray(item.scopes)
    ) {
      throw new ScanExclusionPreferenceError('namesInvalid');
    }
    const key = `${item.kind}:${item.name}`;
    const scopes = scopeOrder.filter(scope => (item.scopes as unknown[]).includes(scope));
    if (seenNames.has(key) || !scopes.length || scopes.length !== item.scopes.length) {
      throw new ScanExclusionPreferenceError('namesInvalid');
    }
    seenNames.add(key);
    return { name: item.name, kind: item.kind, scopes };
  });
  return { schemaVersion: SCAN_EXCLUSION_PREFERENCES_SCHEMA_VERSION, folders, names };
}

/** The first shared schema affected both file scans, not space analysis. */
export function fromSharedV1(value: unknown): ScanExclusionPreferences {
  return fromLegacyList(value, [SCAN_EXCLUSION_SCOPES.largeFiles, SCAN_EXCLUSION_SCOPES.duplicateFiles]);
}

/** The original large-file key must not silently change another module's behavior. */
export function fromLargeFileV1(value: unknown): ScanExclusionPreferences {
  return fromLegacyList(value, [SCAN_EXCLUSION_SCOPES.largeFiles]);
}

function fromLegacyList(value: unknown, scopes: ScanExclusionScope[]): ScanExclusionPreferences {
  if (!isRecord(value) || value.schemaVersion !== 1 || !Array.isArray(value.excludedFolders)) {
    throw new ScanExclusionPreferenceError('schemaVersionMismatch');
  }
  return parse({
    schemaVersion: SCAN_EXCLUSION_PREFERENCES_SCHEMA_VERSION,
    folders: value.excludedFolders.map(path => ({ path, scopes })),
    names: [],
  });
}

export function pathsForScope(folders: readonly ScanExcludedFolder[], scope: ScanExclusionScope): string[] {
  return PathUtils.collapseOverlappingRoots(
    folders.filter(folder => folder.scopes.includes(scope)).map(folder => folder.path)
  );
}

export function sameExcludedFolders(left: readonly string[], right: readonly string[]): boolean {
  const leftKeys = left.map(PathUtils.comparisonKey).sort();
  const rightKeys = right.map(PathUtils.comparisonKey).sort();
  return leftKeys.length === rightKeys.length && leftKeys.every((key, index) => key === rightKeys[index]);
}

/** A result hint describes possible scan coverage, not whether files were actually skipped. */
export function hasExcludedFolderInScanRoots(roots: readonly string[], excludedFolders: readonly string[]): boolean {
  const rootKeys = roots.map(PathUtils.comparisonKey);
  return excludedFolders.some(folder => {
    const folderKey = PathUtils.comparisonKey(folder);
    return rootKeys.some(
      rootKey => PathUtils.isSameOrChildKey(folderKey, rootKey) || PathUtils.isSameOrChildKey(rootKey, folderKey)
    );
  });
}

export function errorCode(error: unknown): ScanExclusionPreferenceErrorCode | 'unexpected' {
  return error instanceof ScanExclusionPreferenceError ? error.code : 'unexpected';
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/** Exact, case-sensitive names have identical behavior on every supported platform. */
export function validExcludedName(name: string): boolean {
  return Boolean(
    name &&
    name === name.trim() &&
    name !== '.' &&
    name !== '..' &&
    new TextEncoder().encode(name).length <= 255 &&
    !/[\\/:*?"<>|\p{Cc}]/u.test(name)
  );
}

export function namesForScope(names: readonly ScanExcludedName[], scope: ScanExclusionScope): ScanNameExclusion[] {
  return names.filter(item => item.scopes.includes(scope)).map(({ name, kind }) => ({ name, kind }));
}

export function sameExcludedNames(left: readonly ScanNameExclusion[], right: readonly ScanNameExclusion[]): boolean {
  const keys = (items: readonly ScanNameExclusion[]) => items.map(item => `${item.kind}:${item.name}`).sort();
  const a = keys(left),
    b = keys(right);
  return a.length === b.length && a.every((key, index) => key === b[index]);
}

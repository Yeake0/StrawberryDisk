export const SCAN_EXCLUSION_PREFERENCES_SCHEMA_VERSION = 3;
export const MAX_SCAN_EXCLUDED_FOLDERS = 50;
export const MAX_SCAN_EXCLUDED_NAMES = 50;

export interface ScanNameExclusion {
  name: string;
  kind: 'file' | 'folder';
}

export interface ScanExcludedName extends ScanNameExclusion {
  scopes: ScanExclusionScope[];
}

export const SCAN_EXCLUSION_SCOPES = {
  cleanup: 'cleanup',
  largeFiles: 'largeFiles',
  duplicateFiles: 'duplicateFiles',
  analysis: 'analysis',
} as const;

export type ScanExclusionScope = (typeof SCAN_EXCLUSION_SCOPES)[keyof typeof SCAN_EXCLUSION_SCOPES];

export interface ScanExcludedFolder {
  path: string;
  scopes: ScanExclusionScope[];
}

export interface ScanExclusionPreferences {
  schemaVersion: typeof SCAN_EXCLUSION_PREFERENCES_SCHEMA_VERSION;
  folders: ScanExcludedFolder[];
  names: ScanExcludedName[];
}

export const STORAGE_SCOPE_IDS = {
  analysis: 'analysis',
  largeFiles: 'large-files',
  duplicateFiles: 'duplicate-files',
} as const;

export type StorageScopeId = (typeof STORAGE_SCOPE_IDS)[keyof typeof STORAGE_SCOPE_IDS];

export const MAX_RECENT_STORAGE_FOLDERS = 8;

export interface StorageScopePreferences {
  /** Missing version denotes the original single-selection document; reads migrate older versions to version 3. */
  schemaVersion?: 1 | 2 | 3;
  selectedPaths: Partial<Record<StorageScopeId, string | string[]>>;
  recentFolders: string[];
  /** Duplicate-file protection is scoped here because it belongs to a selected scan location. */
  duplicateFileProtectedPaths?: string[];
}

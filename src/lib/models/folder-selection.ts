/** Version 1 of the directory-entry resolution IPC response. */
export interface FolderSelectionOutcome {
  schemaVersion: 1;
  directories: ResolvedDirectory[];
  rejectedCount: number;
  redirectedCount: number;
}

export const MAX_DIRECTORY_ENTRIES_PER_REQUEST = 64;

export interface ResolvedDirectory {
  requestedPath: string;
  path: string;
}

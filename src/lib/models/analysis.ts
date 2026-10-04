export const ANALYSIS_RESULT_CACHE_LIMIT = 80;
export const ANALYSIS_CHART_MAX_DEPTH = 6;
export const ANALYSIS_REMAINDER_SCHEMA_VERSION = 2;

export const ANALYSIS_VIEW_IDS = {
  treemap: 'treemap',
  sunburst: 'sunburst',
} as const;

export type AnalysisViewId = (typeof ANALYSIS_VIEW_IDS)[keyof typeof ANALYSIS_VIEW_IDS];

export const ANALYSIS_VIEW_PREFERENCES_SCHEMA_VERSION = 1;

export interface AnalysisViewPreferences {
  schemaVersion: typeof ANALYSIS_VIEW_PREFERENCES_SCHEMA_VERSION;
  viewMode: AnalysisViewId;
  treemapDepth: number;
  sunburstDepth: number;
}

export const ANALYSIS_SORT_KEYS = {
  name: 'name',
  bytes: 'bytes',
  fileCount: 'fileCount',
  modified: 'modified',
} as const;

export const TREEMAP_TILE_KINDS = {
  entry: 'entry',
  remainder: 'remainder',
} as const;

export interface DirectoryEntryInfo {
  name: string;
  path: string;
  bytes: number;
  fileCount: number;
  isDirectory: boolean;
  modifiedAtMs: number | null;
  contentFingerprint: string | null;
}

export interface AnalysisResult {
  scanId: number;
  root: string;
  scannedAtMs: number;
  totalBytes: number;
  /** Positive-byte direct children; absent in older cached results. */
  totalEntryCount?: number;
  skippedCount: number;
  truncated: boolean;
  entries: DirectoryEntryInfo[];
  directoryHierarchy?: AnalysisDirectoryNode[];
}

export interface AnalysisDirectoryNode {
  name: string;
  path: string;
  bytes: number;
  fileCount: number;
  /** Positive-byte direct children before projection; absent in older results. */
  totalEntryCount?: number;
  children: AnalysisDirectoryNode[];
  /** Bounded read-only files; absent in older in-memory scan results. */
  files?: DirectoryEntryInfo[];
}

export interface AnalysisRemainderSelection {
  parentPath: string;
  bytes: number;
  visiblePaths: string[];
}

export interface AnalysisRemainderPage {
  snapshotId: number;
  schemaVersion: typeof ANALYSIS_REMAINDER_SCHEMA_VERSION;
  parentPath: string;
  totalBytes: number;
  totalCount: number;
  entries: DirectoryEntryInfo[];
  nextOffset: number | null;
}

export interface AnalysisDeleteResult {
  requiresRescan: boolean;
  removedPath: string;
  releasedBytes: number;
  removedFileCount: number;
}

interface TreemapTileRect {
  left: number;
  top: number;
  width: number;
  height: number;
}

export type TreemapTile = TreemapTileRect &
  (
    | {
        kind: typeof TREEMAP_TILE_KINDS.entry;
        entry: DirectoryEntryInfo;
        bytes: number;
      }
    | {
        kind: typeof TREEMAP_TILE_KINDS.remainder;
        entry: null;
        bytes: number;
        entryCount: number | null;
      }
  );

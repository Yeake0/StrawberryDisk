import { BYTE_SIZE_UNITS, type ByteSizePreset } from '@/lib/models/byte-size';

export const DUPLICATE_FILE_MINIMUM_PRESETS = [
  { amount: 200, unit: BYTE_SIZE_UNITS.kilobytes },
  { amount: 1, unit: BYTE_SIZE_UNITS.megabytes },
  { amount: 10, unit: BYTE_SIZE_UNITS.megabytes },
  { amount: 100, unit: BYTE_SIZE_UNITS.megabytes },
  { amount: 500, unit: BYTE_SIZE_UNITS.megabytes },
] as const satisfies readonly ByteSizePreset[];

export const DEFAULT_DUPLICATE_FILE_MINIMUM_PRESET = DUPLICATE_FILE_MINIMUM_PRESETS[0];
export const DUPLICATE_RESULT_PAGE_SIZE = 40;
export const DUPLICATE_GROUP_RENDER_BATCH_SIZE = 40;
export const DUPLICATE_ENTRY_RENDER_BATCH_SIZE = 80;

export const DUPLICATE_KEEPER_RULE_IDS = {
  shortestPath: 'shortestPath',
  shortestName: 'shortestName',
  oldestModified: 'oldestModified',
  newestModified: 'newestModified',
} as const;

export const DEFAULT_DUPLICATE_KEEPER_RULE = DUPLICATE_KEEPER_RULE_IDS.shortestPath;

export type DuplicateKeeperRuleId = (typeof DUPLICATE_KEEPER_RULE_IDS)[keyof typeof DUPLICATE_KEEPER_RULE_IDS];

export const DUPLICATE_SCAN_LOCATION_MODES = {
  cleanable: 'cleanable',
  protected: 'protected',
} as const;

export type DuplicateScanLocationMode =
  (typeof DUPLICATE_SCAN_LOCATION_MODES)[keyof typeof DUPLICATE_SCAN_LOCATION_MODES];

export interface DuplicateScanLocation {
  path: string;
  mode: DuplicateScanLocationMode;
}

export const DUPLICATE_ENTRY_DELETE_POLICIES = {
  cleanable: 'cleanable',
  protected: 'protected',
} as const;

export type DuplicateEntryDeletePolicy =
  (typeof DUPLICATE_ENTRY_DELETE_POLICIES)[keyof typeof DUPLICATE_ENTRY_DELETE_POLICIES];

export interface DuplicateFileEntry {
  name: string;
  path: string;
  parentPath: string;
  bytes: number;
  allocatedBytes: number;
  modifiedAtMs: number | null;
  deletePolicy: DuplicateEntryDeletePolicy;
}

export const DUPLICATE_GROUP_KINDS = {
  file: 'file',
  directory: 'directory',
} as const;

export type DuplicateGroupKind = (typeof DUPLICATE_GROUP_KINDS)[keyof typeof DUPLICATE_GROUP_KINDS];

export interface DuplicateGroup {
  id: string;
  /** Opaque exact-match proof token; it is not always a reusable file checksum. */
  hash: string;
  kind: DuplicateGroupKind;
  bytesPerFile: number;
  fileCountPerEntry: number;
  reclaimableBytes: number;
  entries: DuplicateFileEntry[];
}

export interface DuplicateFilesResult {
  scanId: number;
  roots: string[];
  protectedRoots: string[];
  scannedAtMs: number;
  scannedFileCount: number;
  skippedCount: number;
  duplicateFileCount: number;
  totalDuplicateBytes: number;
  reclaimableBytes: number;
  totalGroupCount: number;
  returnedGroupCount: number;
  truncated: boolean;
  groups: DuplicateGroup[];
}

export interface DuplicateGroupBatch {
  operationId: number;
  sequence: number;
  groups: DuplicateGroup[];
  foundGroupCount: number;
  foundFileCount: number;
  foundTotalBytes: number;
  foundReclaimableBytes: number;
  elapsedMs: number;
}

export interface DuplicateGroupPage {
  scanId: number;
  offset: number;
  nextOffset: number | null;
  totalCount: number;
  groups: DuplicateGroup[];
}

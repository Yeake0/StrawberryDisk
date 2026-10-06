import { describe, expect, it } from 'vitest';

import { ANALYSIS_SORT_KEYS, type DirectoryEntryInfo } from '@/lib/models/analysis';
import { SORT_DIRECTIONS } from '@/lib/models/sort';

import * as AnalysisEntryUtils from './analysis-entry';

const entries: DirectoryEntryInfo[] = [
  {
    name: 'Folder 10',
    path: '/Folder 10',
    bytes: 100,
    fileCount: 5,
    isDirectory: true,
    modifiedAtMs: 20,
    contentFingerprint: null,
  },
  {
    name: 'Folder 2',
    path: '/Folder 2',
    bytes: 300,
    fileCount: 2,
    isDirectory: true,
    modifiedAtMs: null,
    contentFingerprint: null,
  },
];

describe('analysis entry utilities', () => {
  it('sorts by name, bytes, file count, and modification time', () => {
    expect(AnalysisEntryUtils.sort(entries, ANALYSIS_SORT_KEYS.name, SORT_DIRECTIONS.ascending)).toEqual([
      entries[1],
      entries[0],
    ]);
    expect(AnalysisEntryUtils.sort(entries, ANALYSIS_SORT_KEYS.bytes, SORT_DIRECTIONS.descending)).toEqual([
      entries[1],
      entries[0],
    ]);
    expect(AnalysisEntryUtils.sort(entries, ANALYSIS_SORT_KEYS.fileCount, SORT_DIRECTIONS.ascending)).toEqual([
      entries[1],
      entries[0],
    ]);
    expect(AnalysisEntryUtils.sort(entries, ANALYSIS_SORT_KEYS.modified, SORT_DIRECTIONS.ascending)).toEqual([
      entries[1],
      entries[0],
    ]);
  });
});

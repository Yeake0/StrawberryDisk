import { describe, expect, it } from 'vitest';

import { LARGE_FILE_SORT_KEYS, type LargeFileEntry } from '@/lib/models/large-file';
import { SORT_DIRECTIONS } from '@/lib/models/sort';

import * as LargeFileEntryUtils from './large-file-entry';

const entries: LargeFileEntry[] = [
  { name: 'Archive 10', path: '/data/10', parentPath: '/data', bytes: 200, modifiedAtMs: 20 },
  { name: 'Archive 2', path: '/data/2', parentPath: '/data', bytes: 100, modifiedAtMs: null },
  { name: 'Archive 3', path: '/data/3', parentPath: '/data', bytes: 300, modifiedAtMs: 10 },
];

describe('large file entry utilities', () => {
  it('sorts every supported column without mutating the source', () => {
    expect(LargeFileEntryUtils.sorted(entries, LARGE_FILE_SORT_KEYS.name, SORT_DIRECTIONS.ascending)).toEqual([
      entries[1],
      entries[2],
      entries[0],
    ]);
    expect(LargeFileEntryUtils.sorted(entries, LARGE_FILE_SORT_KEYS.bytes, SORT_DIRECTIONS.descending)).toEqual([
      entries[2],
      entries[0],
      entries[1],
    ]);
    expect(LargeFileEntryUtils.sorted(entries, LARGE_FILE_SORT_KEYS.modified, SORT_DIRECTIONS.ascending)).toEqual([
      entries[1],
      entries[2],
      entries[0],
    ]);
    expect(entries.map(entry => entry.path)).toEqual(['/data/10', '/data/2', '/data/3']);
  });

  it('derives selected entries and aggregate checkbox state', () => {
    expect(LargeFileEntryUtils.selectedEntries(entries, ['/data/2', '/missing'])).toEqual([entries[1]]);
    expect(LargeFileEntryUtils.selectionState(entries, new Set(['/data/2']))).toEqual({
      checked: false,
      indeterminate: true,
    });
    expect(LargeFileEntryUtils.selectionState(entries, new Set(entries.map(entry => entry.path)))).toEqual({
      checked: true,
      indeterminate: false,
    });
    expect(LargeFileEntryUtils.selectionState([], new Set())).toEqual({ checked: false, indeterminate: false });
  });

  it('adds and removes target paths while preserving unrelated selections', () => {
    expect(LargeFileEntryUtils.updateSelection(['/keep'], ['/a', '/b'], true)).toEqual(['/keep', '/a', '/b']);
    expect(LargeFileEntryUtils.updateSelection(['/keep', '/a', '/b'], ['/a', '/b'], false)).toEqual(['/keep']);
  });
});

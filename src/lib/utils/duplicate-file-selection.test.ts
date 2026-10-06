import { describe, expect, it } from 'vitest';

import {
  DUPLICATE_ENTRY_DELETE_POLICIES,
  DUPLICATE_GROUP_KINDS,
  DUPLICATE_KEEPER_RULE_IDS,
} from '@/lib/models/duplicate-file';
import type { DuplicateGroup } from '@/lib/models/duplicate-file';

import * as DuplicateFileSelectionUtils from './duplicate-file-selection';

const group: DuplicateGroup = {
  id: 'group-1',
  hash: 'hash-1',
  kind: DUPLICATE_GROUP_KINDS.file,
  bytesPerFile: 1024,
  fileCountPerEntry: 1,
  reclaimableBytes: 1024,
  entries: [
    {
      name: 'report.pdf',
      path: '/docs/report.pdf',
      parentPath: '/docs',
      bytes: 1024,
      allocatedBytes: 1024,
      modifiedAtMs: 1,
      deletePolicy: DUPLICATE_ENTRY_DELETE_POLICIES.cleanable,
    },
    {
      name: 'report-copy.pdf',
      path: '/archive/reports/report-copy.pdf',
      parentPath: '/archive/reports',
      bytes: 1024,
      allocatedBytes: 1024,
      modifiedAtMs: 2,
      deletePolicy: DUPLICATE_ENTRY_DELETE_POLICIES.cleanable,
    },
  ],
};

describe('duplicate file selection utilities', () => {
  it('selects removable copies while preserving selections from other groups', () => {
    expect(
      DuplicateFileSelectionUtils.toggleGroupCopies(
        ['/other/group-copy.pdf'],
        group,
        DUPLICATE_KEEPER_RULE_IDS.shortestPath
      )
    ).toEqual(['/other/group-copy.pdf', '/archive/reports/report-copy.pdf']);
  });

  it('clears only the active group when its suggested selection is already applied', () => {
    expect(
      DuplicateFileSelectionUtils.toggleGroupCopies(
        ['/other/group-copy.pdf', '/archive/reports/report-copy.pdf'],
        group,
        DUPLICATE_KEEPER_RULE_IDS.shortestPath
      )
    ).toEqual(['/other/group-copy.pdf']);
  });

  it('keeps every protected copy and selects all cleanable copies in the group', () => {
    const protectedGroup: DuplicateGroup = {
      ...group,
      entries: [
        { ...group.entries[0]!, deletePolicy: DUPLICATE_ENTRY_DELETE_POLICIES.protected },
        { ...group.entries[1]!, deletePolicy: DUPLICATE_ENTRY_DELETE_POLICIES.cleanable },
      ],
    };

    expect(
      DuplicateFileSelectionUtils.suggestedPaths([protectedGroup], DUPLICATE_KEEPER_RULE_IDS.shortestPath)
    ).toEqual(['/archive/reports/report-copy.pdf']);
    expect(
      DuplicateFileSelectionUtils.updateEntrySelection([], protectedGroup.entries[0]!, protectedGroup, true)
    ).toEqual([]);
  });

  it('never exposes protected entries as selected cleanup candidates', () => {
    const protectedGroup: DuplicateGroup = {
      ...group,
      entries: group.entries.map(entry => ({
        ...entry,
        deletePolicy: DUPLICATE_ENTRY_DELETE_POLICIES.protected,
      })),
    };

    expect(
      DuplicateFileSelectionUtils.selectedEntries(
        [protectedGroup],
        protectedGroup.entries.map(entry => entry.path)
      )
    ).toEqual([]);
  });
});

import { describe, expect, it } from 'vitest';

import {
  DUPLICATE_ENTRY_DELETE_POLICIES,
  DUPLICATE_GROUP_KINDS,
  type DuplicateFilesResult,
} from '@/lib/models/duplicate-file';
import type { LargeFilesResult } from '@/lib/models/large-file';
import * as DuplicateFileResultUtils from '@/lib/utils/duplicate-file-result';
import * as LargeFileResultUtils from '@/lib/utils/large-file-result';

describe('permanent-delete result synchronization', () => {
  it('removes only successful large-file paths and updates totals', () => {
    const result: LargeFilesResult = {
      scanId: 3,
      roots: ['/fixture'],
      scannedAtMs: 1,
      scanMode: 'complete',
      minimumBytes: 1,
      totalBytes: 300,
      totalCount: 2,
      returnedCount: 2,
      truncated: false,
      skippedCount: 0,
      entries: [
        { name: 'deleted.bin', path: '/fixture/deleted.bin', parentPath: '/fixture', bytes: 100, modifiedAtMs: 1 },
        { name: 'failed.bin', path: '/fixture/failed.bin', parentPath: '/fixture', bytes: 200, modifiedAtMs: 1 },
      ],
    };

    const updated = LargeFileResultUtils.removePaths(result, new Set(['/fixture/deleted.bin']), 100);

    expect(updated.entries.map(entry => entry.path)).toEqual(['/fixture/failed.bin']);
    expect(updated.totalBytes).toBe(200);
    expect(updated.totalCount).toBe(1);
  });

  it('removes a resolved duplicate group while retaining failed groups', () => {
    const result: DuplicateFilesResult = {
      scanId: 7,
      roots: ['/fixture'],
      protectedRoots: [],
      scannedAtMs: 1,
      scannedFileCount: 4,
      skippedCount: 0,
      duplicateFileCount: 4,
      totalDuplicateBytes: 400,
      reclaimableBytes: 200,
      totalGroupCount: 2,
      returnedGroupCount: 2,
      truncated: false,
      groups: [
        {
          id: 'deleted-group',
          hash: 'deleted-hash',
          kind: DUPLICATE_GROUP_KINDS.file,
          bytesPerFile: 100,
          fileCountPerEntry: 1,
          reclaimableBytes: 100,
          entries: [
            {
              name: 'a.bin',
              path: '/fixture/a.bin',
              parentPath: '/fixture',
              bytes: 100,
              allocatedBytes: 100,
              modifiedAtMs: 1,
              deletePolicy: DUPLICATE_ENTRY_DELETE_POLICIES.cleanable,
            },
            {
              name: 'b.bin',
              path: '/fixture/b.bin',
              parentPath: '/fixture',
              bytes: 100,
              allocatedBytes: 100,
              modifiedAtMs: 1,
              deletePolicy: DUPLICATE_ENTRY_DELETE_POLICIES.cleanable,
            },
          ],
        },
        {
          id: 'failed-group',
          hash: 'failed-hash',
          kind: DUPLICATE_GROUP_KINDS.file,
          bytesPerFile: 100,
          fileCountPerEntry: 1,
          reclaimableBytes: 100,
          entries: [
            {
              name: 'c.bin',
              path: '/fixture/c.bin',
              parentPath: '/fixture',
              bytes: 100,
              allocatedBytes: 100,
              modifiedAtMs: 1,
              deletePolicy: DUPLICATE_ENTRY_DELETE_POLICIES.cleanable,
            },
            {
              name: 'd.bin',
              path: '/fixture/d.bin',
              parentPath: '/fixture',
              bytes: 100,
              allocatedBytes: 100,
              modifiedAtMs: 1,
              deletePolicy: DUPLICATE_ENTRY_DELETE_POLICIES.cleanable,
            },
          ],
        },
      ],
    };

    const updated = DuplicateFileResultUtils.removePaths(result, new Set(['/fixture/a.bin']));

    expect(updated.groups.map(group => group.id)).toEqual(['failed-group']);
    expect(updated.duplicateFileCount).toBe(2);
    expect(updated.totalDuplicateBytes).toBe(200);
    expect(updated.reclaimableBytes).toBe(100);
  });

  it('recomputes duplicate totals from physical storage after removal', () => {
    const result: DuplicateFilesResult = {
      scanId: 8,
      roots: ['/fixture'],
      protectedRoots: [],
      scannedAtMs: 1,
      scannedFileCount: 3,
      skippedCount: 0,
      duplicateFileCount: 3,
      totalDuplicateBytes: 24,
      reclaimableBytes: 20,
      totalGroupCount: 1,
      returnedGroupCount: 1,
      truncated: false,
      groups: [
        {
          id: 'sparse-group',
          hash: 'sparse-hash',
          kind: DUPLICATE_GROUP_KINDS.file,
          bytesPerFile: 1024,
          fileCountPerEntry: 1,
          reclaimableBytes: 20,
          entries: [
            {
              name: 'a.bin',
              path: '/fixture/a.bin',
              parentPath: '/fixture',
              bytes: 1024,
              allocatedBytes: 4,
              modifiedAtMs: 1,
              deletePolicy: DUPLICATE_ENTRY_DELETE_POLICIES.cleanable,
            },
            {
              name: 'b.bin',
              path: '/fixture/b.bin',
              parentPath: '/fixture',
              bytes: 1024,
              allocatedBytes: 8,
              modifiedAtMs: 1,
              deletePolicy: DUPLICATE_ENTRY_DELETE_POLICIES.cleanable,
            },
            {
              name: 'c.bin',
              path: '/fixture/c.bin',
              parentPath: '/fixture',
              bytes: 1024,
              allocatedBytes: 12,
              modifiedAtMs: 1,
              deletePolicy: DUPLICATE_ENTRY_DELETE_POLICIES.cleanable,
            },
          ],
        },
      ],
    };

    const updated = DuplicateFileResultUtils.removePaths(result, new Set(['/fixture/c.bin']));

    expect(updated.totalDuplicateBytes).toBe(12);
    expect(updated.reclaimableBytes).toBe(8);
  });
});

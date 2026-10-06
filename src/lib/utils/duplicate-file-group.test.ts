import { describe, expect, it } from 'vitest';

import {
  DUPLICATE_ENTRY_DELETE_POLICIES,
  DUPLICATE_GROUP_KINDS,
  type DuplicateGroup,
} from '@/lib/models/duplicate-file';
import { FILE_CATEGORY_IDS } from '@/lib/models/file-category';
import * as DuplicateFileGroupUtils from '@/lib/utils/duplicate-file-group';

function group(id: string, hash: string, name: string, parentPath: string): DuplicateGroup {
  return {
    id,
    hash,
    kind: DUPLICATE_GROUP_KINDS.file,
    bytesPerFile: 10,
    fileCountPerEntry: 1,
    reclaimableBytes: 10,
    entries: [
      {
        name,
        path: `${parentPath}/${name}`,
        parentPath,
        bytes: 10,
        allocatedBytes: 10,
        modifiedAtMs: null,
        deletePolicy: DUPLICATE_ENTRY_DELETE_POLICIES.cleanable,
      },
      {
        name,
        path: `/copy/${id}/${name}`,
        parentPath: `/copy/${id}`,
        bytes: 10,
        allocatedBytes: 10,
        modifiedAtMs: null,
        deletePolicy: DUPLICATE_ENTRY_DELETE_POLICIES.cleanable,
      },
    ],
  };
}

describe('DuplicateFileGroupUtils', () => {
  it('computes reclaimable space from physical allocation', () => {
    const fixture = group('sparse', 'sparse-hash', 'sparse.bin', '/Downloads');
    fixture.entries[0]!.allocatedBytes = 4;
    fixture.entries[1]!.allocatedBytes = 12;

    expect(DuplicateFileGroupUtils.totalAllocatedBytes(fixture.entries)).toBe(16);
    expect(DuplicateFileGroupUtils.maximumReclaimableBytes(fixture.entries)).toBe(12);
  });

  it('counts only cleanable copies when a group contains protected entries', () => {
    const fixture = group('protected', 'protected-hash', 'report.pdf', '/Work');
    fixture.entries[0]!.allocatedBytes = 12;
    fixture.entries[0]!.deletePolicy = DUPLICATE_ENTRY_DELETE_POLICIES.protected;
    fixture.entries[1]!.allocatedBytes = 4;

    expect(DuplicateFileGroupUtils.maximumReclaimableBytes(fixture.entries)).toBe(4);
  });

  it('keeps unique file names concise', () => {
    const fixture = group('one', '1111111111111111', 'archive.zip', '/Downloads');

    expect(DuplicateFileGroupUtils.displayLabels([fixture]).get(fixture.id)).toBe('archive.zip');
  });

  it('uses the parent directory to distinguish same-name content groups', () => {
    const first = group('one', '1111111111111111', 'Google Chrome Framework', '/Versions/150.0.1');
    const second = group('two', '2222222222222222', 'Google Chrome Framework', '/Versions/151.0.1');
    const labels = DuplicateFileGroupUtils.displayLabels([first, second]);

    expect(labels.get(first.id)).toBe('Google Chrome Framework · 150.0.1');
    expect(labels.get(second.id)).toBe('Google Chrome Framework · 151.0.1');
  });

  it('uses the parent path when immediate parent labels still collide', () => {
    const first = group('one', '11111111aaaaaaaa', 'report.pdf', '/first/archive');
    const second = group('two', '22222222bbbbbbbb', 'report.pdf', '/second/archive');
    const labels = DuplicateFileGroupUtils.displayLabels([first, second]);

    expect(labels.get(first.id)).toBe('report.pdf · /first/archive');
    expect(labels.get(second.id)).toBe('report.pdf · /second/archive');
  });

  it('classifies mixed-content directory aggregates as other', () => {
    const fixture = {
      ...group('directory', 'directory:hash', 'archive', '/Downloads'),
      kind: DUPLICATE_GROUP_KINDS.directory,
      fileCountPerEntry: 12,
    };

    expect(DuplicateFileGroupUtils.category(fixture)).toBe(FILE_CATEGORY_IDS.other);
    expect(DuplicateFileGroupUtils.representedFileCount(fixture)).toBe(24);
  });

  it('keeps directory aggregate labels to the shared folder name', () => {
    const first = {
      ...group('directory-one', 'directory:first', 'bin', '/first/node_modules/package'),
      kind: DUPLICATE_GROUP_KINDS.directory,
    };
    const second = {
      ...group('directory-two', 'directory:second', 'bin', '/second/node_modules/package'),
      kind: DUPLICATE_GROUP_KINDS.directory,
    };
    const labels = DuplicateFileGroupUtils.displayLabels([first, second]);

    expect(labels.get(first.id)).toBe('bin');
    expect(labels.get(second.id)).toBe('bin');
  });
});

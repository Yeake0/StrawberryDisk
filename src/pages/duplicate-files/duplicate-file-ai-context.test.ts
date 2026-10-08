import { describe, expect, it } from 'vitest';
import { AI_DUPLICATE_COPY_LIMIT } from '@/lib/models/ai';
import type { DuplicateFileEntry, DuplicateGroup } from '@/lib/models/duplicate-file';
import { duplicateFileAiContext } from './duplicate-file-ai-context';

const entries: DuplicateFileEntry[] = Array.from({ length: 40 }, (_, index) => ({
  name: 'library.dll',
  path: `/App-${index}/library.dll`,
  parentPath: `/App-${index}`,
  bytes: 100,
  allocatedBytes: 80,
  modifiedAtMs: index,
  deletePolicy: index === 39 ? 'protected' : 'cleanable',
}));
const group: DuplicateGroup = {
  id: 'operational-id',
  hash: 'opaque-proof',
  kind: 'file',
  bytesPerFile: 100,
  fileCountPerEntry: 1,
  reclaimableBytes: 3120,
  entries,
};

describe('duplicate file explanation metadata', () => {
  it('always includes the requested copy and explicitly counts omitted peers', () => {
    const context = duplicateFileAiContext(group, entries[39]!, 'windows')!;
    expect(context.subject).toMatchObject({
      module: 'duplicateFiles',
      kind: 'file',
      target: { file: { path: entries[39]!.path }, deletePolicy: 'protected' },
      omittedCount: 39 - AI_DUPLICATE_COPY_LIMIT,
    });
    if (context.subject.module !== 'duplicateFiles') throw new Error('wrong subject');
    expect(context.subject.otherCopies).toHaveLength(AI_DUPLICATE_COPY_LIMIT);
    expect(context.subject.otherCopies.some(copy => copy.file.path === entries[39]!.path)).toBe(false);
    expect(JSON.stringify(context)).not.toMatch(/operational-id|opaque-proof|allocatedBytes|parentPath/);
  });

  it('includes protected peers before sampling cleanable copies', () => {
    const context = duplicateFileAiContext(group, entries[0]!, 'windows')!;
    if (context.subject.module !== 'duplicateFiles') throw new Error('wrong subject');
    expect(context.subject.otherCopies[0]).toMatchObject({
      file: { path: entries[39]!.path },
      deletePolicy: 'protected',
    });
  });

  it('takes protection from the current group rather than a stale clicked entry', () => {
    const context = duplicateFileAiContext(group, { ...entries[39]!, deletePolicy: 'cleanable' }, 'windows');
    expect(context?.subject).toMatchObject({ target: { deletePolicy: 'protected' } });
  });

  it('rejects a missing target or a group without another copy', () => {
    expect(duplicateFileAiContext(group, { ...entries[0]!, path: '/missing' }, 'windows')).toBeNull();
    expect(duplicateFileAiContext({ ...group, entries: [entries[0]!] }, entries[0]!, 'windows')).toBeNull();
  });

  it('preserves directory grouping with no inferred file count', () => {
    const context = duplicateFileAiContext(
      { ...group, kind: 'directory', entries: entries.slice(0, 2) },
      entries[0]!,
      'macos'
    );
    expect(context?.subject).toMatchObject({ kind: 'directory', omittedCount: 0 });
  });
});

import { AI_DUPLICATE_COPY_LIMIT, type AiContext, type AiDuplicateEntry } from '@/lib/models/ai';
import type { DuplicateFileEntry, DuplicateGroup } from '@/lib/models/duplicate-file';

function duplicateAiEntry(entry: DuplicateFileEntry): AiDuplicateEntry {
  return {
    file: { name: entry.name, path: entry.path, bytes: entry.bytes, modifiedAtMs: entry.modifiedAtMs },
    deletePolicy: entry.deletePolicy,
  };
}

export function duplicateFileAiContext(
  group: DuplicateGroup,
  entry: DuplicateFileEntry,
  platform: AiContext['platform']
): AiContext | null {
  const target = group.entries.find(copy => copy.path === entry.path);
  if (!target || group.entries.length < 2) return null;
  // Preserve known protection clues even when a very large group needs sampling.
  const others = group.entries.filter(copy => copy.path !== target.path);
  const otherCopies = [
    ...others.filter(copy => copy.deletePolicy === 'protected'),
    ...others.filter(copy => copy.deletePolicy !== 'protected'),
  ];
  return {
    schemaVersion: 2,
    platform,
    title: target.name,
    description: '',
    subject: {
      module: 'duplicateFiles',
      kind: group.kind,
      target: duplicateAiEntry(target),
      otherCopies: otherCopies.slice(0, AI_DUPLICATE_COPY_LIMIT).map(duplicateAiEntry),
      omittedCount: Math.max(0, otherCopies.length - AI_DUPLICATE_COPY_LIMIT),
    },
  };
}

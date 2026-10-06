import type { AiContext } from '@/lib/models/ai';
import type { LargeFileEntry } from '@/lib/models/large-file';

export function largeFileAiContext(entry: LargeFileEntry, platform: AiContext['platform']): AiContext {
  return {
    schemaVersion: 2,
    platform,
    title: entry.name,
    description: '',
    subject: {
      module: 'largeFiles',
      file: { name: entry.name, path: entry.path, bytes: entry.bytes, modifiedAtMs: entry.modifiedAtMs },
    },
  };
}

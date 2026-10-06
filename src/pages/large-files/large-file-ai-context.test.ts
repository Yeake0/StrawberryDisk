import { describe, expect, it } from 'vitest';
import { largeFileAiContext } from './large-file-ai-context';

describe('large file explanation metadata', () => {
  it('exposes only descriptive facts rather than the scan object or content', () => {
    const entry = {
      name: 'model.gguf',
      path: '/Models/model.gguf',
      parentPath: '/Models',
      bytes: 100,
      modifiedAtMs: null,
      content: 'private content',
      scanId: 10,
      hash: 'opaque-proof',
    };
    expect(largeFileAiContext(entry, 'macos')).toEqual({
      schemaVersion: 2,
      platform: 'macos',
      title: entry.name,
      description: '',
      subject: { module: 'largeFiles', file: { name: entry.name, path: entry.path, bytes: 100, modifiedAtMs: null } },
    });
  });
});

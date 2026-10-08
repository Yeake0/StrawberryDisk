import { open } from '@tauri-apps/plugin-dialog';
import { invoke } from '@tauri-apps/api/core';
import {
  MAX_DIRECTORY_ENTRIES_PER_REQUEST,
  type FolderSelectionOutcome,
  type ResolvedDirectory,
} from '@/lib/models/folder-selection';
import type { CommandError } from '@/lib/utils/error';
import * as PathUtils from '@/lib/utils/path';

/**
 * Coordinates native folder selection and directory validation. Native drag
 * events live in a separate adapter so stores can validate saved paths without
 * loading WebView-only integration code.
 */
export class FolderSelectionService {
  static async select(multiple: boolean, title: string, defaultPath?: string): Promise<string[]> {
    const selected = await open({ directory: true, multiple, title, defaultPath });
    if (!selected) return [];
    return Array.isArray(selected) ? selected : [selected];
  }

  /** Keeps input mappings so redirected Known Folders retain their labels. */
  static async resolveDirectories(paths: string[]): Promise<ResolvedDirectory[]> {
    if (!paths.length) return [];
    if (paths.length > MAX_DIRECTORY_ENTRIES_PER_REQUEST) {
      throw {
        code: 'invalidInput',
        details: { operation: 'filter_directory_paths', reason: 'folderSelectionLimitExceeded' },
        retryable: false,
      } satisfies CommandError;
    }
    const result = await invoke<FolderSelectionOutcome>('filter_directory_paths', { paths });
    if (result.schemaVersion !== 1) throw new Error('Unsupported directory resolution response');
    return result.directories;
  }

  /** Resolves explicit selections and reports failures instead of silently ignoring them. */
  static async filterExistingDirectories(paths: string[]): Promise<string[]> {
    const resolved = await this.resolveDirectories(paths);
    if (resolved.length < paths.length) {
      throw {
        code: 'invalidInput',
        details: { operation: 'filter_directory_paths', reason: 'folderUnavailable' },
        retryable: true,
      } satisfies CommandError;
    }
    const seen = new Set<string>();
    return resolved.flatMap(({ path }) => {
      const key = PathUtils.comparisonKey(path);
      if (seen.has(key)) return [];
      seen.add(key);
      return [path];
    });
  }
}

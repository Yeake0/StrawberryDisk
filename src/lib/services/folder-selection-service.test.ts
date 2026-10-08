import { beforeEach, describe, expect, it, vi } from 'vitest';

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }));

import { FolderSelectionService } from './folder-selection-service';
import { MAX_DIRECTORY_ENTRIES_PER_REQUEST } from '@/lib/models/folder-selection';
import { parseCommandErrorReason } from '@/lib/utils/error';

describe('directory entry resolution', () => {
  beforeEach(() => vi.clearAllMocks());

  it('keeps mappings but deduplicates targets for an explicit selection', async () => {
    invokeMock.mockResolvedValue({
      schemaVersion: 1,
      directories: [
        { requestedPath: 'C:\\Alias', path: '\\\\server\\share\\Folder' },
        { requestedPath: '\\\\SERVER\\share\\folder', path: '\\\\server\\share\\Folder' },
      ],
      rejectedCount: 0,
      redirectedCount: 1,
    });
    expect(await FolderSelectionService.filterExistingDirectories(['C:\\Alias', '\\\\SERVER\\share\\folder'])).toEqual([
      '\\\\server\\share\\Folder',
    ]);
  });

  it('reports rejected selections without returning a partial silent success', async () => {
    invokeMock.mockResolvedValue({ schemaVersion: 1, directories: [], rejectedCount: 1, redirectedCount: 0 });
    await expect(FolderSelectionService.filterExistingDirectories(['C:\\Missing'])).rejects.toMatchObject({
      code: 'invalidInput',
      details: { reason: 'folderUnavailable' },
    });
    expect(
      parseCommandErrorReason({ code: 'invalidInput', details: { reason: 'folderUnavailable' }, retryable: true })
    ).toBe('folderUnavailable');
  });

  it('does not report cancellation as a failure', async () => {
    expect(await FolderSelectionService.filterExistingDirectories([])).toEqual([]);
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it('rejects incompatible protocol versions', async () => {
    invokeMock.mockResolvedValue({ schemaVersion: 2, directories: [] });
    await expect(FolderSelectionService.resolveDirectories(['C:\\Folder'])).rejects.toThrow(
      'Unsupported directory resolution response'
    );
  });

  it('rejects oversized selections before invoking native path resolution', async () => {
    const paths = Array.from({ length: MAX_DIRECTORY_ENTRIES_PER_REQUEST + 1 }, (_, index) => `/folder-${index}`);
    const error = {
      code: 'invalidInput',
      details: { reason: 'folderSelectionLimitExceeded' },
      retryable: false,
    } as const;
    await expect(FolderSelectionService.resolveDirectories(paths)).rejects.toMatchObject(error);
    expect(parseCommandErrorReason(error)).toBe('folderSelectionLimitExceeded');
    expect(invokeMock).not.toHaveBeenCalled();
  });
});

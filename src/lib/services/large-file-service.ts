import type { ScanNameExclusion } from '@/lib/models/storage-scan';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import { EVENT_NAMES } from '@/lib/models/telemetry';
import type { LargeFileScanMode, LargeFilesResult } from '@/lib/models/large-file';
import type { TraversalProgress } from '@/lib/models/progress';

export class LargeFileService {
  static find(
    roots: string[],
    minimumBytes: number,
    scanMode: LargeFileScanMode,
    excludedFolders: string[],
    excludedNames: ScanNameExclusion[] = []
  ): Promise<LargeFilesResult> {
    return invoke<LargeFilesResult>('find_large_files', {
      roots,
      minimumBytes,
      scanMode,
      excludedPaths: excludedFolders,
      excludedNames,
    });
  }

  static filter(scanId: number, minimumBytes: number): Promise<LargeFilesResult> {
    return invoke<LargeFilesResult>('filter_large_files', { scanId, minimumBytes });
  }

  static listenProgress(handler: (progress: TraversalProgress) => void): Promise<UnlistenFn> {
    return listen<TraversalProgress>(EVENT_NAMES.largeFilesProgress, event => handler(event.payload));
  }

  static cancel(): Promise<void> {
    return invoke<void>('cancel_large_files');
  }
}

import type { ScanNameExclusion } from '@/lib/models/storage-scan';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import { EVENT_NAMES } from '@/lib/models/telemetry';
import { ANALYSIS_REMAINDER_SCHEMA_VERSION } from '@/lib/models/analysis';
import type {
  AnalysisDeleteResult,
  AnalysisRemainderPage,
  AnalysisRemainderSelection,
  AnalysisResult,
} from '@/lib/models/analysis';
import type { TraversalProgress } from '@/lib/models/progress';

export class AnalysisService {
  static async listRemainder(
    scanId: number,
    selection: AnalysisRemainderSelection,
    offset = 0,
    snapshotId: number | null = null
  ): Promise<AnalysisRemainderPage> {
    const page = await invoke<AnalysisRemainderPage>('list_analysis_remainder', {
      request: {
        schemaVersion: ANALYSIS_REMAINDER_SCHEMA_VERSION,
        scanId,
        parentPath: selection.parentPath,
        visiblePaths: selection.visiblePaths,
        expectedBytes: selection.bytes,
        offset,
        snapshotId,
      },
    });
    if (page.schemaVersion !== ANALYSIS_REMAINDER_SCHEMA_VERSION) {
      throw new Error('unsupported analysis remainder schema');
    }
    return page;
  }

  static releaseRemainder(snapshotId: number): Promise<void> {
    return invoke('release_analysis_remainder', { snapshotId });
  }

  static analyze(
    path: string | undefined,
    refresh: boolean,
    excludedFolders: string[],
    excludedNames: ScanNameExclusion[] = []
  ): Promise<AnalysisResult> {
    return invoke<AnalysisResult>('analyze_path', {
      path: path?.trim() || null,
      refresh,
      excludedPaths: excludedFolders,
      excludedNames,
    });
  }

  static listenProgress(handler: (progress: TraversalProgress) => void): Promise<UnlistenFn> {
    return listen<TraversalProgress>(EVENT_NAMES.analysisProgress, event => handler(event.payload));
  }

  static cancel(): Promise<void> {
    return invoke<void>('cancel_analysis');
  }

  static deletePermanently(scanId: number, selectedPath: string): Promise<AnalysisDeleteResult> {
    return invoke<AnalysisDeleteResult>('delete_analysis_entry_permanently', { scanId, selectedPath });
  }
}

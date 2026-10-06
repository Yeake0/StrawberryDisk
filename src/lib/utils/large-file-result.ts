import type { LargeFilesResult } from '@/lib/models/large-file';
export function removePaths(
  result: LargeFilesResult,
  removedPaths: ReadonlySet<string>,
  releasedBytes: number
): LargeFilesResult {
  const entries = result.entries.filter(entry => !removedPaths.has(entry.path));
  const totalCount = Math.max(0, result.totalCount - removedPaths.size);
  return {
    ...result,
    totalBytes: Math.max(0, result.totalBytes - releasedBytes),
    totalCount,
    returnedCount: entries.length,
    truncated: entries.length < totalCount,
    entries,
  };
}

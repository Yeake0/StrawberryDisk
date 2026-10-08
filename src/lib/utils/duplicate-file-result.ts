import type { DuplicateFilesResult } from '@/lib/models/duplicate-file';
import * as DuplicateFileGroupUtils from '@/lib/utils/duplicate-file-group';
export function removePaths(result: DuplicateFilesResult, removedPaths: ReadonlySet<string>): DuplicateFilesResult {
  const groups: DuplicateFilesResult['groups'] = [];
  let removedDuplicateFiles = 0;
  let removedDuplicateBytes = 0;
  let removedReclaimableBytes = 0;
  let removedGroups = 0;
  for (const group of result.groups) {
    const remainingEntries = group.entries.filter(entry => !removedPaths.has(entry.path));
    const remainingIsDuplicate = remainingEntries.length > 1;
    const previousRepresentedFileCount = DuplicateFileGroupUtils.representedFileCount(group);
    const remainingEntryCount = remainingIsDuplicate ? remainingEntries.length : 0;
    const remainingRepresentedFileCount = remainingEntryCount * group.fileCountPerEntry;
    const previousTotalBytes = DuplicateFileGroupUtils.totalAllocatedBytes(group.entries);
    const remainingTotalBytes = remainingIsDuplicate
      ? DuplicateFileGroupUtils.totalAllocatedBytes(remainingEntries)
      : 0;
    const remainingReclaimableBytes = DuplicateFileGroupUtils.maximumReclaimableBytes(remainingEntries);
    removedDuplicateFiles += previousRepresentedFileCount - remainingRepresentedFileCount;
    removedDuplicateBytes += previousTotalBytes - remainingTotalBytes;
    removedReclaimableBytes += group.reclaimableBytes - remainingReclaimableBytes;
    if (!remainingIsDuplicate) {
      removedGroups += 1;
      continue;
    }
    groups.push({
      ...group,
      entries: remainingEntries,
      reclaimableBytes: remainingReclaimableBytes,
    });
  }
  const totalGroupCount = Math.max(0, result.totalGroupCount - removedGroups);
  return {
    ...result,
    groups,
    duplicateFileCount: Math.max(0, result.duplicateFileCount - removedDuplicateFiles),
    totalDuplicateBytes: Math.max(0, result.totalDuplicateBytes - removedDuplicateBytes),
    reclaimableBytes: Math.max(0, result.reclaimableBytes - removedReclaimableBytes),
    totalGroupCount,
    returnedGroupCount: groups.length,
    truncated: groups.length < totalGroupCount,
  };
}

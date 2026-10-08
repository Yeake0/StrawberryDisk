import type { DiskInfo } from '@/lib/models/disk';
import type { AnalysisDirectoryNode, AnalysisResult, DirectoryEntryInfo } from '@/lib/models/analysis';
import * as PathUtils from '@/lib/utils/path';
export type AnalysisSiblingFolder = Pick<DirectoryEntryInfo, 'name' | 'path' | 'bytes'>;
export interface AnalysisBreadcrumb {
  label: string;
  path: string;
  siblings?: readonly AnalysisSiblingFolder[];
  siblingParentBytes?: number;
}
export function create(path: string, activeDisk: DiskInfo | null, localDiskLabel: string): AnalysisBreadcrumb[] {
  const normalized = PathUtils.display(path);
  if (!normalized) return [];
  const driveMatch = normalized.match(/^([A-Za-z]:)[\\/]*(.*)$/);
  if (driveMatch) {
    const drive = driveMatch[1];
    const parts = driveMatch[2].split(/[\\/]+/).filter(Boolean);
    const result = [{ label: `${localDiskLabel} (${drive})`, path: `${drive}\\` }];
    let current = `${drive}\\`;
    for (const part of parts) {
      current = `${current}${part}\\`;
      result.push({ label: part, path: current });
    }
    return result;
  }
  if (!normalized.startsWith('/')) {
    return [{ label: normalized, path: normalized }];
  }
  const diskMountPoint = PathUtils.display(activeDisk?.mountPoint ?? '/').replace(/\/+$/, '');
  const mountPoint = diskMountPoint || '/';
  const isOnActiveDisk =
    normalized === mountPoint || normalized.startsWith(mountPoint === '/' ? '/' : `${mountPoint}/`);
  const rootPath = isOnActiveDisk ? mountPoint : '/';
  const rootLabel = isOnActiveDisk ? (activeDisk?.name ?? rootPath) : rootPath;
  const relativePath = normalized.slice(rootPath === '/' ? 1 : rootPath.length).replace(/^\/+/, '');
  const result = [{ label: rootLabel, path: rootPath }];
  let current = rootPath;
  for (const part of relativePath.split('/').filter(Boolean)) {
    current = current === '/' ? `/${part}` : `${current}/${part}`;
    result.push({ label: part, path: current });
  }
  return result;
}

/** Offer only known directories, using the newest snapshot without loading another folder. */
export function withSiblingFolders(
  breadcrumbs: readonly AnalysisBreadcrumb[],
  results: readonly AnalysisResult[]
): AnalysisBreadcrumb[] {
  const parentKeys = new Set(breadcrumbs.slice(0, -1).map(segment => PathUtils.comparisonKey(segment.path)));
  const requestedParents = [...parentKeys];
  const choices = new Map<string, { folders: AnalysisSiblingFolder[]; totalBytes: number }>();
  function offer(path: string, folders: readonly AnalysisSiblingFolder[], totalBytes: number) {
    const key = PathUtils.comparisonKey(path);
    if (!parentKeys.has(key) || choices.has(key)) return;
    choices.set(key, {
      folders: [...folders].sort((left, right) => right.bytes - left.bytes || left.name.localeCompare(right.name)),
      totalBytes,
    });
  }
  function visit(node: AnalysisDirectoryNode) {
    const key = PathUtils.comparisonKey(node.path);
    if (!requestedParents.some(parent => PathUtils.isSameOrChildKey(parent, key))) return;
    // Empty children may mean a hierarchy depth limit, not an empty directory.
    if (node.children.length) offer(node.path, node.children, node.bytes);
    node.children.forEach(visit);
  }
  for (const result of [...results].sort((left, right) => right.scannedAtMs - left.scannedAtMs)) {
    offer(
      result.root,
      result.entries.filter(entry => entry.isDirectory),
      result.totalBytes
    );
    result.directoryHierarchy?.forEach(visit);
    if (choices.size === parentKeys.size) break;
  }
  return breadcrumbs.map((segment, index) => {
    const parent = index > 0 ? choices.get(PathUtils.comparisonKey(breadcrumbs[index - 1].path)) : undefined;
    return { ...segment, siblings: parent?.folders ?? [], siblingParentBytes: parent?.totalBytes };
  });
}

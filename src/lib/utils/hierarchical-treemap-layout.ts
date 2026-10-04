import { ANALYSIS_CHART_MAX_DEPTH, TREEMAP_TILE_KINDS } from '@/lib/models/analysis';
import type {
  AnalysisDirectoryNode,
  AnalysisRemainderSelection,
  AnalysisResult,
  DirectoryEntryInfo,
  TreemapTile,
} from '@/lib/models/analysis';
import { layout as layoutSiblings } from './treemap-layout';

export type HierarchicalTreemapTile = TreemapTile & {
  key: string;
  depth: number;
  parentPath: string;
  parentBytes: number;
  branchPath: string | null;
  colorIndex: number;
  headerHeight: number;
  contentsRemainder: boolean;
  remainder?: AnalysisRemainderSelection;
};

interface Rect {
  left: number;
  top: number;
  width: number;
  height: number;
}

/** Headers reserve a reachable parent target; descendants occupy only its body. */
export function layout(
  result: AnalysisResult,
  depthLimit: number,
  viewport: { width: number; height: number }
): HierarchicalTreemapTile[] {
  const { width, height } = viewport;
  if (!(width > 0 && height > 0 && result.totalBytes > 0)) return [];
  const levels = Math.max(1, Math.min(ANALYSIS_CHART_MAX_DEPTH, Math.round(depthLimit)));
  const roots = new Map((result.directoryHierarchy ?? []).map(node => [node.path, node]));
  const tiles: HierarchicalTreemapTile[] = [];

  function partition(
    entries: DirectoryEntryInfo[],
    directories: Map<string, AnalysisDirectoryNode>,
    totalBytes: number,
    rect: Rect,
    depth: number,
    parentPath: string,
    branchPath: string | null,
    colorIndex: number,
    totalEntryCount?: number
  ) {
    const unlisted = totalBytes > entries.reduce((sum, entry) => sum + entry.bytes, 0);
    const siblings = layoutSiblings(entries, {
      totalBytes,
      totalEntryCount,
      viewport: { width: rect.width, height: rect.height },
      aspectRatio: rect.width / rect.height,
    });
    const visiblePaths = siblings.flatMap(tile => (tile.entry ? [tile.entry.path] : []));
    siblings.forEach((tile, index) => {
      const path = tile.entry?.path ?? null;
      const node = path ? directories.get(path) : undefined;
      const box = {
        left: rect.left + (tile.left / 100) * rect.width,
        top: rect.top + (tile.top / 100) * rect.height,
        width: (tile.width / 100) * rect.width,
        height: (tile.height / 100) * rect.height,
      };
      if (depth === 1) {
        // Inset only root regions; descendants share their parent's inner edges.
        const insetX = Math.min(1, box.width / 2);
        const insetY = Math.min(1, box.height / 2);
        box.left += insetX;
        box.top += insetY;
        box.width -= insetX * 2;
        box.height -= insetY * 2;
      }
      const headerHeight =
        depth < levels &&
        node &&
        (node.children.length > 0 || (node.files?.length ?? 0) > 0) &&
        box.width >= 112 &&
        box.height >= 96
          ? depth === 1
            ? 24
            : 18
          : 0;
      const branch = depth === 1 ? path : branchPath;
      const color = depth === 1 ? index % 5 : colorIndex;
      tiles.push({
        ...tile,
        left: (box.left / width) * 100,
        top: (box.top / height) * 100,
        width: (box.width / width) * 100,
        height: (box.height / height) * 100,
        key: path ?? `${parentPath}:remainder`,
        depth,
        parentPath,
        parentBytes: totalBytes,
        branchPath: branch,
        colorIndex: color,
        headerHeight,
        contentsRemainder: tile.kind === TREEMAP_TILE_KINDS.remainder && (depth > 1 || unlisted),
        remainder:
          tile.kind === TREEMAP_TILE_KINDS.remainder ? { parentPath, bytes: tile.bytes, visiblePaths } : undefined,
      });
      if (headerHeight && node && path) {
        const children = node.children.map(child => ({
          name: child.name,
          path: child.path,
          bytes: child.bytes,
          fileCount: child.fileCount,
          isDirectory: true,
          modifiedAtMs: null,
          contentFingerprint: null,
        }));
        partition(
          [...children, ...(node.files ?? [])],
          new Map(node.children.map(child => [child.path, child])),
          node.bytes,
          {
            // Shared side and bottom edges avoid gutters that grow with depth.
            left: box.left,
            top: box.top + headerHeight,
            width: box.width,
            height: box.height - headerHeight,
          },
          depth + 1,
          path,
          branch,
          color,
          node.totalEntryCount
        );
      }
    });
  }
  partition(
    result.entries,
    roots,
    result.totalBytes,
    { left: 0, top: 0, width, height },
    1,
    result.root,
    null,
    0,
    result.totalEntryCount
  );
  return tiles;
}

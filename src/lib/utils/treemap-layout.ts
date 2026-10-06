import { TREEMAP_TILE_KINDS } from '@/lib/models/analysis';
import type { DirectoryEntryInfo, TreemapTile } from '@/lib/models/analysis';

/** Computes normalized treemap geometry independently of viewport rendering. */
interface TreemapRect {
  left: number;
  top: number;
  width: number;
  height: number;
}
type TreemapLayoutNode =
  | {
      kind: typeof TREEMAP_TILE_KINDS.entry;
      entry: DirectoryEntryInfo;
      bytes: number;
    }
  | {
      kind: typeof TREEMAP_TILE_KINDS.remainder;
      entry: null;
      bytes: number;
      entryCount: number | null;
    };
export interface TreemapLayoutOptions {
  minimumVisibleShare?: number;
  aspectRatio?: number;
  totalBytes?: number;
  totalEntryCount?: number;
  viewport?: { width: number; height: number };
}
const DEFAULT_MINIMUM_VISIBLE_SHARE = 0.0075;
const MINIMUM_VISIBLE_PIXEL_AREA = 600;
export function layout(entries: DirectoryEntryInfo[], options: TreemapLayoutOptions = {}): TreemapTile[] {
  const candidates = entries.filter(entry => entry.bytes > 0).sort((left, right) => right.bytes - left.bytes);
  const listedBytes = candidates.reduce((sum, entry) => sum + entry.bytes, 0);
  const total = Math.max(listedBytes, options.totalBytes ?? 0);
  if (!(total > 0)) return [];
  const unlistedBytes = total - listedBytes;
  const viewportArea = options.viewport ? options.viewport.width * options.viewport.height : 0;
  const defaultVisibleShare =
    Number.isFinite(viewportArea) && viewportArea > 0
      ? MINIMUM_VISIBLE_PIXEL_AREA / viewportArea
      : DEFAULT_MINIMUM_VISIBLE_SHARE;
  const minimumVisibleShare = Math.min(1, Math.max(0, options.minimumVisibleShare ?? defaultVisibleShare));
  // Select by proportional pixel area, not the resulting rectangle's shape.
  // A larger viewport lowers this floor and can only reveal more candidates.
  // Label visibility is handled separately; it must not change membership.
  let visibleCount = candidates.length;
  while (visibleCount > 1 && candidates[visibleCount - 1].bytes / total < minimumVisibleShare) {
    visibleCount -= 1;
  }
  const requestedAspectRatio = options.aspectRatio ?? 1;
  const aspectRatio = Number.isFinite(requestedAspectRatio) && requestedAspectRatio > 0 ? requestedAspectRatio : 1;
  const remainderBytes = total - candidates.slice(0, visibleCount).reduce((sum, entry) => sum + entry.bytes, 0);
  const render = (count: number, bytes: number): TreemapTile[] => {
    const nodes: TreemapLayoutNode[] = candidates.slice(0, count).map(entry => ({
      kind: TREEMAP_TILE_KINDS.entry,
      entry,
      bytes: entry.bytes,
    }));
    if (bytes > 0) {
      nodes.push({
        kind: TREEMAP_TILE_KINDS.remainder,
        entry: null,
        bytes,
        // A truncated file list or bounded hierarchy projection cannot supply
        // an exact item count. Never present its partial count as authoritative.
        entryCount:
          options.totalEntryCount !== undefined
            ? Math.max(0, options.totalEntryCount - count)
            : unlistedBytes > 0
              ? null
              : candidates.length - count,
      });
    }
    nodes.sort((left, right) => right.bytes - left.bytes);
    return squarify(nodes, { left: 0, top: 0, width: 100 * aspectRatio, height: 100 }).map(tile => ({
      ...tile,
      left: tile.left / aspectRatio,
      width: tile.width / aspectRatio,
    }));
  };
  return render(visibleCount, remainderBytes);
}
/**
 * Lays out descending nodes in strips that minimize the worst tile aspect
 * ratio. The byte total is converted to the rectangle area once so every
 * emitted tile remains proportional to its source size.
 */
function squarify(entries: TreemapLayoutNode[], rect: TreemapRect): TreemapTile[] {
  const totalBytes = entries.reduce((sum, entry) => sum + entry.bytes, 0);
  if (totalBytes <= 0 || rect.width <= 0 || rect.height <= 0) return [];
  const scale = (rect.width * rect.height) / totalBytes;
  const areas = entries.map(entry => entry.bytes * scale);
  const tiles: TreemapTile[] = [];
  let remaining = { ...rect };
  let index = 0;
  while (index < entries.length) {
    const shortSide = Math.min(remaining.width, remaining.height);
    const rowStart = index;
    const rowAreas = [areas[index]];
    let rowArea = areas[index];
    let rowSmallestArea = areas[index];
    let rowLargestArea = areas[index];
    index += 1;
    // A row is finalized as soon as adding the next item would create a
    // worse extreme. This is the core Squarified Treemap decision and keeps
    // both labels and pointer targets usable across uneven size sets.
    while (index < entries.length) {
      const nextArea = areas[index];
      const nextRowArea = rowArea + nextArea;
      const nextSmallestArea = Math.min(rowSmallestArea, nextArea);
      const nextLargestArea = Math.max(rowLargestArea, nextArea);
      if (
        worstAspectRatio(nextRowArea, nextSmallestArea, nextLargestArea, shortSide) <=
        worstAspectRatio(rowArea, rowSmallestArea, rowLargestArea, shortSide)
      ) {
        rowAreas.push(nextArea);
        rowArea = nextRowArea;
        rowSmallestArea = nextSmallestArea;
        rowLargestArea = nextLargestArea;
        index += 1;
      } else {
        break;
      }
    }
    const thickness = shortSide > 0 ? rowArea / shortSide : 0;
    if (remaining.width >= remaining.height) {
      let top = remaining.top;
      rowAreas.forEach((area, rowIndex) => {
        const height = thickness > 0 ? area / thickness : 0;
        tiles.push({
          ...entries[rowStart + rowIndex],
          left: remaining.left,
          top,
          width: thickness,
          height,
        });
        top += height;
      });
      remaining = {
        left: remaining.left + thickness,
        top: remaining.top,
        width: Math.max(0, remaining.width - thickness),
        height: remaining.height,
      };
    } else {
      let left = remaining.left;
      rowAreas.forEach((area, rowIndex) => {
        const width = thickness > 0 ? area / thickness : 0;
        tiles.push({
          ...entries[rowStart + rowIndex],
          left,
          top: remaining.top,
          width,
          height: thickness,
        });
        left += width;
      });
      remaining = {
        left: remaining.left,
        top: remaining.top + thickness,
        width: remaining.width,
        height: Math.max(0, remaining.height - thickness),
      };
    }
  }
  return tiles;
}
/** Returns the largest width-to-height ratio in a prospective row. */
function worstAspectRatio(rowArea: number, smallestArea: number, largestArea: number, side: number): number {
  if (rowArea <= 0 || smallestArea <= 0 || side <= 0) return Number.POSITIVE_INFINITY;
  const rowAreaSquared = rowArea * rowArea;
  const sideSquared = side * side;
  return Math.max((sideSquared * largestArea) / rowAreaSquared, rowAreaSquared / (sideSquared * smallestArea));
}

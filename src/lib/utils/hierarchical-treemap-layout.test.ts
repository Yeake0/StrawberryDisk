import { describe, expect, it } from 'vitest';
import type { AnalysisDirectoryNode, AnalysisResult, DirectoryEntryInfo } from '@/lib/models/analysis';
import { layout } from './hierarchical-treemap-layout';
import { layout as sunburstLayout } from './sunburst-layout';

function node(
  name: string,
  path: string,
  bytes: number,
  children: AnalysisDirectoryNode[] = []
): AnalysisDirectoryNode {
  return { name, path, bytes, fileCount: 1, children };
}
const child = node('child', '/fixture/A/child', 200);
const root = node('A', '/fixture/A', 600, [child]);
const entries: DirectoryEntryInfo[] = [root, node('B', '/fixture/B', 300)].map(item => ({
  ...item,
  isDirectory: true,
  modifiedAtMs: null,
  contentFingerprint: null,
}));
const result: AnalysisResult = {
  scanId: 1,
  root: '/fixture',
  scannedAtMs: 1,
  totalBytes: 1000,
  skippedCount: 0,
  truncated: false,
  entries,
  directoryHierarchy: [root],
};
const viewport = { width: 1200, height: 900 };

describe('hierarchical treemap', () => {
  it('insets only root regions and accounts for bytes missing from the entry list', () => {
    const tiles = layout(result, 1, viewport);
    expect(tiles.reduce((sum, tile) => sum + tile.bytes, 0)).toBe(1000);
    for (const tile of tiles) {
      const outerWidth = tile.width + 200 / viewport.width;
      const outerHeight = tile.height + 200 / viewport.height;
      expect(outerWidth * outerHeight).toBeCloseTo(tile.bytes * 10);
    }
    expect(tiles.find(tile => tile.contentsRemainder)?.bytes).toBe(100);
    expect(tiles.every(tile => tile.headerHeight === 0 && tile.depth === 1)).toBe(true);
  });
  it('keeps child rectangles inside their parent body and links them to the root row', () => {
    const tiles = layout(result, 6, viewport);
    const parent = tiles.find(tile => tile.entry?.path === root.path)!;
    const descendants = tiles.filter(tile => tile.parentPath === root.path);
    expect(parent.headerHeight).toBeGreaterThan(0);
    expect(descendants.reduce((sum, tile) => sum + tile.bytes, 0)).toBe(600);
    for (const tile of descendants) {
      expect((tile.top * viewport.height) / 100).toBeGreaterThanOrEqual(
        (parent.top * viewport.height) / 100 + parent.headerHeight
      );
      expect(tile.left).toBeGreaterThanOrEqual(parent.left);
      expect(tile.left + tile.width).toBeLessThanOrEqual(parent.left + parent.width + 1e-10);
      expect(tile.top + tile.height).toBeLessThanOrEqual(parent.top + parent.height + 1e-10);
      expect(tile.branchPath).toBe(root.path);
      expect(tile.colorIndex).toBe(parent.colorIndex);
    }
    const visibleChild = descendants.find(tile => tile.entry?.path === child.path)!;
    const remainder = descendants.find(tile => tile.contentsRemainder)!;
    expect((visibleChild.width * visibleChild.height) / (remainder.width * remainder.height)).toBeCloseTo(0.5);
    expect(remainder.bytes).toBe(400);
  });
  it('honors six levels in both charts without inventing deeper directories', () => {
    let chain = node('G', '/fixture/A/B/C/D/E/F/G', 1000);
    for (const [name, path] of [
      ['F', '/fixture/A/B/C/D/E/F'],
      ['E', '/fixture/A/B/C/D/E'],
      ['D', '/fixture/A/B/C/D'],
      ['C', '/fixture/A/B/C'],
      ['B', '/fixture/A/B'],
      ['A', '/fixture/A'],
    ]) {
      chain = node(name!, path!, 1000, [chain]);
    }
    const deep = { ...result, entries: [{ ...entries[0]!, bytes: 1000 }], directoryHierarchy: [chain] };
    for (const level of [2, 3, 4, 5, 6, 20]) {
      const limit = Math.min(level, 6);
      expect(Math.max(...layout(deep, level, viewport).map(tile => tile.depth))).toBe(limit);
      expect(Math.max(...sunburstLayout(deep, level).map(sector => sector.depth))).toBe(limit);
    }
  });
  it('stops subdividing small regions and supports older results without a directory hierarchy', () => {
    expect(layout(result, 6, { width: 100, height: 60 }).every(tile => tile.depth === 1)).toBe(true);
    expect(layout({ ...result, directoryHierarchy: undefined }, 6, viewport).every(tile => tile.depth === 1)).toBe(
      true
    );
    expect(layout(result, 6, { width: 0, height: 900 })).toEqual([]);
  });
  it('counts omitted direct children at nested levels without counting recursive descendants', () => {
    const nested = { ...root, totalEntryCount: 120, children: [{ ...child, fileCount: 8000 }] };
    const tiles = layout({ ...result, directoryHierarchy: [nested] }, 6, viewport);
    expect(tiles.find(tile => tile.parentPath === root.path && !tile.entry)).toMatchObject({
      entryCount: 119,
      bytes: 400,
    });
    const smaller = layout({ ...result, directoryHierarchy: [nested] }, 6, { width: 400, height: 300 });
    const visible = smaller.filter(tile => tile.parentPath === root.path && tile.entry).length;
    expect(smaller.find(tile => tile.parentPath === root.path && !tile.entry)).toMatchObject({
      entryCount: 120 - visible,
    });
  });
  it('uses the complete root count and never invents counts for directory-only child projections', () => {
    const tiles = layout({ ...result, totalEntryCount: 42 }, 6, viewport);
    expect(tiles.find(tile => tile.depth === 1 && !tile.entry)).toMatchObject({ entryCount: 40 });
    expect(tiles.find(tile => tile.parentPath === root.path && !tile.entry)).toMatchObject({ entryCount: null });
  });
});

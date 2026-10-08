import { describe, expect, it } from 'vitest';
import type { AnalysisResult, DirectoryEntryInfo } from '@/lib/models/analysis';
import { layout } from './sunburst-layout';

function entry(path: string, bytes: number): DirectoryEntryInfo {
  return {
    name: path.split('/').at(-1) ?? '',
    path,
    bytes,
    fileCount: 1,
    isDirectory: true,
    modifiedAtMs: null,
    contentFingerprint: null,
  };
}
const result: AnalysisResult = {
  scanId: 1,
  root: '/fixture',
  scannedAtMs: 1,
  totalBytes: 1000,
  skippedCount: 0,
  truncated: false,
  entries: [entry('/fixture/A', 600), entry('/fixture/B', 300)],
  directoryHierarchy: [
    {
      name: 'A',
      path: '/fixture/A',
      bytes: 600,
      fileCount: 2,
      children: [
        { name: 'one', path: '/fixture/A/one', bytes: 200, fileCount: 1, children: [] },
        { name: 'two', path: '/fixture/A/two', bytes: 200, fileCount: 1, children: [] },
        { name: 'small', path: '/fixture/A/small', bytes: 1, fileCount: 1, children: [] },
      ],
    },
  ],
};

describe('sunburst layout', () => {
  it('keeps omitted bytes in the parent without inflating visible child angles', () => {
    const sectors = layout(result, 3);
    expect(sectors.filter(sector => sector.depth === 1).reduce((sum, sector) => sum + sector.bytes, 0)).toBe(1000);
    const secondRing = sectors.filter(sector => sector.depth === 2 && sector.branchPath === '/fixture/A');
    expect(secondRing.reduce((sum, sector) => sum + sector.bytes, 0)).toBe(400);
    expect(secondRing.every(sector => sector.path)).toBe(true);
    expect(sectors.find(sector => sector.path === '/fixture/A/one')?.bytes).toBe(200);
    expect(sectors.find(sector => sector.path === '/fixture/A/one')?.labelTransform).toContain('rotate(36)');
    expect(sectors.find(sector => sector.path === '/fixture/A')?.bytes).toBe(600);
  });
  it('ends leaf directories and direct files without adding a synthetic outer ring', () => {
    const sectors = layout(result, 4);
    expect(new Set(sectors.map(sector => sector.key)).size).toBe(sectors.length);
    expect(sectors.every(sector => sector.depth <= 2)).toBe(true);
    expect(sectors.filter(sector => !sector.path)).toHaveLength(1);
    expect(sectors.find(sector => sector.path === '/fixture/B')?.depth).toBe(1);
    const fileOnly = { ...result, entries: [{ ...entry('/fixture/file.bin', 1000), isDirectory: false }] };
    expect(layout(fileOnly, 4)).toHaveLength(1);
  });
  it('keeps descendants linked to their direct-child row and honors the depth limit', () => {
    expect(layout(result, 2).every(sector => sector.depth <= 2)).toBe(true);
    expect(layout(result, 3).find(sector => sector.path === '/fixture/A/two')?.branchPath).toBe('/fixture/A');
  });
  it('groups tiny sectors and leaves zero-byte results empty', () => {
    const tiny = { ...result, entries: [entry('/fixture/A', 999), entry('/fixture/tiny', 1)] };
    expect(layout(tiny, 3).find(sector => sector.path === '/fixture/tiny')).toBeUndefined();
    expect(layout(tiny, 3).find(sector => sector.depth === 1 && !sector.path)?.bytes).toBe(1);
    expect(layout({ ...result, totalBytes: 0 }, 3)).toEqual([]);
  });
  it('keeps straight labels tangent to the ring and limits them by ring thickness', () => {
    const only = { ...result, entries: [entry('/fixture/long-directory-name', 1000)], directoryHierarchy: [] };
    const sector = layout(only, 4)[0]!;
    expect(sector.labelTransform).toContain('rotate(360)');
    expect(sector.labelLength).toBeGreaterThan(0);
    expect(sector.labelLength).toBeLessThan(20);
  });
});

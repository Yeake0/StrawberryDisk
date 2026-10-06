import { beforeEach, describe, expect, it, vi } from 'vitest';

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn() }));

import { AnalysisService } from '@/lib/services/analysis-service';

describe('AnalysisService', () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockResolvedValue(undefined);
  });

  it('passes the selected analysis exclusions to the native scan', async () => {
    await AnalysisService.analyze('/fixture', false, ['/fixture/cache']);

    expect(invokeMock).toHaveBeenCalledWith('analyze_path', {
      scanMode: 'standard',
      path: '/fixture',
      refresh: false,
      excludedNames: [],
      excludedPaths: ['/fixture/cache'],
    });
  });

  it('keeps an unfiltered analysis request explicit', async () => {
    await AnalysisService.analyze(undefined, true, []);

    expect(invokeMock).toHaveBeenCalledWith('analyze_path', {
      scanMode: 'standard',
      path: null,
      refresh: true,
      excludedNames: [],
      excludedPaths: [],
    });
  });

  it('sends remainder scope, snapshot identity and pagination through the versioned command', async () => {
    const page = {
      schemaVersion: 2,
      snapshotId: 101,
      parentPath: '/fixture',
      totalBytes: 64,
      totalCount: 231,
      entries: [],
      nextOffset: null,
    };
    invokeMock.mockResolvedValueOnce(page);
    await expect(
      AnalysisService.listRemainder(
        7,
        { parentPath: '/fixture', bytes: 64, visiblePaths: ['/fixture/large'] },
        200,
        101
      )
    ).resolves.toEqual(page);
    expect(invokeMock).toHaveBeenCalledWith('list_analysis_remainder', {
      request: {
        schemaVersion: 2,
        scanId: 7,
        parentPath: '/fixture',
        expectedBytes: 64,
        visiblePaths: ['/fixture/large'],
        offset: 200,
        snapshotId: 101,
      },
    });
  });

  it('rejects an incompatible remainder response', async () => {
    invokeMock.mockResolvedValueOnce({ schemaVersion: 1 });
    await expect(
      AnalysisService.listRemainder(7, { parentPath: '/fixture', bytes: 64, visiblePaths: [] })
    ).rejects.toThrow('unsupported analysis remainder schema');
  });
});

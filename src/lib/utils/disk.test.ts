import { describe, expect, it } from 'vitest';

import type { DiskInfo } from '@/lib/models/disk';

import * as DiskUtils from './disk';

const systemDisk: DiskInfo = {
  name: 'System',
  mountPoint: '/',
  totalBytes: 1_000,
  availableBytes: 500,
  usedBytes: 500,
};
const dataDisk: DiskInfo = {
  name: 'Data',
  mountPoint: '/Volumes/Data',
  totalBytes: 2_000,
  availableBytes: 1_500,
  usedBytes: 500,
};

describe('disk utilities', () => {
  it('prefers the most specific mount and respects path boundaries', () => {
    expect(DiskUtils.findForPath([systemDisk, dataDisk], '/Volumes/Data/projects')).toBe(dataDisk);
    expect(DiskUtils.findForPath([dataDisk], '/Volumes/Database', systemDisk)).toBe(systemDisk);
  });

  it('returns the explicit fallback for an empty path', () => {
    expect(DiskUtils.findForPath([dataDisk], '', systemDisk)).toBe(systemDisk);
  });
});

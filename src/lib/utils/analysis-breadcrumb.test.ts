import { describe, expect, it } from 'vitest';

import type { DiskInfo } from '@/lib/models/disk';

import * as AnalysisBreadcrumbUtils from './analysis-breadcrumb';

const externalDisk: DiskInfo = {
  name: 'Projects',
  mountPoint: '/Volumes/Projects',
  totalBytes: 1_000,
  availableBytes: 500,
  usedBytes: 500,
};

describe('analysis breadcrumb utilities', () => {
  it('builds native Windows drive breadcrumbs', () => {
    expect(AnalysisBreadcrumbUtils.create('C:\\Users\\Fixture', null, 'Local Disk')).toEqual([
      { label: 'Local Disk (C:)', path: 'C:\\' },
      { label: 'Users', path: 'C:\\Users\\' },
      { label: 'Fixture', path: 'C:\\Users\\Fixture\\' },
    ]);
  });

  it('uses the active Unix volume as the root label', () => {
    expect(AnalysisBreadcrumbUtils.create('/Volumes/Projects/src/app', externalDisk, 'Local Disk')).toEqual([
      { label: 'Projects', path: '/Volumes/Projects' },
      { label: 'src', path: '/Volumes/Projects/src' },
      { label: 'app', path: '/Volumes/Projects/src/app' },
    ]);
  });

  it('handles unrelated absolute paths, relative paths, and empty input', () => {
    expect(AnalysisBreadcrumbUtils.create('/tmp/cache', externalDisk, 'Local Disk')).toEqual([
      { label: '/', path: '/' },
      { label: 'tmp', path: '/tmp' },
      { label: 'cache', path: '/tmp/cache' },
    ]);
    expect(AnalysisBreadcrumbUtils.create('relative', null, 'Local Disk')).toEqual([
      { label: 'relative', path: 'relative' },
    ]);
    expect(AnalysisBreadcrumbUtils.create('', null, 'Local Disk')).toEqual([]);
  });
});

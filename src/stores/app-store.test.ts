import { createPinia, setActivePinia } from 'pinia';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { DiskInfo } from '@/lib/models/disk';
import { LOG_DOMAINS, LOG_EVENTS } from '@/lib/models/telemetry';
import { DiskService } from '@/lib/services/disk-service';
import { LoggerService } from '@/lib/services/logger-service';

import { useAppStore } from './app-store';

const currentDisk: DiskInfo = {
  name: 'System',
  mountPoint: '/',
  totalBytes: 1_000,
  availableBytes: 400,
  usedBytes: 600,
};

describe('app store disk refresh', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.restoreAllMocks();
  });

  it('publishes a fresh system disk snapshot across shared disk state', async () => {
    const refreshedDisk: DiskInfo = {
      ...currentDisk,
      availableBytes: 650,
      usedBytes: 350,
    };
    vi.spyOn(DiskService, 'getSystemDisk').mockResolvedValue(refreshedDisk);
    const store = useAppStore();
    store.disk = currentDisk;
    store.disks = [currentDisk, { ...currentDisk, name: 'External', mountPoint: '/Volumes/External' }];

    await expect(store.refreshSystemDisk()).resolves.toBe(true);

    expect(store.disk).toEqual(refreshedDisk);
    expect(DiskService.getSystemDisk).toHaveBeenCalledWith(true);
    expect(store.disks).toEqual([refreshedDisk, { ...currentDisk, name: 'External', mountPoint: '/Volumes/External' }]);
  });

  it('requests a new native snapshot for every refresh', async () => {
    const firstSnapshot = { ...currentDisk, availableBytes: 450, usedBytes: 550 };
    const secondSnapshot = { ...currentDisk, availableBytes: 700, usedBytes: 300 };
    const getSystemDisk = vi
      .spyOn(DiskService, 'getSystemDisk')
      .mockResolvedValueOnce(firstSnapshot)
      .mockResolvedValueOnce(secondSnapshot);
    const store = useAppStore();

    await store.refreshSystemDisk();
    expect(store.disk).toEqual(firstSnapshot);

    await store.refreshSystemDisk();
    expect(store.disk).toEqual(secondSnapshot);
    expect(getSystemDisk).toHaveBeenCalledTimes(2);
  });

  it('keeps the previous snapshot when a secondary refresh fails', async () => {
    vi.spyOn(DiskService, 'getSystemDisk').mockRejectedValue(new Error('disk refresh failed'));
    const warn = vi.spyOn(LoggerService, 'warn').mockImplementation(() => undefined);
    const store = useAppStore();
    store.disk = currentDisk;

    await expect(store.refreshSystemDisk()).resolves.toBe(false);

    expect(store.disk).toEqual(currentDisk);
    expect(warn).toHaveBeenCalledWith(LOG_DOMAINS.applicationShell, LOG_EVENTS.diskRefreshFailed, {
      code: 'operationFailed',
    });
  });

  it('refreshes system and external volumes together', async () => {
    const disks = [
      { ...currentDisk, availableBytes: 700, usedBytes: 300 },
      { ...currentDisk, mountPoint: '/Volumes/External' },
    ];
    vi.spyOn(DiskService, 'listDisks').mockResolvedValue(disks);
    const store = useAppStore();
    store.disk = currentDisk;
    await expect(store.refreshDisks()).resolves.toBe(true);
    expect(store.disk).toEqual(disks[0]);
    expect(store.disks).toEqual(disks);
  });

  it('recovers the system capacity after a transient startup failure', async () => {
    vi.spyOn(LoggerService, 'error').mockImplementation(() => undefined);
    vi.spyOn(DiskService, 'getSystemDisk')
      .mockRejectedValueOnce(new Error('temporary capacity failure'))
      .mockResolvedValue(currentDisk);
    const disks = [{ ...currentDisk, mountPoint: '/Volumes/External' }, currentDisk];
    vi.spyOn(DiskService, 'listDisks').mockResolvedValue(disks);
    const store = useAppStore();
    await store.initialize();
    expect(store.disk).toBeNull();
    await expect(store.refreshDisks()).resolves.toBe(true);
    expect(store.disk).toEqual(currentDisk);
    expect(store.disks).toEqual(disks);
  });

  it('does not publish a startup snapshot older than a completed forced refresh', async () => {
    let complete!: (disk: DiskInfo) => void;
    const fresh = { ...currentDisk, availableBytes: 800, usedBytes: 200 };
    vi.spyOn(DiskService, 'getSystemDisk')
      .mockImplementationOnce(() => new Promise(resolve => (complete = resolve)))
      .mockResolvedValue(fresh);
    vi.spyOn(DiskService, 'listDisks').mockResolvedValue([currentDisk]);
    const store = useAppStore();
    const initialization = store.initialize();
    await store.refreshSystemDisk();
    complete(currentDisk);
    await initialization;
    expect(store.disk).toEqual(fresh);
  });

  it('does not report a late startup failure after a newer capacity snapshot succeeds', async () => {
    let fail!: (error: Error) => void;
    vi.spyOn(LoggerService, 'error').mockImplementation(() => undefined);
    vi.spyOn(DiskService, 'getSystemDisk')
      .mockImplementationOnce(() => new Promise((_, reject) => (fail = reject)))
      .mockResolvedValue(currentDisk);
    vi.spyOn(DiskService, 'listDisks').mockResolvedValue([currentDisk]);
    const store = useAppStore();
    const initialization = store.initialize();
    await store.refreshSystemDisk();
    fail(new Error('old startup failure'));
    await initialization;
    expect(store.disk).toEqual(currentDisk);
    expect(store.errorCode).toBeNull();
    expect(LoggerService.error).not.toHaveBeenCalled();
  });

  it('rejects a periodic response older than the post-cleanup snapshot', async () => {
    let complete!: (disks: DiskInfo[]) => void;
    vi.spyOn(DiskService, 'listDisks').mockImplementation(
      () =>
        new Promise(resolve => {
          complete = resolve;
        })
    );
    const fresh = { ...currentDisk, availableBytes: 800, usedBytes: 200 };
    vi.spyOn(DiskService, 'getSystemDisk').mockResolvedValue(fresh);
    const store = useAppStore();
    store.disk = currentDisk;
    store.disks = [currentDisk];
    const periodic = store.refreshDisks();
    await store.refreshSystemDisk();
    complete([currentDisk]);
    await expect(periodic).resolves.toBe(false);
    expect(store.disk).toEqual(fresh);
    expect(store.disks).toEqual([fresh]);
  });

  it('does not let an older forced response replace a newer forced refresh', async () => {
    const completions: ((disk: DiskInfo) => void)[] = [];
    vi.spyOn(DiskService, 'getSystemDisk').mockImplementation(
      () =>
        new Promise(resolve => {
          completions.push(resolve);
        })
    );
    const store = useAppStore();
    const first = store.refreshSystemDisk();
    const second = store.refreshSystemDisk();
    await expect(store.refreshDisks()).resolves.toBe(false);
    const fresh = { ...currentDisk, availableBytes: 900, usedBytes: 100 };
    completions[1]!(fresh);
    await expect(second).resolves.toBe(true);
    completions[0]!(currentDisk);
    await expect(first).resolves.toBe(false);
    expect(store.disk).toEqual(fresh);
    expect(store.pendingSystemDiskRefreshes).toBe(0);
  });
});

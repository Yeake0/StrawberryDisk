import { createPinia, setActivePinia } from 'pinia';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { DiskInfo } from '@/lib/models/disk';
import { LOG_DOMAINS, LOG_EVENTS } from '@/lib/models/telemetry';
import { DiskService } from '@/lib/services/disk-service';
import { LoggerService } from '@/lib/services/logger-service';
import { PreferenceStorageService } from '@/lib/services/preference-storage-service';
import { LanguageService } from '@/lib/services/language-service';
import { ThemeService } from '@/lib/services/theme-service';
import { ByteSizeService } from '@/lib/services/byte-size-service';
import { LANGUAGE_IDS, THEME_IDS } from '@/lib/models/settings';
import { DUPLICATE_KEEPER_RULE_IDS } from '@/lib/models/duplicate-file';
import { BYTE_UNIT_BASES } from '@/lib/utils/format';
import * as AppSettingsUtils from '@/lib/utils/app-settings';

import { useAppStore } from './app-store';

describe('app store settings upgrade', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.restoreAllMocks();
    vi.spyOn(LanguageService, 'apply').mockImplementation(() => undefined);
    vi.spyOn(ThemeService, 'apply').mockImplementation(() => undefined);
    vi.spyOn(LoggerService, 'warn').mockImplementation(() => undefined);
  });

  it.each([BYTE_UNIT_BASES.binary, BYTE_UNIT_BASES.decimal])(
    'preserves and applies released legacy preferences on startup with base %s',
    async unitBase => {
      const legacy = {
        language: 'zh-TW',
        theme: THEME_IDS.dark,
        largeFileMinimumBytes: 500 * 1024 * 1024,
        duplicateFileMinimumBytes: 10 * 1024 * 1024,
        duplicateKeeperRule: DUPLICATE_KEEPER_RULE_IDS.newestModified,
      };
      vi.spyOn(ByteSizeService, 'currentUnitBase').mockReturnValue(unitBase);
      vi.spyOn(PreferenceStorageService, 'loadSettings').mockResolvedValue(legacy);
      const clear = vi.spyOn(PreferenceStorageService, 'clearSettings').mockResolvedValue();
      const store = useAppStore();

      await store.loadSettings();

      expect(store.settings).toEqual({
        ...legacy,
        language: LANGUAGE_IDS.enUS,
        hideCleanupReadFailureAlerts: false,
        largeFileMinimumBytes: 500 * unitBase ** 2,
        duplicateFileMinimumBytes: 10 * unitBase ** 2,
      });
      expect(clear).not.toHaveBeenCalled();
      expect(LoggerService.warn).not.toHaveBeenCalled();
      expect(LanguageService.apply).toHaveBeenCalledWith(LANGUAGE_IDS.enUS);
      expect(ThemeService.apply).toHaveBeenCalledWith(legacy.theme);
    }
  );

  it('preserves a saved alert choice in the current format', async () => {
    const settings = {
      ...AppSettingsUtils.defaults(LANGUAGE_IDS.ptBR, BYTE_UNIT_BASES.decimal),
      theme: THEME_IDS.light,
      hideCleanupReadFailureAlerts: true,
    };
    vi.spyOn(ByteSizeService, 'currentUnitBase').mockReturnValue(BYTE_UNIT_BASES.decimal);
    vi.spyOn(PreferenceStorageService, 'loadSettings').mockResolvedValue(settings);
    const clear = vi.spyOn(PreferenceStorageService, 'clearSettings').mockResolvedValue();
    const store = useAppStore();

    await store.loadSettings();

    expect(store.settings).toEqual(settings);
    expect(clear).not.toHaveBeenCalled();
    expect(LanguageService.apply).toHaveBeenCalledWith(settings.language);
    expect(ThemeService.apply).toHaveBeenCalledWith(settings.theme);
  });

  it('clears a corrupt legacy document and applies safe defaults', async () => {
    vi.spyOn(ByteSizeService, 'currentUnitBase').mockReturnValue(BYTE_UNIT_BASES.decimal);
    vi.spyOn(LanguageService, 'detectSystemLanguage').mockReturnValue(LANGUAGE_IDS.ptBR);
    vi.spyOn(PreferenceStorageService, 'loadSettings').mockResolvedValue({ language: 'zh-TW' });
    const clear = vi.spyOn(PreferenceStorageService, 'clearSettings').mockResolvedValue();
    const store = useAppStore();

    await store.loadSettings();

    expect(clear).toHaveBeenCalledOnce();
    expect(store.settings).toEqual(AppSettingsUtils.defaults(LANGUAGE_IDS.ptBR, BYTE_UNIT_BASES.decimal));
    expect(LoggerService.warn).toHaveBeenCalledWith(LOG_DOMAINS.settings, LOG_EVENTS.savedSettingsInvalid, {
      error: expect.any(Error),
    });
    expect(LanguageService.apply).toHaveBeenCalledWith(LANGUAGE_IDS.ptBR);
    expect(ThemeService.apply).toHaveBeenCalledWith(THEME_IDS.system);
  });

  it('keeps unreadable persisted settings intact while applying safe defaults', async () => {
    const error = new Error('settings unavailable');
    vi.spyOn(ByteSizeService, 'currentUnitBase').mockReturnValue(BYTE_UNIT_BASES.decimal);
    vi.spyOn(LanguageService, 'detectSystemLanguage').mockReturnValue(LANGUAGE_IDS.ptBR);
    vi.spyOn(PreferenceStorageService, 'loadSettings').mockRejectedValue(error);
    const clear = vi.spyOn(PreferenceStorageService, 'clearSettings').mockResolvedValue();
    const store = useAppStore();

    await store.loadSettings();

    expect(clear).not.toHaveBeenCalled();
    expect(store.settings).toEqual(AppSettingsUtils.defaults(LANGUAGE_IDS.ptBR, BYTE_UNIT_BASES.decimal));
    expect(LoggerService.warn).toHaveBeenCalledWith(LOG_DOMAINS.settings, LOG_EVENTS.savedSettingsLoadFailed, {
      error,
    });
  });
});

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

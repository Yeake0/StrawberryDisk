import { createPinia, setActivePinia } from 'pinia';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type {
  SystemSettingItem,
  SystemSettingsCatalog,
  SystemSettingsChangePlan,
  SystemSettingsChangeResult,
} from '@/lib/models/system-settings';
import { HistoryService } from '@/lib/services/history-service';
import { SystemSettingsService } from '@/lib/services/system-settings-service';

import { useSystemSettingsStore } from './system-settings-store';

const setting: SystemSettingItem = {
  settingId: 'windows.content.disable-suggestions',
  category: 'privacy',
  selectionKind: 'oneClick',
  riskLevel: 'standard',
  status: 'recommended',
  selectedByDefault: true,
  restoreAvailable: false,
  requiresRestart: false,
  requiresElevation: false,
  diagnostic: null,
};

function catalog(items: SystemSettingItem[] = [setting]): SystemSettingsCatalog {
  return {
    schemaVersion: 4,
    scanId: 'scan-1',
    catalogRevision: 'revision-1',
    platform: 'windows',
    scannedAtMs: 1,
    items,
    summary: {
      itemCount: items.length,
      recommendedCount: items.filter(item => item.status === 'recommended').length,
      optimizedCount: items.filter(item => item.status === 'optimized').length,
      selectedCount: items.filter(item => item.selectedByDefault).length,
      unavailableCount: items.filter(item => item.status === 'unavailable').length,
    },
    elapsedMs: 1,
    recoveryAvailable: false,
  };
}

const plan: SystemSettingsChangePlan = {
  schemaVersion: 1,
  planId: 'system-settings-plan-1234567890abcdef',
  scanId: 'scan-1',
  catalogRevision: 'revision-1',
  createdAtMs: 1,
  expiresAtMs: 2,
  items: [
    {
      settingId: setting.settingId,
      category: setting.category,
      target: 'optimized',
      requiresRestart: false,
      requiresElevation: false,
    },
  ],
  skippedItems: [],
  requiresConfirmation: false,
  requiresRestart: false,
};

describe('system settings store', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.restoreAllMocks();
  });

  it('keeps current settings as the initial recommendation profile', async () => {
    const optimized = {
      ...setting,
      settingId: 'already-optimized',
      status: 'optimized' as const,
      selectedByDefault: false,
    };
    vi.spyOn(SystemSettingsService, 'scan').mockResolvedValue(catalog([setting, optimized]));
    const store = useSystemSettingsStore();

    await store.scan();

    expect(store.optimizationMode).toBe('unchanged');
    expect(store.desiredOptimizedIds).toEqual([optimized.settingId]);
  });

  it('leaves the loading state after a failed scan and recovers on retry', async () => {
    const scan = vi
      .spyOn(SystemSettingsService, 'scan')
      .mockRejectedValueOnce(new Error('operation busy'))
      .mockResolvedValueOnce(catalog());
    const store = useSystemSettingsStore();

    await store.scan();

    expect(store.scanning).toBe(false);
    expect(store.scanFailed).toBe(true);
    expect(store.catalog).toBeNull();

    await store.scan();

    expect(scan).toHaveBeenCalledTimes(2);
    expect(store.scanFailed).toBe(false);
    expect(store.catalog).toEqual(catalog());
  });

  it('refreshes operation history after an executed change', async () => {
    const updatedCatalog = catalog([{ ...setting, status: 'optimized', selectedByDefault: false }]);
    const result: SystemSettingsChangeResult = {
      planId: plan.planId,
      changedCount: 1,
      failedCount: 0,
      requiresRestart: false,
      recoveryAvailable: true,
      items: [{ settingId: setting.settingId, status: 'changed', verified: true, failureReason: null }],
      catalog: updatedCatalog,
    };
    vi.spyOn(SystemSettingsService, 'executeChange').mockResolvedValue(result);
    const refreshHistory = vi.spyOn(HistoryService, 'list').mockResolvedValue([]);
    const store = useSystemSettingsStore();
    store.catalog = catalog();
    store.pendingPlan = plan;

    await store.execute();

    expect(refreshHistory).toHaveBeenCalledOnce();
    expect(store.catalog).toEqual(updatedCatalog);
    expect(store.pendingPlan).toBeNull();
  });

  it('keeps a failed manual switch in the requested draft state', async () => {
    const result: SystemSettingsChangeResult = {
      planId: plan.planId,
      changedCount: 0,
      failedCount: 1,
      requiresRestart: false,
      recoveryAvailable: false,
      items: [
        {
          settingId: setting.settingId,
          status: 'failed',
          verified: false,
          failureReason: 'permissionDenied',
        },
      ],
      catalog: catalog(),
    };
    vi.spyOn(SystemSettingsService, 'executeChange').mockResolvedValue(result);
    vi.spyOn(HistoryService, 'list').mockResolvedValue([]);
    const store = useSystemSettingsStore();
    store.catalog = catalog();
    store.pendingPlan = plan;
    store.optimizationMode = 'manual';
    store.desiredOptimizedIds = [setting.settingId];

    await store.execute();

    expect(store.desiredOptimizedIds).toContain(setting.settingId);
  });

  it('prepares recovery only for settings changed by StrawberryDisk', async () => {
    const recoverable = {
      ...setting,
      settingId: 'windows.taskbar.disable-animations',
      status: 'optimized' as const,
      selectedByDefault: false,
      restoreAvailable: true,
    };
    const external = {
      ...setting,
      settingId: 'windows.personalization.disable-transparency',
      status: 'optimized' as const,
      selectedByDefault: false,
    };
    const current = catalog([recoverable, external]);
    current.recoveryAvailable = true;
    const restorePlan: SystemSettingsChangePlan = {
      ...plan,
      items: [
        {
          settingId: recoverable.settingId,
          category: recoverable.category,
          target: 'default',
          requiresRestart: false,
          requiresElevation: false,
        },
      ],
    };
    const prepare = vi.spyOn(SystemSettingsService, 'prepareChange').mockResolvedValue(restorePlan);
    const store = useSystemSettingsStore();
    store.catalog = current;
    store.desiredOptimizedIds = [recoverable.settingId, external.settingId];

    await store.prepareRecovery();

    expect(prepare).toHaveBeenCalledWith({
      scanId: current.scanId,
      items: [{ settingId: recoverable.settingId, target: 'default' }],
    });
    expect(store.desiredOptimizedIds).toEqual([external.settingId]);
    expect(store.optimizationMode).toBe('manual');
    expect(store.pendingPlan).toEqual(restorePlan);
  });

  it('prepares recovery when a catalog update changes the recommendation', async () => {
    const legacyRecoverable = {
      ...setting,
      settingId: 'macos.keyboard.fast-key-repeat',
      status: 'recommended' as const,
      selectedByDefault: false,
      restoreAvailable: true,
    };
    const current = catalog([legacyRecoverable]);
    current.recoveryAvailable = true;
    const restorePlan: SystemSettingsChangePlan = {
      ...plan,
      items: [
        {
          settingId: legacyRecoverable.settingId,
          category: legacyRecoverable.category,
          target: 'default',
          requiresRestart: false,
          requiresElevation: false,
        },
      ],
    };
    const prepare = vi.spyOn(SystemSettingsService, 'prepareChange').mockResolvedValue(restorePlan);
    const store = useSystemSettingsStore();
    store.catalog = current;

    await store.prepareRecovery();

    expect(prepare).toHaveBeenCalledWith({
      scanId: current.scanId,
      items: [{ settingId: legacyRecoverable.settingId, target: 'default' }],
    });
    expect(store.pendingPlan).toEqual(restorePlan);
  });

  it('keeps the current draft when recovery preparation fails', async () => {
    const recoverable = {
      ...setting,
      status: 'optimized' as const,
      selectedByDefault: false,
      restoreAvailable: true,
    };
    vi.spyOn(SystemSettingsService, 'prepareChange').mockRejectedValue(new Error('unavailable'));
    const store = useSystemSettingsStore();
    store.catalog = catalog([recoverable]);
    store.desiredOptimizedIds = [recoverable.settingId];

    await store.prepareRecovery();

    expect(store.desiredOptimizedIds).toEqual([recoverable.settingId]);
    expect(store.optimizationMode).toBe('unchanged');
    expect(store.pendingPlan).toBeNull();
  });

  it('keeps settings that no longer qualify when recovery planning skips them', async () => {
    const recoverable = {
      ...setting,
      status: 'optimized' as const,
      selectedByDefault: false,
      restoreAvailable: true,
    };
    const skippedPlan: SystemSettingsChangePlan = {
      ...plan,
      items: [],
      skippedItems: [
        {
          settingId: recoverable.settingId,
          reason: 'settingChanged',
        },
      ],
    };
    vi.spyOn(SystemSettingsService, 'prepareChange').mockResolvedValue(skippedPlan);
    const store = useSystemSettingsStore();
    store.catalog = catalog([recoverable]);
    store.desiredOptimizedIds = [recoverable.settingId];

    await store.prepareRecovery();

    expect(store.desiredOptimizedIds).toEqual([recoverable.settingId]);
    expect(store.pendingPlan).toEqual(skippedPlan);
  });
});

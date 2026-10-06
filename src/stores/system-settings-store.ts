import { defineStore } from 'pinia';

import type {
  SystemSettingChangeSelectionItem,
  SystemSettingsCatalog,
  SystemSettingsChangePlan,
  SystemSettingsChangeResult,
} from '@/lib/models/system-settings';
import { SystemSettingsService } from '@/lib/services/system-settings-service';
import { parseCommandError } from '@/lib/utils/error';
import {
  systemOptimizationDesiredIdsForMode,
  systemOptimizationPendingChanges,
  type SystemOptimizationMode,
  type SystemOptimizationPresetMode,
} from '@/lib/utils/system-settings-mode';

import { useAppStore } from './app-store';
import { useHistoryStore } from './history-store';

interface SystemSettingsState {
  catalog: SystemSettingsCatalog | null;
  desiredOptimizedIds: string[];
  optimizationMode: SystemOptimizationMode;
  scanning: boolean;
  scanFailed: boolean;
  cancelling: boolean;
  preparing: boolean;
  executing: boolean;
  pendingPlan: SystemSettingsChangePlan | null;
  lastResult: SystemSettingsChangeResult | null;
}

export const useSystemSettingsStore = defineStore('system-settings', {
  state: (): SystemSettingsState => ({
    catalog: null,
    desiredOptimizedIds: [],
    optimizationMode: 'unchanged',
    scanning: false,
    scanFailed: false,
    cancelling: false,
    preparing: false,
    executing: false,
    pendingPlan: null,
    lastResult: null,
  }),
  actions: {
    async scan() {
      if (this.scanning || this.executing) return;
      this.scanning = true;
      this.scanFailed = false;
      this.cancelling = false;
      useAppStore().clearError();
      try {
        const catalog = await SystemSettingsService.scan();
        this.catalog = catalog;
        this.scanFailed = false;
        const mode = this.optimizationMode === 'manual' ? 'unchanged' : this.optimizationMode;
        this.optimizationMode = mode;
        this.desiredOptimizedIds = systemOptimizationDesiredIdsForMode(catalog, mode);
      } catch (error) {
        if (parseCommandError(error)?.code !== 'operationCancelled') {
          this.scanFailed = true;
          useAppStore().reportError(error);
        }
      } finally {
        this.scanning = false;
        this.cancelling = false;
      }
    },
    async cancelScan() {
      if (!this.scanning || this.cancelling) return;
      this.cancelling = true;
      try {
        await SystemSettingsService.cancelScan();
      } catch (error) {
        useAppStore().reportError(error);
        this.cancelling = false;
      }
    },
    setDesiredOptimized(settingId: string, optimized: boolean) {
      const next = new Set(this.desiredOptimizedIds);
      if (optimized) next.add(settingId);
      else next.delete(settingId);
      this.desiredOptimizedIds = [...next];
      this.optimizationMode = 'manual';
    },
    applyMode(mode: SystemOptimizationPresetMode) {
      this.optimizationMode = mode;
      this.desiredOptimizedIds = this.catalog ? systemOptimizationDesiredIdsForMode(this.catalog, mode) : [];
    },
    async prepare() {
      if (!this.catalog || this.preparing || this.executing) return;
      const items = systemOptimizationPendingChanges(this.catalog, this.desiredOptimizedIds);
      await this.prepareItems(items);
    },
    async prepareRecovery() {
      if (!this.catalog || this.preparing || this.executing) return null;
      const restoreIds = new Set(
        this.catalog.items
          // Recovery remains valid when a later StrawberryDisk release changes its recommendation.
          // Core exposes only settings whose current value still matches the durable baseline.
          .filter(item => item.restoreAvailable)
          .map(item => item.settingId)
      );
      if (!restoreIds.size) return null;
      const plan = await this.prepareItems(
        [...restoreIds].map(settingId => ({
          settingId,
          target: 'default',
        }))
      );
      if (!plan) return null;
      const plannedRestoreIds = new Set(
        plan.items.filter(item => item.target === 'default').map(item => item.settingId)
      );
      this.desiredOptimizedIds = this.catalog.items
        .filter(item => item.status === 'optimized' && !plannedRestoreIds.has(item.settingId))
        .map(item => item.settingId);
      this.optimizationMode = 'manual';
      return plan;
    },
    async prepareItems(items: SystemSettingChangeSelectionItem[]) {
      if (!this.catalog || this.preparing || this.executing || !items.length) return null;
      this.preparing = true;
      this.pendingPlan = null;
      useAppStore().clearError();
      try {
        const plan = await SystemSettingsService.prepareChange({
          scanId: this.catalog.scanId,
          items,
        });
        this.pendingPlan = plan;
        return plan;
      } catch (error) {
        if (parseCommandError(error)?.code !== 'operationCancelled') useAppStore().reportError(error);
        return null;
      } finally {
        this.preparing = false;
      }
    },
    clearPlan() {
      if (!this.executing) this.pendingPlan = null;
    },
    async execute() {
      if (!this.pendingPlan || this.executing) return;
      this.executing = true;
      this.lastResult = null;
      const planId = this.pendingPlan.planId;
      const desiredBeforeExecution = new Set(this.desiredOptimizedIds);
      useAppStore().clearError();
      try {
        const result = await SystemSettingsService.executeChange(planId);
        this.lastResult = result;
        if (result.catalog) {
          this.catalog = result.catalog;
          if (this.optimizationMode === 'manual') {
            const failedIds = new Set(
              result.items.filter(item => item.status === 'failed').map(item => item.settingId)
            );
            const desired = new Set(
              result.catalog.items.filter(item => item.status === 'optimized').map(item => item.settingId)
            );
            for (const settingId of failedIds) {
              if (desiredBeforeExecution.has(settingId)) desired.add(settingId);
              else desired.delete(settingId);
            }
            this.desiredOptimizedIds = [...desired];
          } else {
            this.desiredOptimizedIds = systemOptimizationDesiredIdsForMode(result.catalog, this.optimizationMode);
          }
        }
        await useHistoryStore().load({ reportError: false });
      } catch (error) {
        if (parseCommandError(error)?.code !== 'operationCancelled') useAppStore().reportError(error);
      } finally {
        this.pendingPlan = null;
        this.executing = false;
      }
    },
  },
});

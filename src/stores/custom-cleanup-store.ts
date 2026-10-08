import { defineStore } from 'pinia';

import { CUSTOM_CLEANUP_PREFERENCES_SCHEMA_VERSION, type CustomCleanupRule } from '@/lib/models/custom-cleanup';
import { LOG_DOMAINS, LOG_EVENTS } from '@/lib/models/telemetry';
import { LoggerService } from '@/lib/services/logger-service';
import { PreferenceStorageService } from '@/lib/services/preference-storage-service';
import * as CustomCleanupPreferenceUtils from '@/lib/utils/custom-cleanup-preference';

interface CustomCleanupState {
  initialized: boolean;
  includeStandardRules: boolean;
  rules: CustomCleanupRule[];
}

const initializationByStore = new WeakMap<object, Promise<void>>();

export const useCustomCleanupStore = defineStore('custom-cleanup', {
  state: (): CustomCleanupState => ({ initialized: false, includeStandardRules: true, rules: [] }),
  actions: {
    async initialize() {
      if (this.initialized) return;
      const pending = initializationByStore.get(this);
      if (pending) return pending;

      const initialization = this.restore().finally(() => {
        this.initialized = true;
        initializationByStore.delete(this);
      });
      initializationByStore.set(this, initialization);
      return initialization;
    },
    async restore() {
      let saved: unknown | null;
      try {
        saved = await PreferenceStorageService.loadCustomCleanupPreferences();
      } catch (error) {
        LoggerService.warn(LOG_DOMAINS.cleanup, LOG_EVENTS.customCleanupPreferencesLoadFailed, { error });
        return;
      }
      if (saved === null) return;
      try {
        const preferences = CustomCleanupPreferenceUtils.parse(saved);
        this.includeStandardRules = preferences.includeStandardRules;
        this.rules = preferences.rules;
      } catch (error) {
        const reason = CustomCleanupPreferenceUtils.errorCode(error);
        LoggerService.warn(LOG_DOMAINS.cleanup, `${LOG_EVENTS.customCleanupPreferencesInvalid} reason=${reason}`, {
          error,
        });
        this.rules = [];
      }
    },
    async save(rules: CustomCleanupRule[], includeStandardRules: boolean) {
      const preferences = CustomCleanupPreferenceUtils.parse({
        schemaVersion: CUSTOM_CLEANUP_PREFERENCES_SCHEMA_VERSION,
        includeStandardRules,
        rules,
      });
      try {
        await PreferenceStorageService.saveCustomCleanupPreferences(preferences);
        this.includeStandardRules = preferences.includeStandardRules;
        this.rules = preferences.rules;
      } catch (error) {
        LoggerService.warn(LOG_DOMAINS.cleanup, LOG_EVENTS.customCleanupPreferencesSaveFailed, { error });
        throw error;
      }
    },
  },
});

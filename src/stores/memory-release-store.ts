import { defineStore } from 'pinia';
import type { ExcludedApplication, MemoryReleasePreferences } from '@/lib/models/memory-release';
import { MemoryReleaseService } from '@/lib/services/memory-release-service';
import { LoggerService } from '@/lib/services/logger-service';

export const useMemoryReleaseStore = defineStore('memory-release', {
  state: () => ({ preferences: null as MemoryReleasePreferences | null, saving: false, failed: false }),
  getters: {
    excluded: state => (path: string | null) =>
      !!path && !!state.preferences?.exclusions.some(entry => entry.path.toLowerCase() === path.toLowerCase()),
  },
  actions: {
    accept(preferences: MemoryReleasePreferences) {
      if (!this.preferences || preferences.revision >= this.preferences.revision) this.preferences = preferences;
    },
    async load() {
      try {
        this.accept(await MemoryReleaseService.preferences());
        this.failed = false;
      } catch (error) {
        this.failed = true;
        LoggerService.warn('monitoring', 'memory_preferences_load_failed', { error });
      }
    },
    async save(preferences: MemoryReleasePreferences): Promise<boolean> {
      if (this.saving) return false;
      this.saving = true;
      this.failed = false;
      try {
        this.accept(await MemoryReleaseService.save(preferences));
        return true;
      } catch (error) {
        this.failed = true;
        LoggerService.warn('monitoring', 'memory_preferences_save_failed', { error });
        // Reconcile another window's write; never silently overwrite its exclusions.
        try {
          this.accept(await MemoryReleaseService.preferences());
        } catch {
          /* Keep the last confirmed settings. */
        }
        return false;
      } finally {
        this.saving = false;
      }
    },
    async toggle(application: ExcludedApplication) {
      if (!this.preferences || this.saving) return;
      const exclusions = this.excluded(application.path)
        ? this.preferences.exclusions.filter(item => item.path.toLowerCase() !== application.path.toLowerCase())
        : [...this.preferences.exclusions, application];
      await this.save({ ...this.preferences, exclusions });
    },
  },
});

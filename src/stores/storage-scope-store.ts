import { defineStore } from 'pinia';

import { MAX_DIRECTORY_ENTRIES_PER_REQUEST } from '@/lib/models/folder-selection';
import type { DiskInfo } from '@/lib/models/disk';
import { LOG_DOMAINS, LOG_EVENTS } from '@/lib/models/telemetry';
import { STORAGE_SCOPE_IDS, type StorageScopeId, type StorageScopePreferences } from '@/lib/models/storage-scope';
import { FolderSelectionService } from '@/lib/services/folder-selection-service';
import { LoggerService } from '@/lib/services/logger-service';
import { PreferenceStorageService } from '@/lib/services/preference-storage-service';
import { StandardScanFolderService, type StandardScanFolder } from '@/lib/services/standard-scan-folder-service';
import * as PathUtils from '@/lib/utils/path';
import * as StorageScopePreferenceUtils from '@/lib/utils/storage-scope-preference';

interface StorageScopeState extends StorageScopePreferences {
  initialized: boolean;
  standardFolders: StandardScanFolder[];
}

// Initialization promises stay outside Pinia state because Promises are not
// serializable and should not appear in persisted state or developer tools.
// A WeakMap also keeps isolated Pinia instances independent in unit tests.
const initializationByStore = new WeakMap<object, Promise<void>>();

function selectedDuplicateFileProtection(
  protectedPaths: readonly string[],
  selectedPaths: StorageScopePreferences['selectedPaths']
): string[] {
  const selection = selectedPaths[STORAGE_SCOPE_IDS.duplicateFiles] ?? [];
  const selectedKeys = new Set((Array.isArray(selection) ? selection : [selection]).map(PathUtils.comparisonKey));
  return PathUtils.uniquePaths(protectedPaths).filter(path => selectedKeys.has(PathUtils.comparisonKey(path)));
}

export const useStorageScopeStore = defineStore('storage-scope', {
  state: (): StorageScopeState => ({
    initialized: false,
    schemaVersion: 3,
    selectedPaths: {},
    recentFolders: [],
    duplicateFileProtectedPaths: [],
    standardFolders: [],
  }),
  actions: {
    async initialize(disks: readonly DiskInfo[]) {
      if (this.initialized) return;

      const pendingInitialization = initializationByStore.get(this);
      if (pendingInitialization) {
        await pendingInitialization;
        return;
      }

      // System folders and saved selections are independent inputs. Resolve
      // them together so guarded navigation never mounts a selector with an
      // empty list that is replaced immediately after the first paint.
      const initialization = Promise.all([this.restorePreferences(disks), this.loadStandardFolders()])
        .then(() => {
          this.initialized = true;
        })
        .finally(() => {
          // Removing the completed task permits a retry if an unexpected error
          // escaped either initialization branch before completion.
          initializationByStore.delete(this);
        });
      initializationByStore.set(this, initialization);
      await initialization;
    },
    async loadStandardFolders() {
      try {
        this.standardFolders = await StandardScanFolderService.listAvailable();
      } catch (error) {
        // Standard folders are convenience shortcuts. Disk selection and the
        // native folder dialog remain available when platform discovery fails.
        LoggerService.info(LOG_DOMAINS.storageScope, LOG_EVENTS.standardScanFoldersLoadFailed, { error });
      }
    },
    async restorePreferences(disks: readonly DiskInfo[]) {
      let saved: unknown | null;
      try {
        saved = await PreferenceStorageService.loadStorageScopePreferences();
      } catch (error) {
        LoggerService.warn(LOG_DOMAINS.storageScope, LOG_EVENTS.storageScopePreferencesLoadFailed, { error });
        return;
      }
      try {
        const preferences =
          saved === null ? StorageScopePreferenceUtils.empty() : StorageScopePreferenceUtils.parse(saved);
        this.selectedPaths = preferences.selectedPaths;
        this.recentFolders = preferences.recentFolders;
        this.duplicateFileProtectedPaths = selectedDuplicateFileProtection(
          preferences.duplicateFileProtectedPaths ?? [],
          this.selectedPaths
        );
      } catch (error) {
        LoggerService.warn(LOG_DOMAINS.storageScope, LOG_EVENTS.storageScopePreferencesInvalid, { error });
        this.clearPersistedPreferences();
        return;
      }

      const diskKeys = new Set(disks.map(disk => PathUtils.comparisonKey(disk.mountPoint)));
      // Selected folders can outlive the bounded history. Validate the complete union in
      // IPC-sized batches so adding a ninth folder never silently drops an active scope.
      const validationPaths = [
        ...new Set([
          ...this.recentFolders,
          ...Object.values(this.selectedPaths).flat(),
          ...this.duplicateFileProtectedPaths,
        ]),
      ].filter(path => !diskKeys.has(PathUtils.comparisonKey(path)));
      if (!validationPaths.length) return;
      try {
        const previousPreferences = JSON.stringify({
          selectedPaths: this.selectedPaths,
          recentFolders: this.recentFolders,
          duplicateFileProtectedPaths: this.duplicateFileProtectedPaths,
        });
        const resolved = [];
        for (let offset = 0; offset < validationPaths.length; offset += MAX_DIRECTORY_ENTRIES_PER_REQUEST) {
          resolved.push(
            ...(await FolderSelectionService.resolveDirectories(
              validationPaths.slice(offset, offset + MAX_DIRECTORY_ENTRIES_PER_REQUEST)
            ))
          );
        }
        const targets = new Map(resolved.map(entry => [PathUtils.comparisonKey(entry.requestedPath), entry.path]));
        const seen = new Set<string>();
        const recentKeys = new Set(this.recentFolders.map(PathUtils.comparisonKey));
        this.recentFolders = resolved
          .filter(entry => recentKeys.has(PathUtils.comparisonKey(entry.requestedPath)))
          .flatMap(({ path }) => {
            const key = PathUtils.comparisonKey(path);
            if (seen.has(key)) return [];
            seen.add(key);
            return [path];
          });
        for (const [scopeId, selection] of Object.entries(this.selectedPaths)) {
          const paths = Array.isArray(selection) ? selection : [selection];
          const valid = paths.flatMap(path => {
            const key = PathUtils.comparisonKey(path);
            const target = diskKeys.has(key) ? path : targets.get(key);
            return target ? [target] : [];
          });
          if (Array.isArray(selection)) {
            // Keep an explicit empty selection: it must not expand to a whole disk on restart.
            this.selectedPaths[scopeId as StorageScopeId] = [...new Set(valid)];
          } else if (valid[0]) {
            this.selectedPaths[scopeId as StorageScopeId] = valid[0];
          } else {
            delete this.selectedPaths[scopeId as StorageScopeId];
          }
        }
        this.duplicateFileProtectedPaths = selectedDuplicateFileProtection(
          this.duplicateFileProtectedPaths.flatMap(path => {
            const key = PathUtils.comparisonKey(path);
            const target = diskKeys.has(key) ? path : targets.get(key);
            return target ? [target] : [];
          }),
          this.selectedPaths
        );
        const currentPreferences = JSON.stringify({
          selectedPaths: this.selectedPaths,
          recentFolders: this.recentFolders,
          duplicateFileProtectedPaths: this.duplicateFileProtectedPaths,
        });
        if (currentPreferences !== previousPreferences) this.persist();
      } catch (error) {
        // Retain the saved history when validation is temporarily unavailable.
        LoggerService.warn(LOG_DOMAINS.storageScope, LOG_EVENTS.storageScopeValidationFailed, { error });
      }
    },
    selectedPath(scopeId: StorageScopeId): string {
      const selection = this.selectedPaths[scopeId];
      return Array.isArray(selection) ? (selection[0] ?? '') : (selection ?? '');
    },
    select(scopeId: StorageScopeId, path: string, disks: readonly DiskInfo[]) {
      const normalized = PathUtils.display(path);
      if (!normalized) return;

      this.selectedPaths[scopeId] = scopeId === STORAGE_SCOPE_IDS.analysis ? normalized : [normalized];
      if (scopeId === STORAGE_SCOPE_IDS.duplicateFiles) {
        this.duplicateFileProtectedPaths = selectedDuplicateFileProtection(
          this.duplicateFileProtectedPaths ?? [],
          this.selectedPaths
        );
      }
      const diskKeys = new Set(disks.map(disk => PathUtils.comparisonKey(disk.mountPoint)));
      if (!diskKeys.has(PathUtils.comparisonKey(normalized))) {
        this.recentFolders = StorageScopePreferenceUtils.addRecentFolder(this.recentFolders, normalized);
      }
      this.persist();
    },
    selectPaths(
      scopeId: typeof STORAGE_SCOPE_IDS.largeFiles | typeof STORAGE_SCOPE_IDS.duplicateFiles,
      paths: string[],
      disks: readonly DiskInfo[]
    ) {
      const keys = new Set<string>();
      const selected = paths.map(PathUtils.display).filter(path => {
        const key = PathUtils.comparisonKey(path);
        if (!key || keys.has(key)) return false;
        keys.add(key);
        return true;
      });
      const previous = this.selectedPaths[scopeId] ?? [];
      const previousKeys = new Set((Array.isArray(previous) ? previous : [previous]).map(PathUtils.comparisonKey));
      this.selectedPaths[scopeId] = selected;
      if (scopeId === STORAGE_SCOPE_IDS.duplicateFiles) {
        this.duplicateFileProtectedPaths = selectedDuplicateFileProtection(
          this.duplicateFileProtectedPaths ?? [],
          this.selectedPaths
        );
      }
      const diskKeys = new Set(disks.map(disk => PathUtils.comparisonKey(disk.mountPoint)));
      const recentKeys = new Set(this.recentFolders.map(PathUtils.comparisonKey));
      for (const path of selected) {
        const key = PathUtils.comparisonKey(path);
        if (!previousKeys.has(key) && !diskKeys.has(key) && !recentKeys.has(key)) {
          this.recentFolders = StorageScopePreferenceUtils.addRecentFolder(this.recentFolders, path);
        }
      }
      this.persist();
    },
    removeFolder(path: string) {
      const removedKey = PathUtils.comparisonKey(path);
      this.recentFolders = StorageScopePreferenceUtils.removePath(this.recentFolders, path);
      for (const [scopeId, selectedPath] of Object.entries(this.selectedPaths)) {
        if (Array.isArray(selectedPath)) {
          this.selectedPaths[scopeId as StorageScopeId] = StorageScopePreferenceUtils.removePath(selectedPath, path);
        } else if (PathUtils.comparisonKey(selectedPath) === removedKey) {
          delete this.selectedPaths[scopeId as StorageScopeId];
        }
      }
      this.duplicateFileProtectedPaths = StorageScopePreferenceUtils.removePath(
        this.duplicateFileProtectedPaths ?? [],
        path
      );
      this.persist();
    },
    setDuplicateFileProtectedPaths(paths: string[]) {
      this.duplicateFileProtectedPaths = selectedDuplicateFileProtection(paths, this.selectedPaths);
      this.persist();
    },
    persist() {
      void PreferenceStorageService.saveStorageScopePreferences({
        schemaVersion: 3,
        selectedPaths: this.selectedPaths,
        recentFolders: this.recentFolders,
        duplicateFileProtectedPaths: this.duplicateFileProtectedPaths,
      }).catch(error => {
        LoggerService.warn(LOG_DOMAINS.storageScope, LOG_EVENTS.storageScopePreferencesSaveFailed, { error });
      });
    },
    clearPersistedPreferences() {
      this.selectedPaths = {};
      this.recentFolders = [];
      this.duplicateFileProtectedPaths = [];
      void PreferenceStorageService.clearStorageScopePreferences().catch(error => {
        LoggerService.warn(LOG_DOMAINS.storageScope, LOG_EVENTS.storageScopePreferencesClearFailed, { error });
      });
    },
  },
});

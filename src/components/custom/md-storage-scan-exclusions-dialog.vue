<script setup lang="ts">
import './scan-exclusions.css';

import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';

import MdDialogContent from '@/components/custom/md-dialog-content.vue';
import MdDialogFooter from '@/components/custom/md-dialog-footer.vue';
import MdDialogHeader from '@/components/custom/md-dialog-header.vue';
import MdIconAction from '@/components/custom/md-icon-action.vue';
import MdIcon from '@/components/icons/md-icon.vue';
import MdScanNameExclusionsEditor from '@/components/custom/md-scan-name-exclusions-editor.vue';
import MdCheckbox from '@/components/custom/md-checkbox.vue';
import { Button } from '@/components/ui/button';
import { Dialog, DialogDescription, DialogTitle } from '@/components/ui/dialog';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import {
  MAX_SCAN_EXCLUDED_FOLDERS,
  SCAN_EXCLUSION_SCOPES,
  type ScanExcludedFolder,
  type ScanExcludedName,
  type ScanExclusionScope,
} from '@/lib/models/storage-scan';
import { ICON_NAMES, TOOLTIP_OPEN_DELAY_MS } from '@/lib/models/ui';
import { FileManagerService } from '@/lib/services/file-manager-service';
import { FolderSelectionService } from '@/lib/services/folder-selection-service';
import { NativeDragDropService, type NativeDragDropEvent } from '@/lib/services/native-drag-drop-service';
import * as PathUtils from '@/lib/utils/path';

const props = defineProps<{
  modelValue: boolean;
  folders: ScanExcludedFolder[];
  names?: ScanExcludedName[];
  saving: boolean;
}>();

const emit = defineEmits<{
  'update:modelValue': [open: boolean];
  save: [folders: ScanExcludedFolder[], names: ScanExcludedName[]];
  error: [error: unknown];
}>();

const { t } = useI18n({ useScope: 'global' });
const draftFolders = ref<ScanExcludedFolder[]>([]);
const draftNames = ref<ScanExcludedName[]>([]);
const activeTab = ref('paths');
const draftSession = ref(0);
const scopes = Object.values(SCAN_EXCLUSION_SCOPES);
// Space analysis starts as a complete view unless the user explicitly opts out of a folder.
const defaultScopes = [
  SCAN_EXCLUSION_SCOPES.cleanup,
  SCAN_EXCLUSION_SCOPES.largeFiles,
  SCAN_EXCLUSION_SCOPES.duplicateFiles,
];
const scopeLabels = {
  cleanup: 'navigation.cleanup',
  largeFiles: 'navigation.large-files',
  duplicateFiles: 'navigation.duplicate-files',
  analysis: 'navigation.analysis',
} as const;
const selecting = ref(false);
const nativeDropActive = ref(false);
const openHelpPath = ref<string | null>(null);
const dropZoneElement = ref<HTMLElement | null>(null);
let stopNativeDropListener: (() => void) | null = null;
let nativeDropListenerMounted = false;
const addDisabled = computed(
  () => props.saving || selecting.value || draftFolders.value.length >= MAX_SCAN_EXCLUDED_FOLDERS
);

watch(
  () => props.modelValue,
  open => {
    if (open) {
      draftSession.value += 1;
      draftFolders.value = props.folders.map(folder => ({ path: folder.path, scopes: [...folder.scopes] }));
      draftNames.value = (props.names ?? []).map(item => ({ ...item, scopes: [...item.scopes] }));
    } else {
      nativeDropActive.value = false;
      openHelpPath.value = null;
    }
  },
  { immediate: true }
);

async function addFolders() {
  if (addDisabled.value) return;
  selecting.value = true;
  try {
    const selected = await FolderSelectionService.select(true, t('storageScanExclusions.chooseFolders'));
    await appendFolders(selected);
  } catch (error) {
    emit('error', error);
  } finally {
    selecting.value = false;
  }
}

async function appendFolders(paths: string[]) {
  if (!paths.length) return;
  const resolved = await FolderSelectionService.filterExistingDirectories(paths);
  const seen = new Set(draftFolders.value.map(folder => PathUtils.comparisonKey(folder.path)));
  const additions = resolved.filter(path => {
    const key = PathUtils.comparisonKey(path);
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
  if (draftFolders.value.length + additions.length > MAX_SCAN_EXCLUDED_FOLDERS) {
    emit('error', new Error(t('storageScanExclusions.limitReached', { count: MAX_SCAN_EXCLUDED_FOLDERS })));
    return;
  }
  draftFolders.value = [...draftFolders.value, ...additions.map(path => ({ path, scopes: [...defaultScopes] }))];
}

function handleNativeDrop(event: NativeDragDropEvent) {
  if (!props.modelValue || activeTab.value !== 'paths' || addDisabled.value) {
    nativeDropActive.value = false;
    return;
  }
  if (event.type === 'leave') {
    nativeDropActive.value = false;
    return;
  }
  const dropZone = dropZoneElement.value;
  const scale = window.devicePixelRatio || 1;
  const rect = dropZone?.getBoundingClientRect();
  const x = event.position.x / scale;
  const y = event.position.y / scale;
  // Tauri reports physical window coordinates while the DOM uses logical CSS
  // pixels. Restricting the native event to this rectangle prevents a folder
  // dropped elsewhere in the modal from being accepted unexpectedly.
  const withinDropZone = Boolean(rect && x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom);
  nativeDropActive.value = withinDropZone && event.type !== 'drop';
  if (event.type !== 'drop' || !withinDropZone) return;
  selecting.value = true;
  void appendFolders(event.paths)
    .catch(error => emit('error', error))
    .finally(() => {
      selecting.value = false;
    });
}

function removeFolder(path: string) {
  if (props.saving) return;
  const key = PathUtils.comparisonKey(path);
  draftFolders.value = draftFolders.value.filter(folder => PathUtils.comparisonKey(folder.path) !== key);
  if (openHelpPath.value === path) openHelpPath.value = null;
}

function toggleScope(path: string, scope: ScanExclusionScope, checked: boolean) {
  if (props.saving) return;
  draftFolders.value = draftFolders.value.map(folder => {
    if (PathUtils.comparisonKey(folder.path) !== PathUtils.comparisonKey(path)) return folder;
    const next = checked ? [...folder.scopes, scope] : folder.scopes.filter(value => value !== scope);
    // Each configured folder must affect at least one scan. Removing a folder
    // entirely is a separate, explicit action beside the folder path.
    return next.length ? { ...folder, scopes: scopes.filter(value => next.includes(value)) } : folder;
  });
}

function setHelpOpen(path: string, open: boolean) {
  if (open) openHelpPath.value = path;
  else if (openHelpPath.value === path) openHelpPath.value = null;
}

async function openFolder(path: string) {
  try {
    await FileManagerService.reveal(path);
  } catch (error) {
    emit('error', error);
  }
}

function saveDraft() {
  if (props.saving || selecting.value) return;
  emit(
    'save',
    draftFolders.value.map(folder => ({ ...folder, scopes: [...folder.scopes] })),
    draftNames.value.map(item => ({ ...item, scopes: [...item.scopes] }))
  );
}

function preventOutsideDismiss(event: Event) {
  // An exclusion remains a draft until the explicit footer action saves it.
  // Preventing overlay dismissal avoids silently losing several folder choices.
  event.preventDefault();
}

onMounted(() => {
  nativeDropListenerMounted = true;
  void NativeDragDropService.listen(handleNativeDrop)
    .then(stop => {
      if (nativeDropListenerMounted) stopNativeDropListener = stop;
      else stop();
    })
    .catch(error => emit('error', error));
});

onBeforeUnmount(() => {
  nativeDropListenerMounted = false;
  stopNativeDropListener?.();
  stopNativeDropListener = null;
});
</script>

<template>
  <Dialog :open="modelValue" @update:open="emit('update:modelValue', $event)">
    <MdDialogContent
      class="scan-exclusions exclusion-dialog flex h-[540px] min-h-0 flex-col"
      size="large"
      @interact-outside="preventOutsideDismiss"
    >
      <MdDialogHeader class="flex-none">
        <DialogTitle>{{ t('storageScanExclusions.title') }}</DialogTitle>
        <DialogDescription>{{ t('storageScanExclusions.description') }}</DialogDescription>
      </MdDialogHeader>

      <div class="exclusion-dialog-body">
        <div class="exclusion-tabs" role="group" :aria-label="t('storageScanExclusions.title')">
          <button type="button" :aria-pressed="activeTab === 'paths'" @click="activeTab = 'paths'">
            {{ t('storageScanExclusions.pathsTab') }} · {{ draftFolders.length }}
          </button>
          <button type="button" :aria-pressed="activeTab === 'names'" @click="activeTab = 'names'">
            {{ t('storageScanExclusions.namesTab') }} · {{ draftNames.length }}
          </button>
        </div>
        <MdScanNameExclusionsEditor
          v-show="activeTab === 'names'"
          :key="draftSession"
          v-model:names="draftNames"
          :disabled="saving"
        />
        <div v-if="activeTab === 'paths'" class="exclusion-path-panel">
          <div class="exclusion-toolbar">
            <p>
              {{ t('storageScanExclusions.folderCount', { count: draftFolders.length }, draftFolders.length) }}
            </p>
            <Button
              class="exclusion-add-button"
              variant="ghost"
              size="sm"
              type="button"
              :disabled="addDisabled"
              @click="addFolders"
            >
              <MdIcon :name="ICON_NAMES.folderPlus" :size="15" />
              {{ selecting ? t('storageScanExclusions.addingFolder') : t('storageScanExclusions.addFolder') }}
            </Button>
          </div>

          <div
            ref="dropZoneElement"
            class="exclusion-drop-zone"
            :class="{ empty: draftFolders.length === 0, active: nativeDropActive }"
            @dragover.prevent
            @dragenter.prevent
          >
            <div v-if="draftFolders.length" class="exclusion-list">
              <div v-for="folder in draftFolders" :key="PathUtils.comparisonKey(folder.path)" class="exclusion-row">
                <span class="exclusion-path">{{ PathUtils.display(folder.path) }}</span>
                <span class="exclusion-row-actions">
                  <MdIconAction
                    appearance="unstyled"
                    :disabled="saving"
                    :label="t('common.showInFileManager')"
                    @click="openFolder(folder.path)"
                  >
                    <MdIcon :name="ICON_NAMES.folderOpen" :size="16" />
                  </MdIconAction>
                  <MdIconAction
                    appearance="unstyled"
                    destructive
                    :disabled="saving"
                    :label="t('storageScanExclusions.removeFolder')"
                    @click="removeFolder(folder.path)"
                  >
                    <MdIcon :name="ICON_NAMES.trash" :size="16" />
                  </MdIconAction>
                </span>
                <div class="exclusion-scopes">
                  <span v-for="scope in scopes" :key="scope" class="exclusion-scope-item">
                    <label class="exclusion-scope">
                      <MdCheckbox
                        :model-value="folder.scopes.includes(scope)"
                        :disabled="saving || (folder.scopes.length === 1 && folder.scopes.includes(scope))"
                        @update:model-value="toggleScope(folder.path, scope, $event === true)"
                      />
                      {{ t(scopeLabels[scope]) }}
                    </label>
                    <TooltipProvider
                      v-if="scope === SCAN_EXCLUSION_SCOPES.cleanup"
                      :delay-duration="TOOLTIP_OPEN_DELAY_MS"
                      :disable-hoverable-content="true"
                      :ignore-non-keyboard-focus="true"
                    >
                      <Tooltip :open="openHelpPath === folder.path" @update:open="setHelpOpen(folder.path, $event)">
                        <TooltipTrigger as-child>
                          <button
                            class="md-help-action"
                            type="button"
                            :aria-label="t('storageScanExclusions.cleanupScopeHint')"
                            @click="setHelpOpen(folder.path, true)"
                          >
                            <MdIcon :name="ICON_NAMES.help" :size="13" aria-hidden="true" />
                          </button>
                        </TooltipTrigger>
                        <TooltipContent
                          class="max-w-[min(24rem,calc(100vw-24px))] text-left whitespace-normal text-wrap [overflow-wrap:anywhere]"
                        >
                          {{ t('storageScanExclusions.cleanupScopeHint') }}
                        </TooltipContent>
                      </Tooltip>
                    </TooltipProvider>
                  </span>
                </div>
              </div>
            </div>
            <button v-else class="exclusion-empty-action" type="button" :disabled="addDisabled" @click="addFolders">
              <MdIcon :name="ICON_NAMES.folderPlus" :size="28" />
              <strong>{{ t('storageScanExclusions.emptyTitle') }}</strong>
            </button>
          </div>
        </div>
      </div>

      <MdDialogFooter class="exclusion-footer">
        <span class="exclusion-footer-actions">
          <Button variant="outline" type="button" :disabled="saving" @click="emit('update:modelValue', false)">
            {{ t('common.cancel') }}
          </Button>
          <Button type="button" :disabled="saving || selecting" @click="saveDraft">
            {{ saving ? t('storageScanExclusions.saving') : t('storageScanExclusions.save') }}
          </Button>
        </span>
      </MdDialogFooter>
    </MdDialogContent>
  </Dialog>
</template>

<style scoped>
@reference "@assets/main.css";

.exclusion-tabs {
  flex: none;
  display: flex;
  gap: 4px;
  padding: 3px;
  border-radius: 6px;
  background: var(--surface-muted-subtle);
}
.exclusion-tabs button {
  flex: 1;
  cursor: pointer;
  padding: 6px 10px;
  border-radius: 4px;
  color: var(--muted-foreground);
  font-size: var(--font-content-body);
}
.exclusion-tabs button[aria-pressed='true'] {
  background: var(--background);
  color: var(--foreground);
  box-shadow: 0 1px 3px var(--border-subtle);
}
.exclusion-dialog-body {
  display: flex;
  min-height: 0;
  flex: 1;
  overflow: hidden;
  flex-direction: column;
  gap: 8px;
  padding: 10px 18px 12px;
  border-top: 1px solid var(--border-subtle);
}

.exclusion-path-panel {
  display: flex;
  flex: 1;
  min-height: 0;
  flex-direction: column;
  gap: 8px;
}

.exclusion-drop-zone.active {
  border-color: var(--primary);
  background: var(--surface-primary-subtle);
  box-shadow: 0 0 0 2px var(--border-primary-subtle);
}

.exclusion-list {
  height: 100%;
}

.exclusion-footer-actions {
  display: flex;
  flex: none;
  gap: 8px;
}
</style>

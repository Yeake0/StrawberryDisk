<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';

import MdDialogContent from '@/components/custom/md-dialog-content.vue';
import MdDialogFooter from '@/components/custom/md-dialog-footer.vue';
import MdDialogHeader from '@/components/custom/md-dialog-header.vue';
import MdIconAction from '@/components/custom/md-icon-action.vue';
import MdIcon from '@/components/icons/md-icon.vue';
import { Button } from '@/components/ui/button';
import MdCheckbox from '@/components/custom/md-checkbox.vue';
import { Dialog, DialogDescription, DialogTitle } from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import {
  CUSTOM_CLEANUP_PREFERENCES_SCHEMA_VERSION,
  MAX_CUSTOM_CLEANUP_FILTER_DAYS,
  MAX_CUSTOM_CLEANUP_PATTERNS_PER_RULE,
  MAX_CUSTOM_CLEANUP_ROOTS_PER_RULE,
  MAX_CUSTOM_CLEANUP_RULES,
  MAX_CUSTOM_CLEANUP_TEXT_LENGTH,
  type CustomCleanupRule,
} from '@/lib/models/custom-cleanup';
import { ICON_NAMES } from '@/lib/models/ui';
import { FileManagerService } from '@/lib/services/file-manager-service';
import { FolderSelectionService } from '@/lib/services/folder-selection-service';
import { NativeDragDropService, type NativeDragDropEvent } from '@/lib/services/native-drag-drop-service';
import * as PathUtils from '@/lib/utils/path';
import * as CustomCleanupPreferenceUtils from '@/lib/utils/custom-cleanup-preference';
import { useAppStore } from '@/stores/app-store';
import { useCustomCleanupStore } from '@/stores/custom-cleanup-store';
import { customCleanupDraftFingerprint, customCleanupPersistedState } from '../custom-cleanup-dialog-state';

const MEBIBYTE = 1024 * 1024;

const props = defineProps<{ modelValue: boolean }>();
const emit = defineEmits<{
  scan: [rules: CustomCleanupRule[], includeStandardRules: boolean];
  'update:modelValue': [open: boolean];
}>();
const { t } = useI18n({ useScope: 'global' });
const appStore = useAppStore();
const store = useCustomCleanupStore();
const drafts = ref<CustomCleanupRule[]>([]);
const activeRuleId = ref('');
const saving = ref(false);
const includeStandardRules = ref(true);
const validationRequested = ref(false);
const patternExamplesOpen = ref(false);
const directoryDropActive = ref(false);
const savedFingerprint = ref('');
let stopDirectoryDropListener: (() => void) | null = null;
let directoryDropListenerMounted = false;
const activeRule = computed(() => drafts.value.find(rule => rule.id === activeRuleId.value) ?? null);
const parsedRules = computed(() => {
  try {
    return CustomCleanupPreferenceUtils.parse({
      schemaVersion: CUSTOM_CLEANUP_PREFERENCES_SCHEMA_VERSION,
      includeStandardRules: includeStandardRules.value,
      rules: drafts.value,
    }).rules;
  } catch {
    return null;
  }
});
const canSave = computed(() => drafts.value.length === 0 || parsedRules.value !== null);
const draftFingerprint = computed(() => customCleanupDraftFingerprint(drafts.value, includeStandardRules.value));
const isDirty = computed(() => draftFingerprint.value !== savedFingerprint.value);
const invalidRuleIds = computed(() => {
  const ids = new Set<string>();
  for (const rule of drafts.value) {
    try {
      CustomCleanupPreferenceUtils.parse({
        schemaVersion: CUSTOM_CLEANUP_PREFERENCES_SCHEMA_VERSION,
        includeStandardRules: includeStandardRules.value,
        rules: [rule],
      });
    } catch {
      ids.add(rule.id);
    }
  }
  return ids;
});
const activeRuleMissingName = computed(() => activeRule.value?.name.trim().length === 0);
const activeRuleMissingDirectories = computed(() => activeRule.value?.roots.length === 0);
const activeRuleMissingPatterns = computed(() => activeRule.value?.namePatterns.length === 0);
const activeRulePatternsInvalid = computed(() => {
  const rule = activeRule.value;
  if (!rule || !rule.namePatterns.length) return false;
  return (
    rule.namePatterns.length > MAX_CUSTOM_CLEANUP_PATTERNS_PER_RULE ||
    rule.namePatterns.some(
      pattern =>
        !pattern.trim() ||
        pattern.trim().length > MAX_CUSTOM_CLEANUP_TEXT_LENGTH ||
        /[/\\]/u.test(pattern) ||
        pattern.includes('**')
    )
  );
});
const activeRuleSizeRangeInvalid = computed(() => {
  const rule = activeRule.value;
  if (!rule || rule.minimumBytes === null || rule.maximumBytes === null) return false;
  return rule.minimumBytes > rule.maximumBytes;
});
const activeRuleModifiedTimeInvalid = computed(() => {
  const modifiedTime = activeRule.value?.modifiedTime;
  return (
    modifiedTime?.mode !== undefined &&
    modifiedTime.mode !== 'any' &&
    (!Number.isSafeInteger(modifiedTime.days) ||
      modifiedTime.days < 1 ||
      modifiedTime.days > MAX_CUSTOM_CLEANUP_FILTER_DAYS)
  );
});

function cloneRule(rule: CustomCleanupRule): CustomCleanupRule {
  return {
    ...rule,
    roots: [...rule.roots],
    namePatterns: [...rule.namePatterns],
    modifiedTime: { ...rule.modifiedTime },
  };
}

async function reset() {
  await store.initialize();
  includeStandardRules.value = store.includeStandardRules;
  savedFingerprint.value = customCleanupDraftFingerprint(store.rules, store.includeStandardRules);
  drafts.value = store.rules.map(cloneRule);
  if (!drafts.value.length) drafts.value = [CustomCleanupPreferenceUtils.create()];
  activeRuleId.value = drafts.value[0]?.id ?? '';
  validationRequested.value = false;
  patternExamplesOpen.value = false;
}

function addRule() {
  if (drafts.value.length >= MAX_CUSTOM_CLEANUP_RULES) return;
  const rule = CustomCleanupPreferenceUtils.create();
  drafts.value = [...drafts.value, rule];
  activeRuleId.value = rule.id;
  validationRequested.value = false;
}

function copyRule(ruleId: string, index: number) {
  if (drafts.value.length >= MAX_CUSTOM_CLEANUP_RULES) return;
  const sourceIndex = drafts.value.findIndex(rule => rule.id === ruleId);
  const source = drafts.value[sourceIndex];
  if (!source) return;
  const displayName = source.name.trim() || t('cleanup.customCleanup.untitledRule', { index: index + 1 });
  const copy = cloneRule(source);
  copy.id = CustomCleanupPreferenceUtils.create().id;
  copy.name = t('cleanup.customCleanup.ruleCopyName', { name: displayName }).slice(0, MAX_CUSTOM_CLEANUP_TEXT_LENGTH);
  drafts.value.splice(sourceIndex + 1, 0, copy);
  activeRuleId.value = copy.id;
}

function removeRule(ruleId: string) {
  const index = drafts.value.findIndex(rule => rule.id === ruleId);
  if (index < 0) return;
  const removingActiveRule = activeRuleId.value === ruleId;
  drafts.value = drafts.value.filter(rule => rule.id !== ruleId);
  if (removingActiveRule) {
    activeRuleId.value = drafts.value[Math.min(index, drafts.value.length - 1)]?.id ?? '';
  }
}

async function addDirectories() {
  const rule = activeRule.value;
  if (!rule) return;
  try {
    const selected = await FolderSelectionService.select(true, t('cleanup.customCleanup.chooseDirectories'));
    await appendDirectories(rule.id, selected);
  } catch (error) {
    appStore.reportError(error);
  }
}

async function appendDirectories(ruleId: string, paths: string[]) {
  const directories = await FolderSelectionService.filterExistingDirectories(paths);
  const rule = drafts.value.find(item => item.id === ruleId);
  if (!rule) return;
  rule.roots = PathUtils.collapseOverlappingRoots([...rule.roots, ...directories.map(PathUtils.display)]).slice(
    0,
    MAX_CUSTOM_CLEANUP_ROOTS_PER_RULE
  );
}

function handleDirectoryDrop(event: NativeDragDropEvent) {
  if (!props.modelValue || !activeRule.value) {
    directoryDropActive.value = false;
    return;
  }
  if (event.type === 'leave') {
    directoryDropActive.value = false;
    return;
  }
  directoryDropActive.value = event.type !== 'drop';
  if (event.type === 'drop') {
    const ruleId = activeRule.value.id;
    void appendDirectories(ruleId, event.paths).catch(error => appStore.reportError(error));
  }
}

async function openDirectory(path: string) {
  try {
    await FileManagerService.reveal(path);
  } catch (error) {
    appStore.reportError(error);
  }
}

function removeDirectory(path: string) {
  const rule = activeRule.value;
  if (!rule) return;
  const key = PathUtils.comparisonKey(path);
  rule.roots = rule.roots.filter(item => PathUtils.comparisonKey(item) !== key);
}

function setPatterns(value: string | undefined) {
  if (!activeRule.value) return;
  activeRule.value.namePatterns = (value ?? '')
    .split(',')
    .map(pattern => pattern.trim())
    .filter(Boolean);
}

function sizeInMb(bytes: number | null): string {
  return bytes === null ? '' : String(bytes / MEBIBYTE);
}

function setSize(kind: 'minimumBytes' | 'maximumBytes', value: string | undefined) {
  const rule = activeRule.value;
  if (!rule) return;
  const text = value ?? '';
  const number = Number(text);
  rule[kind] = text.trim() === '' || !Number.isFinite(number) || number < 0 ? null : Math.round(number * MEBIBYTE);
}

function setModifiedMode(value: unknown) {
  const rule = activeRule.value;
  if (!rule || !['any', 'olderThan', 'newerThan'].includes(String(value))) return;
  const mode = String(value) as 'any' | 'olderThan' | 'newerThan';
  rule.modifiedTime = mode === 'any' ? { mode } : { mode, days: 30 };
}

function setModifiedDays(value: string | undefined) {
  const rule = activeRule.value;
  if (!rule || rule.modifiedTime.mode === 'any') return;
  const days = Number(value ?? '');
  rule.modifiedTime.days = Number.isSafeInteger(days) ? days : 0;
}

function modifiedDays(rule: CustomCleanupRule): string {
  return rule.modifiedTime.mode === 'any' ? '' : String(rule.modifiedTime.days);
}

async function persist(scan: boolean) {
  if (!canSave.value) {
    validationRequested.value = true;
    activeRuleId.value = drafts.value.find(rule => invalidRuleIds.value.has(rule.id))?.id ?? drafts.value[0]?.id ?? '';
    return;
  }
  if (scan && !parsedRules.value?.length) return;
  saving.value = true;
  try {
    const rules = parsedRules.value ?? [];
    await store.save(rules, includeStandardRules.value);
    const persisted = customCleanupPersistedState(rules, includeStandardRules.value, scan);
    drafts.value = rules.map(cloneRule);
    savedFingerprint.value = persisted.fingerprint;
    validationRequested.value = false;
    if (persisted.startScan) {
      emit('scan', rules.map(cloneRule), includeStandardRules.value);
    }
    if (persisted.closeDialog) {
      emit('update:modelValue', false);
    }
  } catch (error) {
    appStore.reportError(error);
  } finally {
    saving.value = false;
  }
}

watch(
  () => props.modelValue,
  open => {
    if (open) void reset();
    else directoryDropActive.value = false;
  }
);

onMounted(() => {
  directoryDropListenerMounted = true;
  void NativeDragDropService.listen(handleDirectoryDrop)
    .then(stop => {
      if (directoryDropListenerMounted) stopDirectoryDropListener = stop;
      else stop();
    })
    .catch(error => appStore.reportError(error));
});

onBeforeUnmount(() => {
  directoryDropListenerMounted = false;
  stopDirectoryDropListener?.();
  stopDirectoryDropListener = null;
});
</script>

<template>
  <Dialog :open="modelValue" @update:open="emit('update:modelValue', $event)">
    <!-- Keep unsaved rule edits open when the backdrop is clicked accidentally. -->
    <MdDialogContent class="custom-dialog flex min-h-0 flex-col" size="large" @pointer-down-outside.prevent>
      <MdDialogHeader class="flex-none">
        <DialogTitle>{{ t('cleanup.customCleanup.title') }}</DialogTitle>
        <DialogDescription>{{ t('cleanup.customCleanup.description') }}</DialogDescription>
      </MdDialogHeader>

      <div class="custom-dialog-body">
        <aside class="rule-sidebar">
          <div class="rule-list">
            <div
              v-for="(rule, index) in drafts"
              :key="rule.id"
              class="rule-list-item"
              :class="{ active: rule.id === activeRuleId }"
            >
              <button class="rule-list-select" type="button" @click="activeRuleId = rule.id">
                <span>{{ rule.name.trim() || t('cleanup.customCleanup.untitledRule', { index: index + 1 }) }}</span>
                <small>{{
                  t('cleanup.customCleanup.directoryCount', { count: rule.roots.length }, rule.roots.length)
                }}</small>
              </button>
              <span class="rule-list-actions">
                <MdIconAction
                  class="rule-list-action"
                  appearance="unstyled"
                  :label="t('cleanup.customCleanup.copyRule')"
                  :disabled="drafts.length >= MAX_CUSTOM_CLEANUP_RULES"
                  @click="copyRule(rule.id, index)"
                >
                  <MdIcon :name="ICON_NAMES.copy" :size="13" />
                </MdIconAction>
                <MdIconAction
                  class="rule-list-action"
                  appearance="unstyled"
                  destructive
                  :label="t('cleanup.customCleanup.removeRule')"
                  @click="removeRule(rule.id)"
                >
                  <MdIcon :name="ICON_NAMES.trash" :size="13" />
                </MdIconAction>
              </span>
            </div>
          </div>
          <div class="rule-sidebar-footer">
            <Button
              class="add-rule-button"
              variant="ghost"
              type="button"
              :disabled="drafts.length >= MAX_CUSTOM_CLEANUP_RULES"
              @click="addRule"
            >
              <MdIcon :name="ICON_NAMES.folderPlus" :size="15" />
              {{ t('cleanup.customCleanup.addRule') }}
            </Button>
          </div>
        </aside>

        <div v-if="activeRule" class="rule-editor scrollbar-stable-end">
          <div class="field">
            <span class="field-heading">
              <label :for="`custom-rule-name-${activeRule.id}`">{{ t('cleanup.customCleanup.ruleName') }}</label>
              <small v-if="validationRequested && activeRuleMissingName" class="field-error">
                {{ t('cleanup.customCleanup.ruleNameRequired') }}
              </small>
            </span>
            <Input
              :id="`custom-rule-name-${activeRule.id}`"
              v-model="activeRule.name"
              class="compact-control"
              :class="{ invalid: validationRequested && activeRuleMissingName }"
              :maxlength="MAX_CUSTOM_CLEANUP_TEXT_LENGTH"
              :aria-invalid="validationRequested && activeRuleMissingName"
              :placeholder="t('cleanup.customCleanup.ruleNamePlaceholder')"
            />
          </div>

          <div class="directory-section">
            <div class="directory-header">
              <span class="directory-title">
                <span class="field-label">{{ t('cleanup.customCleanup.directories') }}</span>
                <small v-if="validationRequested && activeRuleMissingDirectories" class="field-error">
                  {{ t('cleanup.customCleanup.directoriesRequired') }}
                </small>
                <small>{{ t('cleanup.customCleanup.directoryCount', { count: activeRule.roots.length }) }}</small>
              </span>
              <Button
                v-if="activeRule.roots.length"
                class="directory-header-action"
                variant="ghost"
                type="button"
                @click="addDirectories"
              >
                <MdIcon :name="ICON_NAMES.folderPlus" :size="14" />
                {{ t('cleanup.customCleanup.chooseDirectories') }}
              </Button>
            </div>
            <div
              class="directory-drop-zone"
              :class="{
                active: directoryDropActive,
                invalid: validationRequested && activeRuleMissingDirectories,
              }"
            >
              <div v-if="activeRule.roots.length" class="directory-list scrollbar-stable-end">
                <div v-for="path in activeRule.roots" :key="path" class="directory-item">
                  <span class="directory-path">{{ path }}</span>
                  <span class="directory-item-actions">
                    <MdIconAction
                      class="directory-item-action"
                      appearance="unstyled"
                      :label="t('common.showInFileManager')"
                      @click="openDirectory(path)"
                    >
                      <MdIcon :name="ICON_NAMES.folderOpen" :size="14" />
                    </MdIconAction>
                    <MdIconAction
                      class="directory-item-action"
                      appearance="unstyled"
                      destructive
                      :label="t('cleanup.customCleanup.removeDirectory')"
                      @click="removeDirectory(path)"
                    >
                      <MdIcon :name="ICON_NAMES.close" :size="14" />
                    </MdIconAction>
                  </span>
                </div>
              </div>
              <div v-else class="empty-directories">
                <Button class="choose-directory-button" variant="ghost" type="button" @click="addDirectories">
                  <MdIcon :name="ICON_NAMES.folderPlus" :size="15" />
                  {{ t('cleanup.customCleanup.chooseDirectories') }}
                </Button>
                <span>{{ t('cleanup.customCleanup.dropDirectories') }}</span>
              </div>
            </div>
          </div>

          <div class="field">
            <span class="field-heading">
              <label :for="`custom-rule-pattern-${activeRule.id}`">{{ t('cleanup.customCleanup.patterns') }}</label>
              <small
                v-if="validationRequested && (activeRuleMissingPatterns || activeRulePatternsInvalid)"
                class="field-error"
              >
                {{
                  t(
                    activeRuleMissingPatterns
                      ? 'cleanup.customCleanup.patternsRequired'
                      : 'cleanup.customCleanup.patternsInvalid'
                  )
                }}
              </small>
              <Tooltip v-model:open="patternExamplesOpen">
                <TooltipTrigger as-child>
                  <button
                    class="pattern-example-trigger"
                    type="button"
                    @click="patternExamplesOpen = !patternExamplesOpen"
                  >
                    <MdIcon :name="ICON_NAMES.help" :size="13" />
                    {{ t('cleanup.customCleanup.viewPatternExamples') }}
                  </button>
                </TooltipTrigger>
                <TooltipContent class="grid w-72 gap-2 px-3 py-2 text-left" side="bottom" align="end">
                  <strong class="font-semibold">{{ t('cleanup.customCleanup.patternExamplesTitle') }}</strong>
                  <span class="flex items-baseline gap-2">
                    <code class="w-24 shrink-0 font-mono font-semibold">*.log</code>
                    <span>{{ t('cleanup.customCleanup.patternExampleExtension') }}</span>
                  </span>
                  <span class="flex items-baseline gap-2">
                    <code class="w-24 shrink-0 font-mono font-semibold">cache-?.tmp</code>
                    <span>{{ t('cleanup.customCleanup.patternExampleCharacter') }}</span>
                  </span>
                  <span class="flex items-baseline gap-2">
                    <code class="w-24 shrink-0 font-mono font-semibold">*.log, *.tmp</code>
                    <span>{{ t('cleanup.customCleanup.patternExampleMultiple') }}</span>
                  </span>
                </TooltipContent>
              </Tooltip>
            </span>
            <Input
              :id="`custom-rule-pattern-${activeRule.id}`"
              class="compact-control"
              :class="{ invalid: validationRequested && (activeRuleMissingPatterns || activeRulePatternsInvalid) }"
              :aria-invalid="validationRequested && (activeRuleMissingPatterns || activeRulePatternsInvalid)"
              :model-value="activeRule.namePatterns.join(', ')"
              :placeholder="t('cleanup.customCleanup.patternsPlaceholder')"
              @update:model-value="setPatterns"
            />
            <small>{{ t('cleanup.customCleanup.patternsHint') }}</small>
          </div>

          <div class="filter-fields">
            <div class="field size-field">
              <label :for="`custom-rule-minimum-${activeRule.id}`">{{ t('cleanup.customCleanup.minimumSize') }}</label>
              <Input
                :id="`custom-rule-minimum-${activeRule.id}`"
                class="compact-control"
                :class="{ invalid: validationRequested && activeRuleSizeRangeInvalid }"
                type="number"
                min="0"
                :aria-invalid="validationRequested && activeRuleSizeRangeInvalid"
                :model-value="sizeInMb(activeRule.minimumBytes)"
                :placeholder="t('cleanup.customCleanup.noLimit')"
                @update:model-value="setSize('minimumBytes', $event)"
              />
            </div>
            <div class="field size-field">
              <span class="field-heading">
                <label :for="`custom-rule-maximum-${activeRule.id}`">{{
                  t('cleanup.customCleanup.maximumSize')
                }}</label>
                <small v-if="validationRequested && activeRuleSizeRangeInvalid" class="field-error">
                  {{ t('cleanup.customCleanup.sizeRangeInvalid') }}
                </small>
              </span>
              <Input
                :id="`custom-rule-maximum-${activeRule.id}`"
                class="compact-control"
                :class="{ invalid: validationRequested && activeRuleSizeRangeInvalid }"
                type="number"
                min="0"
                :aria-invalid="validationRequested && activeRuleSizeRangeInvalid"
                :model-value="sizeInMb(activeRule.maximumBytes)"
                :placeholder="t('cleanup.customCleanup.noLimit')"
                @update:model-value="setSize('maximumBytes', $event)"
              />
            </div>
            <div class="field modified-field">
              <span class="modified-labels">
                <span class="field-heading">
                  <span class="field-label">{{ t('cleanup.customCleanup.modifiedTime') }}</span>
                  <small v-if="validationRequested && activeRuleModifiedTimeInvalid" class="field-error">
                    {{ t('cleanup.customCleanup.modifiedTimeInvalid') }}
                  </small>
                </span>
                <label
                  :class="{ 'days-field-hidden': activeRule.modifiedTime.mode === 'any' }"
                  :for="`custom-rule-days-${activeRule.id}`"
                >
                  {{ t('cleanup.customCleanup.days') }}
                </label>
              </span>
              <div class="modified-controls">
                <Select :model-value="activeRule.modifiedTime.mode" @update:model-value="setModifiedMode">
                  <SelectTrigger
                    class="compact-control modified-select"
                    :class="{ invalid: validationRequested && activeRuleModifiedTimeInvalid }"
                  >
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="any">{{ t('cleanup.customCleanup.modifiedAny') }}</SelectItem>
                    <SelectItem value="olderThan">{{ t('cleanup.customCleanup.modifiedOlder') }}</SelectItem>
                    <SelectItem value="newerThan">{{ t('cleanup.customCleanup.modifiedNewer') }}</SelectItem>
                  </SelectContent>
                </Select>
                <Input
                  :id="`custom-rule-days-${activeRule.id}`"
                  class="compact-control"
                  :class="{
                    'days-field-hidden': activeRule.modifiedTime.mode === 'any',
                    invalid: validationRequested && activeRuleModifiedTimeInvalid,
                  }"
                  type="number"
                  min="1"
                  :max="MAX_CUSTOM_CLEANUP_FILTER_DAYS"
                  :aria-invalid="validationRequested && activeRuleModifiedTimeInvalid"
                  :disabled="activeRule.modifiedTime.mode === 'any'"
                  :model-value="modifiedDays(activeRule)"
                  @update:model-value="setModifiedDays"
                />
              </div>
            </div>
          </div>

          <div class="rule-options">
            <div class="rule-option">
              <MdCheckbox
                :id="`custom-rule-recursive-${activeRule.id}`"
                :model-value="activeRule.recursive"
                @update:model-value="activeRule.recursive = Boolean($event)"
              />
              <label :for="`custom-rule-recursive-${activeRule.id}`">
                {{ t('cleanup.customCleanup.includeSubdirectories') }}
              </label>
              <Tooltip>
                <TooltipTrigger as-child>
                  <button class="md-help-action" type="button">
                    <MdIcon :name="ICON_NAMES.help" :size="13" />
                    <span class="sr-only">{{ t('cleanup.customCleanup.includeSubdirectoriesDescription') }}</span>
                  </button>
                </TooltipTrigger>
                <TooltipContent class="max-w-64 text-left" side="top">
                  {{ t('cleanup.customCleanup.includeSubdirectoriesDescription') }}
                </TooltipContent>
              </Tooltip>
            </div>
            <div class="rule-option">
              <MdCheckbox
                :id="`custom-rule-remove-empty-${activeRule.id}`"
                :model-value="activeRule.removeEmptyDirectories"
                @update:model-value="activeRule.removeEmptyDirectories = Boolean($event)"
              />
              <label :for="`custom-rule-remove-empty-${activeRule.id}`">
                {{ t('cleanup.customCleanup.removeEmptyDirectories') }}
              </label>
              <Tooltip>
                <TooltipTrigger as-child>
                  <button class="md-help-action" type="button">
                    <MdIcon :name="ICON_NAMES.help" :size="13" />
                    <span class="sr-only">{{ t('cleanup.customCleanup.removeEmptyDirectoriesDescription') }}</span>
                  </button>
                </TooltipTrigger>
                <TooltipContent class="max-w-64 text-left" side="top">
                  {{ t('cleanup.customCleanup.removeEmptyDirectoriesDescription') }}
                </TooltipContent>
              </Tooltip>
            </div>
          </div>
        </div>

        <div v-else class="empty-rules">
          <p>{{ t('cleanup.customCleanup.empty') }}</p>
          <Button variant="outline" type="button" @click="addRule">{{ t('cleanup.customCleanup.addRule') }}</Button>
        </div>
      </div>

      <MdDialogFooter align="between">
        <label class="standard-scan-option">
          <MdCheckbox v-model="includeStandardRules" />
          <span>{{ t('cleanup.customCleanup.includeStandardRules') }}</span>
        </label>
        <span class="dialog-actions">
          <Button variant="outline" type="button" @click="emit('update:modelValue', false)">
            {{ t('common.cancel') }}
          </Button>
          <Button variant="outline" type="button" :disabled="saving || !isDirty" @click="persist(false)">
            {{ t('cleanup.customCleanup.save') }}
          </Button>
          <Button type="button" :disabled="saving || !drafts.length" @click="persist(true)">
            {{ t('cleanup.customCleanup.saveAndScan') }}
          </Button>
        </span>
      </MdDialogFooter>
    </MdDialogContent>
  </Dialog>
</template>

<style scoped>
@reference "@assets/main.css";

.custom-dialog {
  min-height: 0;
}

.custom-dialog-body {
  display: grid;
  min-height: 0;
  /*
   * The editor may shrink and scroll when the viewport is short, but it must
   * not grow into unused dialog space. Growing here creates the detached
   * footer divider that made the form look vertically unbalanced.
   */
  flex: 0 1 auto;
  grid-template-columns: 170px minmax(0, 1fr);
  overflow: hidden;
  border-top: 1px solid var(--border-subtle);
}

.rule-sidebar {
  display: flex;
  min-height: 0;
  flex-direction: column;
  overflow: hidden;
  border-right: 1px solid var(--border-subtle);
}

.rule-list {
  display: flex;
  min-height: 0;
  flex: 1 1 0;
  flex-direction: column;
  gap: 2px;
  overflow-y: auto;
  padding: 7px;
}

.rule-sidebar-footer {
  flex: none;
  border-top: 1px solid var(--border-subtle);
  padding: 4px 7px;
}

.rule-list-item {
  position: relative;
  display: flex;
  min-width: 0;
  align-items: center;
  border-radius: 6px;
  transition: background-color 140ms ease;
}

.rule-list-item:hover {
  background: var(--surface-accent-subtle);
}

.rule-list-item.active {
  @apply bg-accent text-accent-foreground;
}

.rule-list-select {
  display: grid;
  min-width: 0;
  flex: 1;
  cursor: pointer;
  gap: 1px;
  padding: 7px 8px;
  text-align: left;
}

.rule-list-select span {
  overflow: hidden;
  font-size: var(--font-content-primary);
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.rule-list-select small,
.field small {
  @apply text-muted-foreground;
  font-size: var(--font-content-secondary);
}

.rule-list-select small {
  padding-right: 52px;
}

.rule-list-actions {
  position: absolute;
  right: 5px;
  bottom: 5px;
  display: flex;
  gap: 1px;
  opacity: 0;
  pointer-events: none;
  transition: opacity 120ms ease;
}

.rule-list-item:hover .rule-list-actions,
.rule-list-item:focus-within .rule-list-actions {
  opacity: 1;
  pointer-events: auto;
}

:deep(.rule-list-action) {
  display: grid;
  width: 24px;
  height: 24px;
  cursor: pointer;
  place-items: center;
  border-radius: 5px;
  color: var(--muted-foreground);
}

:deep(.rule-list-action:hover) {
  @apply text-foreground;
  background: var(--card);
}

:deep(.rule-list-action[aria-disabled='true']) {
  cursor: not-allowed;
  opacity: 0.4;
}

:deep(.rule-list-action.destructive:hover) {
  color: var(--destructive);
}

.add-rule-button {
  width: 100%;
  height: 32px;
  justify-content: flex-start;
}

.rule-editor {
  display: grid;
  min-height: 0;
  align-content: start;
  container-type: inline-size;
  grid-template-columns: minmax(0, 1fr);
  gap: 10px;
  overflow-y: auto;
  padding: 10px 14px;
}

.field {
  display: grid;
  gap: 4px;
}

.field-heading {
  display: flex;
  min-width: 0;
  min-height: 17px;
  align-items: baseline;
  gap: 7px;
}

.field label,
.field-label {
  font-size: var(--font-content-secondary);
  font-weight: var(--font-weight-label);
}

.pattern-example-trigger {
  @apply text-muted-foreground;
  display: inline-flex;
  flex: none;
  margin-left: auto;
  cursor: pointer;
  align-items: center;
  gap: 4px;
  font-size: var(--font-content-secondary);
  transition: color 140ms ease;
}

.pattern-example-trigger:hover,
.pattern-example-trigger:focus-visible {
  color: var(--foreground);
}

.directory-section {
  display: grid;
  gap: 4px;
}

.directory-header {
  display: flex;
  min-height: 26px;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.directory-title {
  display: flex;
  min-width: 0;
  align-items: baseline;
  gap: 8px;
}

.directory-header small,
.empty-directories {
  @apply text-muted-foreground;
  font-size: var(--font-content-secondary);
}

.directory-header-action {
  height: 28px;
  padding-inline: 7px;
  font-size: var(--font-content-secondary);
}

.directory-drop-zone {
  display: flex;
  height: 126px;
  min-height: 126px;
  flex-direction: column;
  overflow: hidden;
  border-color: var(--border-subtle);
  border-width: 1px;
  border-radius: 7px;
  background: transparent;
  transition:
    border-color 140ms ease,
    background-color 140ms ease;
}

.directory-drop-zone.active {
  border-color: var(--primary);
  background: var(--surface-primary-subtle);
}

.directory-drop-zone.invalid {
  border-color: var(--destructive);
}

.directory-list {
  display: grid;
  min-height: 0;
  flex: 1;
  align-content: start;
  overflow-y: auto;
  padding: 4px 8px;
}

.directory-item {
  @apply text-muted-foreground;
  display: grid;
  min-height: 28px;
  grid-template-columns: minmax(0, 1fr) auto;
  align-items: center;
  gap: 6px;
  border-color: var(--border-subtle);
  border-bottom-width: 1px;
  padding: 3px 2px 3px 5px;
  font-size: var(--font-content-secondary);
}

.directory-item:last-child {
  border-bottom-width: 0;
}

.directory-path {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.directory-item-actions {
  display: flex;
  gap: 1px;
  opacity: 0;
  pointer-events: none;
  transition: opacity 120ms ease;
}

:deep(.directory-item-action) {
  display: grid;
  width: 24px;
  height: 24px;
  cursor: pointer;
  place-items: center;
  border-radius: 5px;
}

:deep(.directory-item-action:hover) {
  @apply bg-accent text-foreground;
}

:deep(.directory-item-action.destructive:hover) {
  color: var(--destructive);
}

.directory-item:hover .directory-item-actions,
.directory-item:focus-within .directory-item-actions {
  opacity: 1;
  pointer-events: auto;
}

.choose-directory-button {
  width: fit-content;
  height: 32px;
  padding-inline: 8px;
}

.filter-fields {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 10px;
}

.compact-control {
  width: 100%;
  min-width: 0;
  height: 32px;
}

.compact-control.invalid {
  border-color: var(--destructive);
}

/*
 * Older WebKit exposes platform steppers that do not follow the compact field
 * geometry. Numeric values remain keyboard-editable, while hiding the native
 * controls keeps the editor consistent across supported macOS versions.
 */
.compact-control[type='number'] {
  -webkit-appearance: none;
  appearance: textfield;
}

.compact-control[type='number']::-webkit-inner-spin-button,
.compact-control[type='number']::-webkit-outer-spin-button {
  margin: 0;
  -webkit-appearance: none;
  appearance: none;
}

.modified-select {
  background: transparent;
}

.modified-field {
  grid-column: 1 / -1;
}

.modified-labels,
.modified-controls {
  display: grid;
  min-width: 0;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 10px;
}

.days-field-hidden {
  visibility: hidden;
}

.modified-labels label {
  font-size: var(--font-content-secondary);
  font-weight: var(--font-weight-label);
}

.empty-directories {
  display: flex;
  min-height: 0;
  flex: 1;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 4px;
  padding: 12px;
}

.rule-options {
  display: flex;
  min-height: 24px;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px 24px;
}

.rule-option {
  display: inline-flex;
  min-width: 0;
  align-items: center;
  gap: 7px;
}

.rule-option label {
  cursor: pointer;
  font-size: var(--font-content-secondary);
  font-weight: var(--font-weight-label);
}

.empty-rules {
  display: grid;
  place-content: center;
  justify-items: center;
  gap: 10px;
  padding: 32px;
  color: var(--muted-foreground);
}

.field-error {
  overflow: hidden;
  color: var(--destructive);
  font-size: var(--font-content-secondary);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.field-heading .field-error,
.directory-title .field-error {
  color: var(--destructive);
}

.standard-scan-option {
  display: flex;
  flex: none;
  cursor: pointer;
  align-items: center;
  gap: 7px;
  font-size: var(--font-content-secondary);
  font-weight: var(--font-weight-label);
}

.dialog-actions {
  display: flex;
  margin-left: auto;
  gap: 8px;
}

@container (max-width: 500px) {
  .custom-dialog-body {
    grid-template-columns: 150px minmax(0, 1fr);
  }

  .filter-fields {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}
</style>

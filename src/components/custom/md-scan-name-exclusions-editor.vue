<script setup lang="ts">
import { computed, nextTick, ref } from 'vue';
import { useI18n } from 'vue-i18n';

import MdDialogContent from '@/components/custom/md-dialog-content.vue';
import MdDialogFooter from '@/components/custom/md-dialog-footer.vue';
import MdDialogHeader from '@/components/custom/md-dialog-header.vue';
import { Dialog, DialogDescription, DialogTitle } from '@/components/ui/dialog';
import MdCheckbox from '@/components/custom/md-checkbox.vue';
import MdIconAction from '@/components/custom/md-icon-action.vue';
import MdIcon from '@/components/icons/md-icon.vue';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import {
  MAX_SCAN_EXCLUDED_NAMES,
  SCAN_EXCLUSION_SCOPES,
  type ScanExcludedName,
  type ScanExclusionScope,
} from '@/lib/models/storage-scan';
import { ICON_NAMES, TOOLTIP_OPEN_DELAY_MS } from '@/lib/models/ui';
import { validExcludedName } from '@/lib/utils/storage-scan-preference';

const props = defineProps<{ names: ScanExcludedName[]; disabled: boolean }>();
const emit = defineEmits<{ 'update:names': [names: ScanExcludedName[]] }>();
const { t } = useI18n({ useScope: 'global' });
const adding = ref(false);
const name = ref('');
const kind = ref<'file' | 'folder'>('folder');
const error = ref('');
const openHelpName = ref<string | null>(null);
const usageHelpOpen = ref(false);
const form = ref<HTMLElement | null>(null);
const toolbar = ref<HTMLElement | null>(null);
let addTrigger: HTMLElement | null = null;
const limitReached = computed(() => props.names.length >= MAX_SCAN_EXCLUDED_NAMES);
const scopes = Object.values(SCAN_EXCLUSION_SCOPES);
const labels = {
  cleanup: 'navigation.cleanup',
  largeFiles: 'navigation.large-files',
  duplicateFiles: 'navigation.duplicate-files',
  analysis: 'navigation.analysis',
} as const;

function startAdding(event: MouseEvent) {
  if (props.disabled || limitReached.value) return;
  addTrigger = event.currentTarget as HTMLElement;
  name.value = '';
  kind.value = 'folder';
  error.value = '';
  adding.value = true;
}

async function focusName(event: Event) {
  event.preventDefault();
  await nextTick();
  form.value?.querySelector('input')?.focus();
}

function restoreFocus(event: Event) {
  event.preventDefault();
  // Adding the first rule removes the empty-state trigger from the DOM.
  const target = addTrigger?.isConnected
    ? addTrigger
    : toolbar.value?.querySelector<HTMLButtonElement>('.exclusion-add-button');
  target?.focus();
}

function add() {
  if (props.disabled || limitReached.value) return false;
  const value = name.value.trim();
  if (!validExcludedName(value)) {
    error.value = t('storageScanExclusions.invalidName');
    return false;
  }
  if (props.names.some(item => item.name === value && item.kind === kind.value)) {
    error.value = t('storageScanExclusions.duplicateName');
    return false;
  }
  emit('update:names', [...props.names, { name: value, kind: kind.value, scopes: ['largeFiles', 'duplicateFiles'] }]);
  name.value = '';
  error.value = '';
  adding.value = false;
  return true;
}

function toggle(index: number, scope: ScanExclusionScope, checked: boolean) {
  if (props.disabled) return;
  emit(
    'update:names',
    props.names.map((item, position) => {
      if (position !== index) return item;
      const selected = checked ? [...item.scopes, scope] : item.scopes.filter(value => value !== scope);
      return selected.length ? { ...item, scopes: scopes.filter(value => selected.includes(value)) } : item;
    })
  );
}

function remove(index: number) {
  if (!props.disabled)
    emit(
      'update:names',
      props.names.filter((_, position) => index !== position)
    );
}

function setHelpOpen(item: ScanExcludedName, open: boolean) {
  const key = `${item.kind}:${item.name}`;
  if (open) openHelpName.value = key;
  else if (openHelpName.value === key) openHelpName.value = null;
}
</script>

<template>
  <div class="name-editor">
    <div ref="toolbar" class="name-toolbar">
      <div class="name-summary">
        <p>{{ t('storageScanExclusions.nameCount', { count: names.length }, names.length) }}</p>
        <TooltipProvider
          :delay-duration="TOOLTIP_OPEN_DELAY_MS"
          :disable-hoverable-content="true"
          :ignore-non-keyboard-focus="true"
        >
          <Tooltip v-model:open="usageHelpOpen">
            <TooltipTrigger as-child>
              <button
                class="md-help-action"
                type="button"
                :aria-label="t('storageScanExclusions.nameUsageHint')"
                @click="usageHelpOpen = true"
              >
                <MdIcon :name="ICON_NAMES.help" :size="13" aria-hidden="true" />
              </button>
            </TooltipTrigger>
            <TooltipContent
              class="max-w-[min(24rem,calc(100vw-24px))] text-left whitespace-normal text-wrap [overflow-wrap:anywhere]"
            >
              <p>{{ t('storageScanExclusions.nameUsageHint') }}</p>
              <p class="mt-2">{{ t('storageScanExclusions.nameExampleHint') }}</p>
            </TooltipContent>
          </Tooltip>
        </TooltipProvider>
      </div>
      <Button
        class="exclusion-add-button"
        variant="ghost"
        size="sm"
        :disabled="disabled || limitReached"
        @click="startAdding"
      >
        <MdIcon :name="ICON_NAMES.folderPlus" :size="15" />{{ t('storageScanExclusions.addName') }}
      </Button>
    </div>
    <Dialog v-model:open="adding">
      <MdDialogContent
        size="compact"
        @open-auto-focus="focusName"
        @close-auto-focus="restoreFocus"
        @interact-outside="$event.preventDefault()"
      >
        <form ref="form" class="name-form" @submit.prevent="add">
          <MdDialogHeader>
            <DialogTitle>{{ t('storageScanExclusions.addName') }}</DialogTitle>
            <DialogDescription>{{ t('storageScanExclusions.nameMatchHint') }}</DialogDescription>
          </MdDialogHeader>
          <div class="name-form-body">
            <div class="name-fields">
              <label>
                <span>{{ t('storageScanExclusions.objectType') }}</span>
                <Select v-model="kind" :disabled="disabled">
                  <SelectTrigger class="name-kind-select" :aria-label="t('storageScanExclusions.objectType')">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="folder">{{ t('storageScanExclusions.folderKind') }}</SelectItem>
                    <SelectItem value="file">{{ t('storageScanExclusions.fileKind') }}</SelectItem>
                  </SelectContent>
                </Select>
              </label>
              <label class="name-input-label">
                <span>{{ t('storageScanExclusions.exactName') }}</span>
                <Input
                  v-model="name"
                  :disabled="disabled"
                  :placeholder="t('storageScanExclusions.namePlaceholder')"
                  :aria-invalid="!!error"
                  :aria-describedby="error ? 'scan-name-error' : undefined"
                  @input="error = ''"
                />
              </label>
            </div>
            <div class="name-presets">
              <span>{{ t('storageScanExclusions.quickFill') }}</span>
              <button
                v-for="preset in ['node_modules', '.DS_Store']"
                :key="preset"
                type="button"
                :disabled="disabled"
                @click="
                  name = preset;
                  kind = preset === 'node_modules' ? 'folder' : 'file';
                  error = '';
                "
              >
                {{ preset }}
              </button>
            </div>
            <p v-if="error" id="scan-name-error" class="name-error" role="alert">{{ error }}</p>
          </div>
          <MdDialogFooter>
            <Button type="button" variant="outline" @click="adding = false">{{ t('common.cancel') }}</Button>
            <Button type="submit" :disabled="disabled || !name.trim()">{{ t('storageScanExclusions.add') }}</Button>
          </MdDialogFooter>
        </form>
      </MdDialogContent>
    </Dialog>
    <div class="name-list" :class="{ empty: !names.length }">
      <div v-for="(item, index) in names" :key="`${item.kind}:${item.name}`" class="name-row">
        <span class="name-value">{{ item.name }}</span>
        <span class="exclusion-row-actions">
          <span class="name-kind">{{
            t(item.kind === 'folder' ? 'storageScanExclusions.folderKind' : 'storageScanExclusions.fileKind')
          }}</span>
          <MdIconAction
            appearance="unstyled"
            destructive
            :disabled="disabled"
            :label="t('storageScanExclusions.removeName', { name: item.name })"
            @click="remove(index)"
            ><MdIcon :name="ICON_NAMES.trash" :size="16"
          /></MdIconAction>
        </span>
        <div class="name-scopes">
          <span v-for="scope in scopes" :key="scope" class="exclusion-scope-item">
            <label class="exclusion-scope">
              <MdCheckbox
                :model-value="item.scopes.includes(scope)"
                :disabled="disabled || (item.scopes.length === 1 && item.scopes.includes(scope))"
                @update:model-value="toggle(index, scope, $event === true)"
              />
              {{ t(labels[scope]) }}
            </label>
            <TooltipProvider
              v-if="scope === SCAN_EXCLUSION_SCOPES.cleanup"
              :delay-duration="TOOLTIP_OPEN_DELAY_MS"
              :disable-hoverable-content="true"
              :ignore-non-keyboard-focus="true"
            >
              <Tooltip :open="openHelpName === `${item.kind}:${item.name}`" @update:open="setHelpOpen(item, $event)">
                <TooltipTrigger as-child>
                  <button
                    class="md-help-action"
                    type="button"
                    :aria-label="t('storageScanExclusions.nameCleanupScopeHint')"
                    @click="setHelpOpen(item, true)"
                  >
                    <MdIcon :name="ICON_NAMES.help" :size="13" aria-hidden="true" />
                  </button>
                </TooltipTrigger>
                <TooltipContent
                  class="max-w-[min(24rem,calc(100vw-24px))] text-left whitespace-normal text-wrap [overflow-wrap:anywhere]"
                >
                  {{ t('storageScanExclusions.nameCleanupScopeHint') }}
                </TooltipContent>
              </Tooltip>
            </TooltipProvider>
          </span>
        </div>
      </div>
      <button
        v-if="!names.length"
        class="name-empty"
        type="button"
        :disabled="disabled || limitReached"
        @click="startAdding"
      >
        <MdIcon :name="ICON_NAMES.folderPlus" :size="28" />
        <strong>{{ t('storageScanExclusions.emptyNames') }}</strong>
      </button>
    </div>
    <p v-if="limitReached" class="name-note" role="status">
      {{ t('storageScanExclusions.nameLimitReached', { count: MAX_SCAN_EXCLUDED_NAMES }) }}
    </p>
  </div>
</template>

<style scoped>
@reference "@assets/main.css";
.name-editor {
  display: flex;
  min-height: 0;
  flex: 1;
  flex-direction: column;
  gap: 8px;
}
.name-form-body {
  padding: 16px 20px;
}
.name-note {
  flex: none;
  color: var(--muted-foreground);
  font-size: var(--font-content-meta);
}
.name-summary {
  display: flex;
  align-items: center;
  gap: 2px;
}
.name-fields {
  display: flex;
  flex-wrap: wrap;
  align-items: flex-end;
  gap: 8px;
}
.name-fields label {
  display: flex;
  flex-direction: column;
  gap: 5px;
  font-size: var(--font-content-meta);
}
.name-input-label {
  flex: 1;
  min-width: 140px;
}
.name-kind-select {
  min-width: 100px;
}
.name-presets {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  margin-top: 10px;
  color: var(--muted-foreground);
  font-size: var(--font-content-meta);
}
.name-presets button {
  padding: 2px 6px;
  border: 1px solid var(--border-subtle);
  border-radius: 4px;
}
.name-presets button:hover:not(:disabled) {
  background: var(--surface-primary-subtle);
  color: var(--foreground);
}
.name-error {
  margin-top: 8px;
  color: var(--destructive);
  font-size: var(--font-content-meta);
}
.name-kind {
  padding: 2px 6px;
  border-radius: 4px;
  color: var(--muted-foreground);
  background: var(--surface-muted-subtle);
  font-size: var(--font-content-meta);
}
</style>

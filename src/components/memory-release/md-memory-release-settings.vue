<script setup lang="ts">
import MdTooltip from '@/components/custom/md-tooltip.vue';
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import MdSettingsGroup from '@/components/custom/md-settings-group.vue';
import MdSettingsRow from '@/components/custom/md-settings-row.vue';
import MdResultSearch from '@/components/custom/md-result-search.vue';
import MdSwitch from '@/components/custom/md-switch.vue';
import MdCheckbox from '@/components/custom/md-checkbox.vue';
import { Button } from '@/components/ui/button';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import MdIcon from '@/components/icons/md-icon.vue';
import MdNativeFileIcon from '@/components/custom/md-native-file-icon.vue';
import { ICON_NAMES } from '@/lib/models/ui';
import type { ExcludedApplication, MemoryReleasePreferences } from '@/lib/models/memory-release';
import { MemoryReleaseService } from '@/lib/services/memory-release-service';
import { LoggerService } from '@/lib/services/logger-service';
import { OperatingSystemService } from '@/lib/services/operating-system-service';
import MdDialogHeader from '@/components/custom/md-dialog-header.vue';
import MdDialogFooter from '@/components/custom/md-dialog-footer.vue';
import { DialogTitle, DialogDescription } from '@/components/ui/dialog';

const props = withDefaults(
  defineProps<{
    dialog?: boolean;
    preferences: MemoryReleasePreferences | null;
    saving: boolean;
    failed: boolean;
    reloadPreferences: () => Promise<void>;
    savePreferences: (preferences: MemoryReleasePreferences) => Promise<boolean>;
  }>(),
  { dialog: false }
);
const emit = defineEmits<{ close: [] }>();

const { t } = useI18n();
const windows = OperatingSystemService.isWindows();
function copyPreferences(preferences: MemoryReleasePreferences | null): MemoryReleasePreferences | null {
  return preferences ? { ...preferences, exclusions: preferences.exclusions.map(app => ({ ...app })) } : null;
}
// The resident panel already keeps preferences current. Render its cached draft
// immediately instead of resizing the dialog after a subscription and IPC read.
const draft = ref(copyPreferences(props.preferences));
watch(
  () => props.preferences,
  preferences => {
    if (!draft.value) draft.value = copyPreferences(preferences);
  }
);
const choosing = ref(false);
const candidates = ref<ExcludedApplication[]>([]);
const search = ref('');
const selected = ref<string[]>([]);
const loading = ref(false);
const reloading = ref(false);
const editingDisabled = computed(() => props.saving || reloading.value);
const pickerFailed = ref(false);
const conflict = computed(() => !!draft.value && draft.value.revision !== props.preferences?.revision);
const filtered = computed(() =>
  candidates.value.filter(app => `${app.name} ${app.path}`.toLowerCase().includes(search.value.toLowerCase()))
);
let disposed = false;
async function reload() {
  if (reloading.value || props.saving) return;
  reloading.value = true;
  try {
    await props.reloadPreferences();
    await nextTick();
    // A failed refresh must keep the current draft. Disable edits while waiting
    // so a successful response cannot overwrite newer changes made in the form.
    if (!disposed && !props.failed) draft.value = copyPreferences(props.preferences);
  } finally {
    reloading.value = false;
  }
}
function close() {
  if (!props.saving) emit('close');
}
async function save() {
  if (!editingDisabled.value && draft.value && !conflict.value && (await props.savePreferences(draft.value))) close();
}
async function choose() {
  if (editingDisabled.value) return;
  choosing.value = true;
  selected.value = [];
  search.value = '';
  loading.value = true;
  pickerFailed.value = false;
  try {
    candidates.value = await MemoryReleaseService.applications();
  } catch (error) {
    pickerFailed.value = true;
    LoggerService.warn('monitoring', 'memory_exclusion_candidates_failed', { error });
  } finally {
    loading.value = false;
  }
}
function isAdded(path: string) {
  return draft.value?.exclusions.some(app => app.path.toLowerCase() === path.toLowerCase());
}
function toggleCandidate(path: string) {
  if (isAdded(path)) return;
  selected.value = selected.value.includes(path)
    ? selected.value.filter(value => value !== path)
    : [...selected.value, path];
}
function addSelected() {
  if (!draft.value || editingDisabled.value) return;
  draft.value.exclusions.push(
    ...candidates.value.filter(app => selected.value.includes(app.path) && !isAdded(app.path))
  );
  choosing.value = false;
}
function remove(path: string) {
  if (draft.value) draft.value.exclusions = draft.value.exclusions.filter(app => app.path !== path);
}
onMounted(() => {
  // The native settings adapter owns its initial read and subscription. Inline
  // settings only request a read when the resident panel has no cached value.
  if (props.dialog && !draft.value) void reload();
});
onBeforeUnmount(() => {
  disposed = true;
});
</script>

<template>
  <section class="release-settings" :class="{ 'release-settings-dialog': dialog }">
    <component :is="dialog ? MdDialogHeader : 'header'" class="flex-none border-b border-border/70">
      <component :is="dialog ? DialogTitle : 'h1'">{{
        t(choosing ? 'memoryRelease.addTitle' : 'memoryRelease.title')
      }}</component>
      <component :is="dialog ? DialogDescription : 'p'" v-if="choosing">{{
        t('memoryRelease.addDescription')
      }}</component>
    </component>
    <div class="settings-body scrollbar-stable">
      <div v-if="props.failed || conflict" class="notice" role="alert">
        <span>{{ t(conflict ? 'memoryRelease.conflict' : 'memoryRelease.failed') }}</span>
        <button class="page-action" type="button" :disabled="editingDisabled" @click="reload">
          {{ t('memoryRelease.reload') }}
        </button>
      </div>
      <template v-if="choosing">
        <MdResultSearch
          v-model="search"
          class="candidate-search"
          :aria-label="t('memoryRelease.search')"
          :placeholder="t('memoryRelease.searchPlaceholder')"
        />
        <p v-if="loading" class="empty" role="status">{{ t('memoryRelease.loading') }}</p>
        <div v-else-if="pickerFailed" class="notice" role="alert">
          <span>{{ t('memoryRelease.failed') }}</span
          ><button class="page-action" @click="choose">{{ t('memoryRelease.reload') }}</button>
        </div>
        <p v-else-if="!filtered.length" class="empty">{{ t('memoryRelease.noMatches') }}</p>
        <ul v-else class="application-list">
          <li v-for="app in filtered" :key="app.path">
            <label class="candidate"
              ><MdCheckbox
                :model-value="!!isAdded(app.path) || selected.includes(app.path)"
                :value="app.path"
                :disabled="isAdded(app.path) || editingDisabled"
                :aria-label="app.name"
                @update:model-value="toggleCandidate(app.path)"
              /><MdNativeFileIcon :path="app.path" :name="app.name" compact /><span class="app-copy"
                ><strong>{{ app.name }}</strong
                ><MdTooltip :text="app.path"
                  ><small>{{ app.path }}</small></MdTooltip
                ></span
              ><span v-if="isAdded(app.path)" class="badge">{{ t('memoryRelease.excluded') }}</span></label
            >
          </li>
        </ul>
      </template>
      <template v-else-if="draft">
        <MdSettingsGroup plain>
          <MdSettingsRow
            compact
            :title="t('memoryRelease.automatic')"
            :description="t('memoryRelease.automaticDescription')"
            label-for="automatic-release"
            ><MdSwitch id="automatic-release" v-model="draft.automatic" :disabled="editingDisabled"
          /></MdSettingsRow>
          <div class="schedule-fields">
            <div class="schedule-field">
              <label id="release-interval-label" for="release-interval">{{ t('memoryRelease.interval') }}</label>
              <Select v-model="draft.intervalMinutes" :disabled="!draft.automatic || editingDisabled">
                <SelectTrigger id="release-interval" class="w-full" size="sm" aria-labelledby="release-interval-label"
                  ><SelectValue
                /></SelectTrigger>
                <SelectContent>
                  <SelectItem v-for="minutes in [3, 5, 15, 30, 60, 120]" :key="minutes" :value="minutes">{{
                    t('memoryRelease.minutes', { count: minutes })
                  }}</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <div class="schedule-field">
              <label id="release-threshold-label" for="release-threshold">{{ t('memoryRelease.threshold') }}</label>
              <Select v-model="draft.thresholdPercent" :disabled="!draft.automatic || editingDisabled">
                <SelectTrigger id="release-threshold" class="w-full" size="sm" aria-labelledby="release-threshold-label"
                  ><SelectValue
                /></SelectTrigger>
                <SelectContent>
                  <SelectItem v-for="percent in [70, 80, 90]" :key="percent" :value="percent"
                    >{{ percent }}%</SelectItem
                  >
                  <SelectItem :value="0">{{ t('memoryRelease.anyUsage') }}</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <label v-if="windows" class="foreground"
              ><MdCheckbox
                v-model="draft.skipForeground"
                class="mt-0.5"
                :disabled="!draft.automatic || editingDisabled"
              />{{ t('memoryRelease.skipForeground') }}</label
            >
          </div>
        </MdSettingsGroup>
        <section v-if="windows" class="exclusion-section">
          <div class="section-heading">
            <div class="exclusion-heading">
              <h2>
                {{ t('memoryRelease.exclusions') }} <span>{{ draft.exclusions.length }}</span>
              </h2>
              <MdTooltip :text="t('memoryRelease.exclusionsHint')">
                <button type="button" class="md-help-action" :aria-label="t('memoryRelease.exclusionsHint')">
                  <MdIcon :name="ICON_NAMES.help" :size="15" />
                </button>
              </MdTooltip>
            </div>
            <button class="page-action" type="button" :disabled="editingDisabled" @click="choose">
              {{ t('memoryRelease.add') }}
            </button>
          </div>
          <p v-if="!draft.exclusions.length" class="empty">{{ t('memoryRelease.empty') }}</p>
          <ul v-else class="application-list">
            <li v-for="app in draft.exclusions" :key="app.path" class="excluded-row">
              <MdNativeFileIcon :path="app.path" :name="app.name" compact /><span class="app-copy"
                ><strong>{{ app.name }}</strong
                ><MdTooltip :text="app.path"
                  ><small>{{ app.path }}</small></MdTooltip
                ></span
              ><button
                class="page-action"
                type="button"
                :disabled="editingDisabled"
                :aria-label="t('memoryRelease.removeNamed', { name: app.name })"
                @click="remove(app.path)"
              >
                {{ t('memoryRelease.remove') }}
              </button>
            </li>
          </ul>
        </section>
      </template>
      <p v-else-if="!props.failed" class="empty">{{ t('memoryRelease.loading') }}</p>
    </div>
    <component :is="dialog ? MdDialogFooter : 'footer'" v-if="choosing" class="settings-footer flex-row flex-wrap">
      <Button variant="outline" size="sm" @click="choosing = false">{{ t('memoryRelease.back') }}</Button
      ><Button
        class="primary"
        size="sm"
        :disabled="!selected.length || loading || editingDisabled"
        @click="addSelected"
      >
        {{ t('memoryRelease.addSelected', { count: selected.length }) }}
      </Button>
    </component>
    <component :is="dialog ? MdDialogFooter : 'footer'" v-else class="settings-footer flex-row flex-wrap">
      <Button variant="outline" size="sm" :disabled="props.saving" @click="close">{{
        t('memoryRelease.cancel')
      }}</Button
      ><Button class="primary" size="sm" :disabled="!draft || editingDisabled || conflict" @click="save">
        {{ t(props.saving ? 'memoryRelease.saving' : 'memoryRelease.save') }}
      </Button>
    </component>
  </section>
</template>

<style scoped>
@reference "@assets/main.css";
.release-settings {
  @apply bg-card text-card-foreground;
  display: flex;
  flex-direction: column;
  /* Follow the WebView content bounds. Older WKWebView versions can size
     dynamic viewport units beyond the visible native window content area. */
  height: 100%;
  min-height: 0;
  overflow: hidden;
}
.release-settings:not(.release-settings-dialog) > header {
  flex: none;
  padding: 16px 18px;
  border-bottom: 1px solid var(--border);
}
h1 {
  font-size: 18px;
  font-weight: 650;
}
.release-settings:not(.release-settings-dialog) > header p {
  @apply text-muted-foreground;
  font-size: 12px;
  line-height: 1.6;
  margin-top: 7px;
}
.release-settings-dialog {
  height: auto;
  max-height: calc(100vh - var(--layout-dialog-viewport-inset) - var(--layout-dialog-viewport-inset));
}
.settings-body {
  padding: 12px 18px;
  flex: 1;
  min-height: 0;
  overflow-y: auto;
}
.schedule-fields {
  padding-top: 4px;
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 10px;
}
.schedule-field,
.schedule-fields > label {
  display: flex;
  flex-direction: column;
  gap: 6px;
  font-size: 12px;
  min-width: 0;
}
.schedule-fields > .foreground {
  grid-column: 1 / -1;
  flex-direction: row;
  align-items: flex-start;
}
.candidate-search.result-search {
  width: 100%;
  min-width: 0;
  max-width: none;
}
.exclusion-section {
  margin-top: 16px;
}
.section-heading {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 12px;
}
.exclusion-heading {
  display: flex;
  align-items: center;
  gap: 6px;
}
h2 {
  font-size: 14px;
  font-weight: 600;
}
h2 span {
  @apply text-muted-foreground;
  font-weight: 400;
  margin-left: 6px;
}
.page-action {
  @apply text-primary rounded-md;
  padding: 7px 10px;
  cursor: pointer;
  font-size: 12px;
  flex: none;
  background-color: transparent;
}
.page-action:hover:not(:disabled) {
  background-color: var(--accent);
}
.page-action:disabled {
  opacity: 0.5;
  cursor: default;
}
.page-action:focus-visible {
  outline: 2px solid var(--ring);
  outline-offset: 2px;
}
.application-list {
  padding: 0;
  margin: 8px 0 0;
  list-style: none;
}
.application-list li {
  border-bottom: 1px solid var(--border);
}
.excluded-row,
.candidate {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 9px 0;
}
.candidate {
  cursor: pointer;
}
.app-copy {
  display: flex;
  flex-direction: column;
  gap: 3px;
  min-width: 0;
  flex: 1;
}
.app-copy strong {
  font-size: 13px;
  font-weight: 550;
  overflow-wrap: anywhere;
}
.app-copy small {
  @apply text-muted-foreground;
  font-size: 11px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.badge {
  @apply text-muted-foreground;
  font-size: 11px;
  flex: none;
}
.empty {
  @apply text-muted-foreground;
  padding: 16px 0;
  font-size: 12px;
  text-align: center;
}
.notice {
  @apply text-destructive bg-muted rounded-md;
  padding: 12px;
  margin-bottom: 14px;
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
  font-size: 12px;
}
.settings-footer {
  flex: none;
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  min-height: var(--layout-dialog-footer-height);
  padding: var(--layout-dialog-footer-padding);
  @apply border-t border-border/70 bg-muted/20;
  align-items: center;
}
</style>

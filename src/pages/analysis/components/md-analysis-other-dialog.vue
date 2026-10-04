<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, shallowRef, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { useVirtualizer } from '@tanstack/vue-virtual';

import MdDialogContent from '@/components/custom/md-dialog-content.vue';
import MdDialogFooter from '@/components/custom/md-dialog-footer.vue';
import MdDialogHeader from '@/components/custom/md-dialog-header.vue';
import MdSpinner from '@/components/custom/md-spinner.vue';
import MdTooltip from '@/components/custom/md-tooltip.vue';
import { Button } from '@/components/ui/button';
import { Dialog, DialogDescription, DialogTitle } from '@/components/ui/dialog';
import type { AnalysisRemainderPage, AnalysisRemainderSelection, DirectoryEntryInfo } from '@/lib/models/analysis';
import { LOG_DOMAINS, LOG_EVENTS } from '@/lib/models/telemetry';
import { AnalysisService } from '@/lib/services/analysis-service';
import { ByteSizeService } from '@/lib/services/byte-size-service';
import { LoggerService } from '@/lib/services/logger-service';
import * as FormatUtils from '@/lib/utils/format';
import MdAnalysisEntryIcon from './md-analysis-entry-icon.vue';

const props = defineProps<{ scanId: number; selection: AnalysisRemainderSelection | null }>();
const emit = defineEmits<{ close: []; navigate: [path: string]; refreshDirectory: [path: string] }>();
const { t } = useI18n({ useScope: 'global' });
const entries = shallowRef<DirectoryEntryInfo[]>([]);
const page = shallowRef<AnalysisRemainderPage | null>(null);
const loading = ref(false);
const failed = ref(false);
const open = computed(() => props.selection !== null);
const bodyElement = ref<HTMLElement | null>(null);
// Paging retains the listing, while only viewport rows mount native icons and
// tooltip components. Fixed compact row geometry keeps fast scrolling predictable.
const rowHeight = 36;
const virtualizer = useVirtualizer(
  computed(() => ({
    count: entries.value.length,
    getScrollElement: () => bodyElement.value,
    getItemKey: (index: number) => entries.value[index]?.path ?? index,
    estimateSize: () => rowHeight,
    initialRect: { width: 600, height: 400 },
    overscan: 8,
  }))
);
const rows = computed(() =>
  virtualizer.value.getVirtualItems().flatMap(row => {
    const entry = entries.value[row.index];
    return entry ? [{ row, entry }] : [];
  })
);
let generation = 0;
let openingFrame: number | null = null;

function cancelOpeningLoad() {
  if (openingFrame !== null) cancelAnimationFrame(openingFrame);
  openingFrame = null;
}

async function loadAfterOpening() {
  const requestGeneration = generation;
  const isCurrent = () => requestGeneration === generation && props.selection !== null;
  await nextTick();
  if (!isCurrent()) return;
  openingFrame = requestAnimationFrame(async () => {
    openingFrame = null;
    // Let the modal paint and finish its own opening animation before mounting
    // a page of rows and native icons. Reduced motion needs no timed delay.
    const content = bodyElement.value?.closest<HTMLElement>('[data-slot="dialog-content"]');
    await Promise.allSettled(content?.getAnimations().map(animation => animation.finished) ?? []);
    if (!isCurrent()) return;
    openingFrame = requestAnimationFrame(() => {
      openingFrame = null;
      if (!isCurrent()) return;
      loading.value = false;
      void loadPage();
    });
  });
}

function releaseSnapshot(snapshotId: number | undefined) {
  if (snapshotId === undefined) return;
  void AnalysisService.releaseRemainder(snapshotId).catch(error => {
    LoggerService.warn(LOG_DOMAINS.analysis, LOG_EVENTS.operationFailed, {
      operation: 'release_analysis_remainder',
      snapshotId,
      error,
    });
  });
}

async function loadPage(offset = 0) {
  const selection = props.selection;
  if (!selection || loading.value) return;
  const requestGeneration = generation;
  loading.value = true;
  failed.value = false;
  try {
    const snapshotId = offset === 0 ? null : (page.value?.snapshotId ?? null);
    const result = await AnalysisService.listRemainder(props.scanId, selection, offset, snapshotId);
    if (requestGeneration !== generation || props.selection !== selection) {
      releaseSnapshot(result.snapshotId);
      return;
    }
    const replaced = offset === 0 || result.snapshotId !== snapshotId;
    entries.value = replaced ? result.entries : [...entries.value, ...result.entries];
    if (replaced && page.value?.snapshotId !== result.snapshotId) releaseSnapshot(page.value?.snapshotId);
    page.value = result;
    if (replaced && bodyElement.value) bodyElement.value.scrollTop = 0;
  } catch (error) {
    if (requestGeneration !== generation || props.selection !== selection) return;
    failed.value = true;
    LoggerService.warn(LOG_DOMAINS.analysis, LOG_EVENTS.operationFailed, {
      operation: 'list_analysis_remainder',
      scanId: props.scanId,
      parentPath: selection.parentPath,
      offset,
      error,
    });
  } finally {
    if (requestGeneration === generation) loading.value = false;
  }
}

function navigate(path: string) {
  emit('close');
  emit('navigate', path);
}
function refreshDirectory() {
  const path = props.selection?.parentPath;
  if (!path) return;
  emit('close');
  emit('refreshDirectory', path);
}
watch(
  [() => props.selection, () => props.scanId],
  () => {
    generation++;
    cancelOpeningLoad();
    releaseSnapshot(page.value?.snapshotId);
    entries.value = [];
    page.value = null;
    loading.value = props.selection !== null;
    failed.value = false;
    if (props.selection) void loadAfterOpening();
  },
  { immediate: true }
);
onBeforeUnmount(() => {
  generation++;
  cancelOpeningLoad();
  releaseSnapshot(page.value?.snapshotId);
});
</script>

<template>
  <Dialog :open="open" @update:open="!$event && emit('close')">
    <MdDialogContent size="large" class="flex flex-col">
      <MdDialogHeader>
        <DialogTitle
          >{{ t('analysis.other') }} ·
          {{ ByteSizeService.bytes(page?.totalBytes ?? selection?.bytes ?? 0) }}</DialogTitle
        >
        <DialogDescription class="other-parent">{{ selection?.parentPath }}</DialogDescription>
      </MdDialogHeader>
      <div v-if="entries.length" class="other-columns">
        <span>{{ t('analysis.name') }}</span>
        <span>{{ t('analysis.size') }}</span>
      </div>
      <div ref="bodyElement" class="other-body scrollbar-stable" :aria-busy="loading">
        <div class="other-virtual-content" :style="{ height: `${virtualizer.getTotalSize()}px` }">
          <div class="other-virtual-window" :style="{ top: `${rows[0]?.row.start ?? 0}px` }">
            <MdTooltip v-for="{ entry } in rows" :key="entry.path" :text="entry.path">
              <button type="button" class="other-entry" :disabled="!entry.isDirectory" @click="navigate(entry.path)">
                <MdAnalysisEntryIcon :entry="entry" :deleting="false" compact />
                <span class="other-name">{{ entry.name }}</span>
                <span class="other-size">{{ ByteSizeService.bytes(entry.bytes) }}</span>
              </button>
            </MdTooltip>
          </div>
        </div>
        <div v-if="failed" class="other-status" role="alert">
          <p>{{ t('errors.operationFailed') }}</p>
          <div class="other-recovery">
            <Button variant="outline" size="sm" @click="loadPage(page?.nextOffset ?? 0)">{{
              t('common.retry')
            }}</Button>
            <Button variant="outline" size="sm" @click="refreshDirectory">{{ t('analysis.rescan') }}</Button>
          </div>
        </div>
        <div v-else-if="loading" class="other-status" role="status"><MdSpinner /></div>
        <div v-else-if="page && page.nextOffset !== null" class="other-status">
          <Button variant="outline" size="sm" @click="loadPage(page.nextOffset ?? 0)">{{
            t('common.loadMore')
          }}</Button>
        </div>
      </div>
      <MdDialogFooter align="between">
        <span v-if="page" class="other-count"
          >{{ FormatUtils.integer(entries.length) }} /
          {{ t('common.itemCount', { count: FormatUtils.integer(page.totalCount) }, page.totalCount) }}</span
        >
        <Button variant="outline" @click="emit('close')">{{ t('common.close') }}</Button>
      </MdDialogFooter>
    </MdDialogContent>
  </Dialog>
</template>

<style scoped>
@reference "@assets/main.css";
.other-parent {
  display: -webkit-box;
  overflow: hidden;
  -webkit-box-orient: vertical;
  -webkit-line-clamp: 2;
  overflow-wrap: anywhere;
}
.other-body {
  flex: 1;
  min-height: 0;
  max-height: min(400px, 55dvh);
  overflow: auto;
  padding: 0 var(--layout-dialog-body-inline-padding) 12px;
}
.other-columns {
  display: flex;
  justify-content: space-between;
  padding: 0 calc(var(--layout-dialog-body-inline-padding) + 10px) 6px;
  @apply text-muted-foreground;
  font-size: var(--font-content-meta);
}
.other-virtual-content {
  position: relative;
}
.other-virtual-window {
  position: absolute;
  inset-inline: 0;
}
.other-entry {
  height: 36px;
  display: flex;
  width: 100%;
  min-width: 0;
  align-items: center;
  gap: 8px;
  border: 0;
  border-radius: 6px;
  padding: 3px 10px;
  background: transparent;
  @apply text-card-foreground;
  text-align: left;
  font: inherit;
  font-size: var(--font-content-secondary);
  cursor: pointer;
}
.other-entry:hover:enabled {
  @apply bg-accent/65;
}
.other-entry:focus-visible {
  @apply outline-none ring-2 ring-inset ring-ring/45;
}
.other-entry:disabled {
  cursor: default;
}
.other-name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.other-size {
  flex: none;
  font-variant-numeric: tabular-nums;
}
.other-status {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
  padding: 16px;
  @apply text-muted-foreground;
  font-size: var(--font-content-secondary);
}
.other-body:not(:has(.other-entry)) .other-status {
  min-height: 120px;
  justify-content: center;
}
.other-recovery {
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  gap: 8px;
}
.other-count {
  @apply text-muted-foreground;
  font-size: var(--font-content-meta);
}
</style>

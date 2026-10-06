<script setup lang="ts">
import MdTooltip from '@/components/custom/md-tooltip.vue';
import { useI18n } from 'vue-i18n';
import {
  computed,
  nextTick,
  onActivated,
  onBeforeUnmount,
  onDeactivated,
  onMounted,
  ref,
  watch,
  type VNode,
} from 'vue';
import { observeElementRect, useVirtualizer, type Range } from '@tanstack/vue-virtual';

import MdAnalysisEntryIcon from './md-analysis-entry-icon.vue';
import MdFileEntryContextMenu from '@/components/custom/md-file-entry-context-menu.vue';
import MdIconAction from '@/components/custom/md-icon-action.vue';
import MdResultTable from '@/components/custom/md-result-table.vue';
import MdResultTableRow from '@/components/custom/md-result-table-row.vue';
import MdIcon from '@/components/icons/md-icon.vue';
import { ANALYSIS_SORT_KEYS } from '@/lib/models/analysis';
import { ICON_NAMES } from '@/lib/models/ui';
import type { DirectoryEntryInfo } from '@/lib/models/analysis';
import { SORT_DIRECTIONS } from '@/lib/models/sort';
import * as AnalysisEntryUtils from '@/lib/utils/analysis-entry';
import { type AnalysisSortKey, type SortDirection } from '@/lib/utils/analysis-entry';
import { ByteSizeService } from '@/lib/services/byte-size-service';
import * as FormatUtils from '@/lib/utils/format';

const { locale, t } = useI18n({ useScope: 'global' });

const props = defineProps<{
  entries: DirectoryEntryInfo[];
  openDisabled: boolean;
  deleteDisabled: boolean;
  deletingPath?: string | null;
}>();

const emit = defineEmits<{
  activate: [entry: DirectoryEntryInfo];
  openEntry: [entry: DirectoryEntryInfo];
  reveal: [path: string];
  delete: [entry: DirectoryEntryInfo];
}>();

const sortKey = ref<AnalysisSortKey>(ANALYSIS_SORT_KEYS.bytes);
const sortDirection = ref<SortDirection>(SORT_DIRECTIONS.descending);

const sortedEntries = computed(() => AnalysisEntryUtils.sort(props.entries, sortKey.value, sortDirection.value));

const table = ref<InstanceType<typeof MdResultTable> | null>(null);
const viewportHeight = ref(600);
const rowHeight = 44;
const overscan = 40;
const poolSize = computed(() => Math.ceil(viewportHeight.value / rowHeight) + overscan * 2 + 1);
const observeVisibleRect: typeof observeElementRect<HTMLElement> = (instance, update) =>
  observeElementRect(instance, rect => {
    if (rect.height > 0) {
      viewportHeight.value = rect.height;
      update(rect);
    }
  });
function pooledRange(capacity: number) {
  return (range: Range) => {
    const length = Math.min(range.count, capacity);
    const start = Math.max(0, Math.min(range.startIndex - overscan, range.count - length));
    return Array.from({ length }, (_, offset) => start + offset);
  };
}
const virtualizer = useVirtualizer(
  computed(() => ({
    count: sortedEntries.value.length,
    getScrollElement: () => table.value?.getScrollElement?.() ?? null,
    observeElementRect: observeVisibleRect,
    initialRect: { width: 800, height: 600 },
    useAnimationFrameWithResizeObserver: true,
    getItemKey: (index: number) => sortedEntries.value[index]?.path ?? index,
    estimateSize: () => rowHeight,
    overscan,
    rangeExtractor: pooledRange(poolSize.value),
  }))
);
const rows = computed(() =>
  virtualizer.value.getVirtualItems().flatMap(row => {
    const entry = sortedEntries.value[row.index];
    return entry ? [{ row, entry }] : [];
  })
);
let active = true;
let lastScrollTop = 0;
function rememberScroll() {
  const element = table.value?.getScrollElement?.();
  if (active && element?.isConnected) lastScrollTop = element.scrollTop;
}
function releaseRecycledFocus(next: VNode, previous: VNode) {
  if (next.props?.['data-entry-key'] === previous.props?.['data-entry-key']) return;
  const element = previous.el;
  const focused = element instanceof HTMLElement ? element.ownerDocument.activeElement : null;
  if (focused instanceof HTMLElement && element instanceof HTMLElement && element.contains(focused)) focused.blur();
}
watch(
  sortedEntries,
  () => {
    lastScrollTop = 0;
    table.value?.scrollTo({ top: 0 });
    virtualizer.value.scrollToOffset(0);
  },
  { flush: 'post' }
);
onMounted(() => {
  table.value?.getScrollElement?.()?.addEventListener('scroll', rememberScroll, { passive: true });
});
onDeactivated(() => {
  active = false;
});
onActivated(() => {
  active = true;
  void nextTick(() => virtualizer.value.scrollToOffset(lastScrollTop));
});
onBeforeUnmount(() => {
  table.value?.getScrollElement?.()?.removeEventListener('scroll', rememberScroll);
});

function changeSort(key: AnalysisSortKey) {
  if (sortKey.value === key) {
    sortDirection.value =
      sortDirection.value === SORT_DIRECTIONS.ascending ? SORT_DIRECTIONS.descending : SORT_DIRECTIONS.ascending;
    return;
  }
  sortKey.value = key;
  sortDirection.value = key === ANALYSIS_SORT_KEYS.name ? SORT_DIRECTIONS.ascending : SORT_DIRECTIONS.descending;
}

function sortIndicator(key: AnalysisSortKey) {
  if (sortKey.value !== key) return ICON_NAMES.arrowUpDown;
  return sortDirection.value === SORT_DIRECTIONS.ascending ? ICON_NAMES.arrowUp : ICON_NAMES.arrowDown;
}

function sortActionLabel(key: AnalysisSortKey) {
  return sortKey.value === key && sortDirection.value === SORT_DIRECTIONS.ascending
    ? t('analysis.sortDescending')
    : t('analysis.sortAscending');
}

function sortControlLabel(key: AnalysisSortKey, column: string) {
  return t('analysis.sortColumn', {
    column,
    direction: sortActionLabel(key),
  });
}
</script>

<template>
  <MdResultTable ref="table" class="details-view" synchronous-scroll>
    <template #header>
      <div
        class="details-head-grid grid-cols-[minmax(178px,1fr)_90px_72px] @5xl/analysis:grid-cols-[minmax(188px,1fr)_100px_85px_110px]"
      >
        <button
          type="button"
          class="md-result-sort flex h-full items-center justify-start gap-1"
          :data-active="sortKey === ANALYSIS_SORT_KEYS.name"
          :aria-label="sortControlLabel(ANALYSIS_SORT_KEYS.name, t('analysis.name'))"
          @click="changeSort(ANALYSIS_SORT_KEYS.name)"
        >
          {{ t('analysis.name') }}
          <MdIcon :name="sortIndicator(ANALYSIS_SORT_KEYS.name)" :size="13" />
        </button>
        <button
          type="button"
          class="details-number md-result-sort flex h-full items-center justify-end gap-1"
          :data-active="sortKey === ANALYSIS_SORT_KEYS.bytes"
          :aria-label="sortControlLabel(ANALYSIS_SORT_KEYS.bytes, t('analysis.size'))"
          @click="changeSort(ANALYSIS_SORT_KEYS.bytes)"
        >
          {{ t('analysis.size') }}
          <MdIcon :name="sortIndicator(ANALYSIS_SORT_KEYS.bytes)" :size="13" />
        </button>
        <button
          type="button"
          class="details-number md-result-sort flex h-full items-center justify-end gap-1"
          :data-active="sortKey === ANALYSIS_SORT_KEYS.fileCount"
          :aria-label="sortControlLabel(ANALYSIS_SORT_KEYS.fileCount, t('analysis.fileCount'))"
          @click="changeSort(ANALYSIS_SORT_KEYS.fileCount)"
        >
          {{ t('analysis.fileCount') }}
          <MdIcon :name="sortIndicator(ANALYSIS_SORT_KEYS.fileCount)" :size="13" />
        </button>
        <button
          type="button"
          class="details-modified md-result-sort hidden h-full items-center justify-end gap-1 @5xl/analysis:flex"
          :data-active="sortKey === ANALYSIS_SORT_KEYS.modified"
          :aria-label="sortControlLabel(ANALYSIS_SORT_KEYS.modified, t('analysis.modified'))"
          @click="changeSort(ANALYSIS_SORT_KEYS.modified)"
        >
          {{ t('analysis.modified') }}
          <MdIcon :name="sortIndicator(ANALYSIS_SORT_KEYS.modified)" :size="13" />
        </button>
      </div>
    </template>

    <div class="virtual-content" :style="{ height: `${virtualizer.getTotalSize()}px` }">
      <div class="virtual-window" :style="{ top: `${rows[0]?.row.start ?? 0}px` }">
        <div
          v-for="{ row, entry } in rows"
          :key="row.index % poolSize"
          class="virtual-row"
          :data-index="row.index"
          :data-entry-key="entry.path"
          @vue:before-update="releaseRecycledFocus"
        >
          <MdFileEntryContextMenu
            :entry-key="entry.path"
            :open-disabled="openDisabled"
            :delete-disabled="deleteDisabled"
            :reveal-disabled="deletingPath === entry.path"
            @open="emit('openEntry', entry)"
            @reveal="emit('reveal', entry.path)"
            @delete="emit('delete', entry)"
          >
            <MdResultTableRow
              class="details-row grid-cols-[minmax(178px,1fr)_90px_72px] @5xl/analysis:grid-cols-[minmax(188px,1fr)_100px_85px_110px]"
            >
              <span class="details-primary">
                <MdTooltip :text="entry.path"
                  ><button
                    class="details-name"
                    type="button"
                    :disabled="openDisabled"
                    :aria-busy="deletingPath === entry.path || undefined"
                    @click="emit('activate', entry)"
                    @dblclick="!entry.isDirectory && emit('openEntry', entry)"
                    @keydown.enter="!entry.isDirectory && emit('openEntry', entry)"
                  >
                    <MdAnalysisEntryIcon :entry="entry" :deleting="deletingPath === entry.path" compact />
                    <strong class="md-result-primary">{{ entry.name }}</strong>
                    <span v-if="deletingPath === entry.path" class="deletion-label">{{ t('analysis.deleting') }}</span>
                  </button></MdTooltip
                >
                <span class="details-actions">
                  <MdIconAction
                    variant="ghost"
                    :label="t('common.open')"
                    :disabled="openDisabled"
                    @click="emit('openEntry', entry)"
                  >
                    <MdIcon :name="ICON_NAMES.external" :size="16" />
                  </MdIconAction>
                  <MdIconAction
                    variant="ghost"
                    :label="t('common.showInFileManager')"
                    :disabled="deletingPath === entry.path"
                    @click="emit('reveal', entry.path)"
                  >
                    <MdIcon :name="ICON_NAMES.folder" :size="16" />
                  </MdIconAction>
                </span>
              </span>
              <strong class="details-number md-result-primary">{{ ByteSizeService.bytes(entry.bytes) }}</strong>
              <span class="details-number">{{ FormatUtils.integer(entry.fileCount) }}</span>
              <span class="details-modified hidden @5xl/analysis:block">{{
                FormatUtils.dateTime(entry.modifiedAtMs, locale)
              }}</span>
            </MdResultTableRow>
          </MdFileEntryContextMenu>
        </div>
      </div>
    </div>
  </MdResultTable>
</template>

<style scoped>
@reference "@assets/main.css";

.virtual-content,
.virtual-window {
  position: relative;
  width: 100%;
  overflow-anchor: none;
}

.virtual-window {
  /* Paint the bounded row window independently of the full scroll surface. */
  transform: translateZ(0);
}

.details-view {
  display: flex;
  min-height: 0;
  flex: 1;
  flex-direction: column;
  overflow: hidden;
  margin: 0 16px 12px;
  border-width: 1px;
  border-radius: 9px;
  @apply border-border/70;
}

.details-view :deep(.result-table-header) {
  @apply border-border/70 bg-muted/35;
}

.details-head-grid,
.details-row {
  display: grid;
  align-items: center;
  gap: 12px;
}

.details-head-grid {
  height: var(--layout-result-header-height);
  font-size: var(--font-content-meta);
}

.details-head-grid button {
  border: 0;
  background: transparent;
  color: inherit;
  font: inherit;
  text-align: left;
  cursor: pointer;
}

.details-row {
  height: 44px;
  @apply text-card-foreground;
  font-size: var(--font-content-body);
}

.details-number,
.details-modified {
  font-variant-numeric: tabular-nums;
  text-align: right;
}

.details-primary {
  position: relative;
  display: flex;
  min-width: 0;
  align-items: center;
}

.details-name {
  display: flex;
  min-width: 0;
  flex: 1;
  align-items: center;
  gap: 11px;
  border: 0;
  padding: 0;
  background: transparent;
  color: inherit;
  font: inherit;
  text-align: left;
  cursor: pointer;
}

.details-name strong {
  min-width: 0;
  flex: 1;
  overflow: hidden;
  font-size: var(--font-content-primary);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.deletion-label {
  flex: none;
  @apply text-muted-foreground;
  font-size: var(--font-content-secondary);
}

.details-actions {
  position: absolute;
  right: 0;
  display: flex;
  opacity: 0;
  pointer-events: none;
  transition: opacity 0.14s ease;
}

.details-row:is(:hover, :has(:focus-visible)) .details-actions {
  opacity: 1;
  pointer-events: auto;
}

.details-row:is(:hover, :has(:focus-visible)) .details-name strong {
  padding-right: 64px;
}
</style>

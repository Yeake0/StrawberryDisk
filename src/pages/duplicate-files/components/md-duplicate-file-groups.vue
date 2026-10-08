<script setup lang="ts">
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
import { observeElementRect, useVirtualizer, type Range, type VirtualItem } from '@tanstack/vue-virtual';

import MdAiAction from '@/layouts/components/md-ai-action.vue';
import MdLoadMoreButton from '@/components/custom/md-load-more-button.vue';
import MdFileEntryContextMenu from '@/components/custom/md-file-entry-context-menu.vue';
import MdIconAction from '@/components/custom/md-icon-action.vue';
import MdMiddleEllipsis from '@/components/custom/md-middle-ellipsis.vue';
import MdNativeFileIcon from '@/components/custom/md-native-file-icon.vue';
import MdResultCheckbox from '@/components/custom/md-result-checkbox.vue';
import MdResultTable from '@/components/custom/md-result-table.vue';
import MdResultTableRow from '@/components/custom/md-result-table-row.vue';
import MdIcon from '@/components/icons/md-icon.vue';
import { Button } from '@/components/ui/button';
import { ICON_NAMES } from '@/lib/models/ui';
import {
  DUPLICATE_ENTRY_DELETE_POLICIES,
  DUPLICATE_GROUP_KINDS,
  DUPLICATE_ENTRY_RENDER_BATCH_SIZE,
  DUPLICATE_GROUP_RENDER_BATCH_SIZE,
  type DuplicateFileEntry,
  type DuplicateGroup,
  type DuplicateKeeperRuleId,
} from '@/lib/models/duplicate-file';
import { FILE_CATEGORY_IDS, type FileCategoryId } from '@/lib/models/file-category';
import * as DuplicateFileSelectionUtils from '@/lib/utils/duplicate-file-selection';
import * as DuplicateFileGroupUtils from '@/lib/utils/duplicate-file-group';
import { ByteSizeService } from '@/lib/services/byte-size-service';
import * as FormatUtils from '@/lib/utils/format';
import * as PathUtils from '@/lib/utils/path';
import * as RenderBatchUtils from '@/lib/utils/render-batch';

const { locale, t } = useI18n({ useScope: 'global' });

const props = defineProps<{
  scanId: number;
  category: FileCategoryId;
  groups: DuplicateGroup[];
  keeperRule: DuplicateKeeperRuleId;
  selectedPaths: string[];
  selectionDisabled: boolean;
  openDisabled: boolean;
  deleteDisabled: boolean;
  hasMore: boolean;
  loadingMore: boolean;
  remainingGroupCount: number;
}>();
const emit = defineEmits<{
  openEntry: [entry: DuplicateFileEntry];
  reveal: [path: string];
  delete: [entry: DuplicateFileEntry];
  explain: [group: DuplicateGroup, entry: DuplicateFileEntry];
  loadMore: [category: FileCategoryId];
  'update:selectedPaths': [paths: string[]];
}>();

const groupsScroll = ref<InstanceType<typeof MdResultTable> | null>(null);
let lastScrollTop = 0;
const active = ref(true);
const collapsedGroupIds = ref<ReadonlySet<string>>(new Set());
const selectedPathSet = computed(() => new Set(props.selectedPaths));
const suggestedPathsByGroup = computed(
  () =>
    new Map(
      props.groups.map(group => [
        group.id,
        new Set(DuplicateFileSelectionUtils.suggestedGroupPaths(group, props.keeperRule)),
      ])
    )
);
const appliedSelectionGroupIds = computed(
  () =>
    new Set(
      props.groups
        .filter(group => {
          const suggested = suggestedPathsByGroup.value.get(group.id) ?? new Set<string>();
          // An empty suggestion means the group has no deletable copies. It is
          // not an applied selection, even though every checkbox is unchecked.
          if (!suggested.size) return false;
          return group.entries.every(entry => selectedPathSet.value.has(entry.path) === suggested.has(entry.path));
        })
        .map(group => group.id)
    )
);
const visibleGroupCount = ref(DUPLICATE_GROUP_RENDER_BATCH_SIZE);
const visibleEntryCounts = ref<Record<string, number>>({});
const revealRequestedPage = ref(false);
const visibleGroups = computed(() => RenderBatchUtils.visibleItems(props.groups, visibleGroupCount.value));
// Virtualize individual members: a single group can contain thousands of files.
// Whole-group virtualization makes mounting cost proportional to group size and
// requires asynchronous height corrections while the compositor is scrolling.
type DisplayRow = {
  key: string;
  kind: 'header' | 'member' | 'more';
  group: DuplicateGroup;
  entry?: DuplicateFileEntry;
  height: number;
};
const displayRows = computed<DisplayRow[]>(() =>
  visibleGroups.value.flatMap(group => {
    const rows: DisplayRow[] = [{ key: `header:${group.id}`, kind: 'header', group, height: 50 }];
    if (collapsedGroupIds.value.has(group.id)) return rows;
    for (const entry of visibleEntries(group)) {
      rows.push({ key: `member:${group.id}:${entry.path}`, kind: 'member', group, entry, height: 40 });
    }
    if (remainingEntryCount(group)) rows.push({ key: `more:${group.id}`, kind: 'more', group, height: 42 });
    return rows;
  })
);
const viewportHeight = ref(600);
const overscan = 40;
const rowPoolSize = computed(() => Math.ceil(viewportHeight.value / 40) + overscan * 2 + 1);
function createPooledRange(capacity: number) {
  return (range: Range) => {
    const length = Math.min(range.count, capacity);
    const start = Math.max(0, Math.min(range.startIndex - overscan, range.count - length));
    return Array.from({ length }, (_, offset) => start + offset);
  };
}
// KeepAlive detaches the scroll area and reports a zero-sized viewport.
// Retain the last visible range instead of destroying its rows while hidden.
const observeVisibleRect: typeof observeElementRect<HTMLElement> = (instance, update) =>
  observeElementRect(instance, rect => {
    if (rect.height > 0) {
      viewportHeight.value = rect.height;
      update(rect);
    }
  });
const rowVirtualizer = useVirtualizer(
  computed(() => ({
    count: displayRows.value.length,
    getScrollElement: () => groupsScroll.value?.getScrollElement?.() ?? null,
    observeElementRect: observeVisibleRect,
    initialRect: { width: 800, height: 600 },
    getItemKey: (index: number) => displayRows.value[index]?.key ?? index,
    // The same height drives both layout and offsets. No per-group measurement
    // or delayed correction can move the rendered window away from the viewport.
    estimateSize: (index: number) => displayRows.value[index]?.height ?? 40,
    useAnimationFrameWithResizeObserver: true,
    overscan,
    rangeExtractor: createPooledRange(rowPoolSize.value),
  }))
);
const virtualRows = computed(() => rowVirtualizer.value.getVirtualItems());
const virtualHeight = computed(() => rowVirtualizer.value.getTotalSize());
const virtualTop = computed(() => virtualRows.value[0]?.start ?? 0);
type RenderedRow = DisplayRow & { row: VirtualItem; slot: number };
const renderedRows = computed<RenderedRow[]>(previous => {
  const items = virtualRows.value.flatMap(row => {
    const item = displayRows.value[row.index];
    return item ? [{ row, ...item }] : [];
  });
  const keys = new Set(items.map(item => item.key));
  const existing = new Map(previous?.map(item => [item.key, item]));
  const available: Record<DisplayRow['kind'], number[]> = { header: [], member: [], more: [] };
  let nextSlot = 0;
  for (const item of previous ?? []) {
    nextSlot = Math.max(nextSlot, item.slot + 1);
    if (!keys.has(item.key)) available[item.kind].push(item.slot);
  }
  // Keep overlapping controls in place and recycle outgoing controls by type.
  // Reusing a header as a member would remount its entire context-menu subtree.
  return items.map(item => ({
    ...item,
    slot: existing.get(item.key)?.slot ?? available[item.kind].pop() ?? nextSlot++,
  }));
});
const remainingVisibleGroupCount = computed(() =>
  RenderBatchUtils.remainingCountAcrossPages(props.groups.length, visibleGroups.value.length, props.remainingGroupCount)
);
const groupLabels = computed(() => DuplicateFileGroupUtils.displayLabels(props.groups));
const unselectedCountByGroup = computed(
  () =>
    new Map(
      props.groups.map(group => [
        group.id,
        group.entries.reduce((count, entry) => count + Number(!selectedPathSet.value.has(entry.path)), 0),
      ])
    )
);

watch(
  () => [props.scanId, props.category] as const,
  () => {
    // Streaming and pagination can replace the array during one scan. Reset
    // only for a new scan or category so updates do not interrupt reading.
    visibleGroupCount.value = DUPLICATE_GROUP_RENDER_BATCH_SIZE;
    visibleEntryCounts.value = {};
    revealRequestedPage.value = false;
    collapsedGroupIds.value = new Set();
    lastScrollTop = 0;
    groupsScroll.value?.scrollTo({ top: 0 });
    rowVirtualizer.value.scrollToIndex(0);
  },
  { flush: 'post' }
);

watch(
  () => props.groups.length,
  (nextCount, previousCount) => {
    if (!revealRequestedPage.value || nextCount <= previousCount) return;

    // Reveal a user-requested page as soon as it arrives. Keeping the loaded
    // page hidden would require a second click and make the counter alternate
    // between locally hidden groups and groups not fetched from Core yet.
    visibleGroupCount.value = RenderBatchUtils.nextVisibleCount(
      visibleGroupCount.value,
      nextCount,
      DUPLICATE_GROUP_RENDER_BATCH_SIZE
    );
    revealRequestedPage.value = false;
  },
  { flush: 'post' }
);

watch(
  () => [props.loadingMore, props.hasMore] as const,
  ([loading, hasMore]) => {
    // A matching page clears the request in the group-count watcher. Clear it
    // here only when pagination reaches the end without finding a match.
    if (!loading && !hasMore) revealRequestedPage.value = false;
  },
  { flush: 'post' }
);

function isSelected(path: string) {
  return selectedPathSet.value.has(path);
}

function isOnlyKeeper(entry: DuplicateFileEntry, group: DuplicateGroup) {
  return !selectedPathSet.value.has(entry.path) && unselectedCountByGroup.value.get(group.id) === 1;
}

function isProtected(entry: DuplicateFileEntry) {
  return entry.deletePolicy === DUPLICATE_ENTRY_DELETE_POLICIES.protected;
}

function toggleEntry(entry: DuplicateFileEntry, group: DuplicateGroup, selected: boolean) {
  emit(
    'update:selectedPaths',
    DuplicateFileSelectionUtils.updateEntrySelection(props.selectedPaths, entry, group, selected)
  );
}

function toggleGroupSelection(group: DuplicateGroup) {
  emit(
    'update:selectedPaths',
    DuplicateFileSelectionUtils.toggleGroupCopies(props.selectedPaths, group, props.keeperRule)
  );
}

function isGroupSelectionApplied(group: DuplicateGroup) {
  return appliedSelectionGroupIds.value.has(group.id);
}

function suggestedSelectionCount(group: DuplicateGroup) {
  return suggestedPathsByGroup.value.get(group.id)?.size ?? 0;
}

function hasGroupSelection(group: DuplicateGroup) {
  return suggestedSelectionCount(group) > 0;
}

function toggleGroup(groupId: string) {
  const next = new Set(collapsedGroupIds.value);
  if (next.has(groupId)) next.delete(groupId);
  else next.add(groupId);
  collapsedGroupIds.value = next;
}

function releaseRecycledFocus(next: VNode, previous: VNode) {
  if (next.props?.['data-entry-key'] === previous.props?.['data-entry-key']) return;
  const element = previous.el;
  const focused = element instanceof HTMLElement ? element.ownerDocument.activeElement : null;
  // A recycled checkbox or destructive action must never retain keyboard focus
  // when its slot starts representing a different file or group.
  if (focused instanceof HTMLElement && element instanceof HTMLElement && element.contains(focused)) {
    focused.blur();
  }
}

function rememberScroll() {
  const element = groupsScroll.value?.getScrollElement?.();
  if (active.value && element?.isConnected) lastScrollTop = element.scrollTop;
}

onMounted(() => {
  void nextTick(() =>
    groupsScroll.value?.getScrollElement?.()?.addEventListener('scroll', rememberScroll, { passive: true })
  );
});
onDeactivated(() => {
  active.value = false;
});
onActivated(() => {
  active.value = true;
  void nextTick(() => rowVirtualizer.value.scrollToOffset(lastScrollTop));
});
onBeforeUnmount(() => {
  groupsScroll.value?.getScrollElement?.()?.removeEventListener('scroll', rememberScroll);
});

function visibleEntries(group: DuplicateGroup): DuplicateFileEntry[] {
  const limit = visibleEntryCounts.value[group.id] ?? DUPLICATE_ENTRY_RENDER_BATCH_SIZE;
  return RenderBatchUtils.visibleItems(group.entries, limit);
}

function remainingEntryCount(group: DuplicateGroup): number {
  return RenderBatchUtils.remainingCount(group.entries.length, visibleEntries(group).length);
}

function loadMoreEntries(group: DuplicateGroup) {
  const current = visibleEntryCounts.value[group.id] ?? DUPLICATE_ENTRY_RENDER_BATCH_SIZE;
  visibleEntryCounts.value = {
    ...visibleEntryCounts.value,
    [group.id]: RenderBatchUtils.nextVisibleCount(current, group.entries.length, DUPLICATE_ENTRY_RENDER_BATCH_SIZE),
  };
}

function loadMoreGroups() {
  if (visibleGroupCount.value < props.groups.length) {
    visibleGroupCount.value = RenderBatchUtils.nextVisibleCount(
      visibleGroupCount.value,
      props.groups.length,
      DUPLICATE_GROUP_RENDER_BATCH_SIZE
    );
    return;
  }
  if (props.loadingMore || !props.hasMore) return;
  revealRequestedPage.value = true;
  emit('loadMore', props.category);
}
</script>

<template>
  <MdResultTable ref="groupsScroll" synchronous-scroll class="duplicate-groups">
    <div class="virtual-content" :style="{ height: `${virtualHeight}px` }">
      <div class="virtual-window" :style="{ top: `${virtualTop}px` }">
        <div
          v-for="{ row, group, entry, kind, key, slot } in renderedRows"
          :key="slot"
          v-memo="[
            row.key,
            row.index,
            row.size,
            group,
            entry,
            selectedPathSet,
            keeperRule,
            collapsedGroupIds,
            visibleEntryCounts,
            groupLabels,
            selectionDisabled,
            openDisabled,
            deleteDisabled,
            locale,
          ]"
          class="virtual-row"
          :class="`virtual-row-${kind}`"
          :data-index="row.index"
          :data-entry-key="key"
          :style="{ height: `${row.size}px` }"
          @vue:before-update="releaseRecycledFocus"
        >
          <header v-if="kind === 'header'" class="group-header" @click="toggleGroup(group.id)">
            <MdNativeFileIcon
              :path="group.entries[0]?.path ?? ''"
              :name="group.entries[0]?.name ?? ''"
              :directory="group.kind === DUPLICATE_GROUP_KINDS.directory"
              directory-mode="generic"
            />
            <button class="group-disclosure" type="button" :aria-expanded="!collapsedGroupIds.has(group.id)">
              <span class="group-copy">
                <strong class="md-result-primary">
                  {{ groupLabels.get(group.id) }}
                </strong>
                <small>
                  {{
                    t(
                      group.kind === DUPLICATE_GROUP_KINDS.directory
                        ? 'duplicateFiles.directoryGroupSummary'
                        : 'duplicateFiles.groupSummary',
                      {
                        count: FormatUtils.integer(group.entries.length),
                        size: ByteSizeService.bytes(group.bytesPerFile),
                        files: t(
                          'common.fileCount',
                          { count: FormatUtils.integer(group.fileCountPerEntry) },
                          group.fileCountPerEntry
                        ),
                        reclaimable: ByteSizeService.bytes(group.reclaimableBytes),
                      },
                      group.entries.length
                    )
                  }}
                </small>
              </span>
            </button>
            <Button
              class="group-select-action"
              size="sm"
              variant="ghost"
              type="button"
              :data-applied="isGroupSelectionApplied(group)"
              :disabled="selectionDisabled || !hasGroupSelection(group)"
              :aria-label="
                t(
                  !hasGroupSelection(group)
                    ? 'duplicateFiles.protectedGroupHint'
                    : isGroupSelectionApplied(group)
                      ? 'duplicateFiles.clearGroupSelectionHint'
                      : 'duplicateFiles.selectGroupHint'
                )
              "
              @click.stop="toggleGroupSelection(group)"
            >
              <MdIcon
                :name="
                  !hasGroupSelection(group)
                    ? ICON_NAMES.lock
                    : isGroupSelectionApplied(group)
                      ? ICON_NAMES.check
                      : ICON_NAMES.duplicateFiles
                "
                :size="14"
              />
              <span class="group-select-label">
                {{
                  t(
                    !hasGroupSelection(group)
                      ? 'duplicateFiles.protectedGroup'
                      : isGroupSelectionApplied(group)
                        ? 'duplicateFiles.groupSelectionApplied'
                        : 'duplicateFiles.selectGroup',
                    { count: FormatUtils.integer(suggestedSelectionCount(group)) },
                    suggestedSelectionCount(group)
                  )
                }}
              </span>
            </Button>
            <MdIcon
              class="group-chevron"
              :class="{ collapsed: collapsedGroupIds.has(group.id) }"
              :name="ICON_NAMES.chevronUp"
              :size="16"
            />
          </header>

          <MdFileEntryContextMenu
            v-else-if="entry"
            :entry-key="entry.path"
            :open-disabled="openDisabled"
            :delete-disabled="deleteDisabled || isProtected(entry)"
            @open="emit('openEntry', entry)"
            @reveal="emit('reveal', entry.path)"
            @delete="emit('delete', entry)"
          >
            <MdResultTableRow
              class="member-row grid-cols-[18px_minmax(120px,1fr)_112px] @5xl/duplicates:grid-cols-[18px_minmax(160px,1fr)_128px]"
              :data-selected="isSelected(entry.path)"
            >
              <MdResultCheckbox
                :aria-label="entry.path"
                :checked="isSelected(entry.path)"
                :disabled="selectionDisabled || isProtected(entry) || isOnlyKeeper(entry, group)"
                @update:checked="toggleEntry(entry, group, $event)"
              />
              <span class="member-primary">
                <span class="member-path">
                  <MdMiddleEllipsis :text="PathUtils.display(entry.path)" :tail-length="32" />
                </span>
                <span v-if="isProtected(entry)" class="protected-entry-label">
                  <MdIcon :name="ICON_NAMES.lock" :size="13" />
                  {{ t('duplicateFiles.protectedEntry') }}
                </span>
                <span class="member-actions">
                  <MdAiAction
                    :name="entry.name"
                    :disabled="openDisabled || deleteDisabled"
                    @explain="emit('explain', group, entry)"
                  />
                  <MdIconAction
                    variant="ghost"
                    :label="t('common.showInFileManager')"
                    @click.prevent="emit('reveal', entry.path)"
                  >
                    <MdIcon :name="ICON_NAMES.folder" :size="16" />
                  </MdIconAction>
                  <MdIconAction
                    variant="ghost"
                    :label="t('common.deletePermanently')"
                    destructive
                    :disabled="deleteDisabled || isProtected(entry)"
                    @click.prevent="emit('delete', entry)"
                  >
                    <MdIcon :name="ICON_NAMES.trash" :size="16" />
                  </MdIconAction>
                </span>
              </span>
              <span class="member-date">{{ FormatUtils.dateTime(entry.modifiedAtMs, locale) }}</span>
            </MdResultTableRow>
          </MdFileEntryContextMenu>
          <MdLoadMoreButton
            v-else-if="kind === 'more'"
            :remaining-label="
              t(
                'common.fileCount',
                { count: FormatUtils.integer(remainingEntryCount(group)) },
                remainingEntryCount(group)
              )
            "
            @load-more="loadMoreEntries(group)"
          />
        </div>
      </div>
    </div>
    <MdLoadMoreButton
      v-if="remainingVisibleGroupCount > 0 && (visibleGroups.length < groups.length || hasMore)"
      :remaining-label="
        category === FILE_CATEGORY_IDS.all
          ? t(
              'duplicateFiles.groupCount',
              { count: FormatUtils.integer(remainingVisibleGroupCount) },
              remainingVisibleGroupCount
            )
          : undefined
      "
      :disabled="loadingMore"
      :loading="loadingMore"
      @load-more="loadMoreGroups"
    />
  </MdResultTable>
</template>

<style scoped>
@reference "@assets/main.css";

.duplicate-groups {
  min-height: 0;
  flex: 1;
}

.duplicate-groups :deep(.result-table-scroll) {
  overflow-anchor: none;
}

.virtual-row {
  position: relative;
  width: 100%;
}

.virtual-row-member,
.virtual-row-more {
  padding-inline-start: calc(var(--result-table-hierarchy-indent, 34px) + 10px);
}

.virtual-row-member::before {
  position: absolute;
  inset-block: 0;
  left: var(--result-table-hierarchy-indent, 34px);
  width: 1px;
  content: '';
  @apply bg-border/65;
}

.virtual-window {
  /* Keep the bounded row window on its own layer instead of repainting tiles
     from the full scroll surface after a distant thumb jump. */
  transform: translateZ(0);
  position: relative;
  width: 100%;
}

.virtual-content {
  position: relative;
  width: 100%;
  overflow-anchor: none;
}

.group-header {
  position: relative;
  display: grid;
  height: 100%;
  grid-template-columns: 36px minmax(0, 1fr) auto 24px;
  align-items: center;
  gap: 11px;
  padding-block: 4px;
  padding-inline: var(--result-table-content-inline-padding);
  @apply text-muted-foreground;
  cursor: pointer;
}

.group-header::before {
  position: absolute;
  top: var(--result-item-background-inset, 3px);
  right: 0;
  bottom: var(--result-item-background-inset, 3px);
  left: 0;
  border-radius: 7px;
  content: '';
  @apply bg-muted/38;
  pointer-events: none;
}

.group-header > * {
  position: relative;
}

.group-disclosure {
  min-width: 0;
  border: 0;
  padding: 0;
  background: transparent;
  color: inherit;
  font: inherit;
  text-align: left;
  cursor: pointer;
}

.group-header:has(.group-disclosure:focus-visible)::before {
  box-shadow: inset 0 0 0 1px var(--focus-ring-subtle);
  box-shadow: inset 0 0 0 1px color-mix(in oklab, var(--ring) 52%, transparent);
}

.group-disclosure:focus-visible {
  outline: none;
}

.group-copy {
  display: flex;
  min-width: 0;
  flex-direction: column;
  gap: 3px;
}

.group-copy > strong {
  @apply text-card-foreground;
}

.group-copy strong {
  overflow: hidden;
  font-size: var(--font-content-primary);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.group-copy small {
  overflow: hidden;
  @apply text-muted-foreground;
  font-size: var(--font-content-secondary);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.group-chevron {
  justify-self: center;
  pointer-events: none;
  @apply text-muted-foreground transition-transform duration-200;
}

.group-chevron.collapsed {
  transform: rotate(180deg);
}

.group-select-action {
  height: 30px;
  gap: 5px;
  border-width: 0;
  border-radius: 7px;
  padding: 0 7px;
  @apply bg-transparent text-muted-foreground shadow-none hover:text-primary;
  font-size: var(--font-content-secondary);
  font-weight: 500;
  white-space: nowrap;
}

.group-select-action[data-applied='true'] {
  @apply bg-transparent text-primary hover:text-primary;
}

.group-select-action:hover {
  background: var(--surface-primary-subtle);
}

.group-select-action :deep(svg) {
  flex: none;
}

.member-row {
  display: grid;
  height: 40px;
  align-items: center;
  gap: 10px;
  padding-block: 1px;
}

.member-primary {
  --member-actions-space: 64px;
  position: relative;
  display: flex;
  min-width: 0;
  align-items: center;
}

.member-path {
  min-width: 0;
  flex: 1;
  overflow: hidden;
  @apply text-card-foreground;
  font-size: var(--font-content-body);
  font-weight: 400;
}

.member-actions {
  position: absolute;
  right: 0;
  display: flex;
  gap: 2px;
  opacity: 0;
  pointer-events: none;
  transition: opacity 0.14s ease;
}
.protected-entry-label {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  color: var(--muted-foreground);
  font-size: var(--font-content-meta);
  white-space: nowrap;
  transition: opacity 0.14s ease;
}

.member-row:is(:hover, :has(:focus-visible)) .member-actions {
  opacity: 1;
  pointer-events: auto;
}

.member-row:is(:hover, :has(:focus-visible)) .protected-entry-label {
  opacity: 0;
}

.member-primary:has(.md-ai-action) {
  --member-actions-space: 96px;
}

.member-row:is(:hover, :has(:focus-visible)) .member-path {
  padding-right: var(--member-actions-space);
}

.member-date {
  overflow: hidden;
  @apply text-muted-foreground;
  font-size: 10px;
  font-variant-numeric: tabular-nums;
  text-overflow: ellipsis;
  white-space: nowrap;
}

@container duplicates (max-width: 700px) {
  .member-row {
    grid-template-columns: 18px minmax(0, 1fr);
  }

  .member-date {
    display: none;
  }
}

@container duplicates (max-width: 560px) {
  .group-header {
    grid-template-columns: 32px minmax(0, 1fr) 30px 20px;
    gap: 8px;
  }

  .group-select-action {
    width: 30px;
    padding: 0;
  }

  .group-select-label {
    display: none;
  }
}
</style>

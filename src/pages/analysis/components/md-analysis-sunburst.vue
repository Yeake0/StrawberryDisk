<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onDeactivated, onMounted, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';

import MdFileEntryContextMenu from '@/components/custom/md-file-entry-context-menu.vue';
import MdIconSunburst from '@/components/icons/md-icon-sunburst.vue';
import type { AnalysisRemainderSelection, AnalysisResult, DirectoryEntryInfo } from '@/lib/models/analysis';
import { ByteSizeService } from '@/lib/services/byte-size-service';
import * as FormatUtils from '@/lib/utils/format';
import * as PathUtils from '@/lib/utils/path';
import * as SunburstLayoutUtils from '@/lib/utils/sunburst-layout';
import type { SunburstSector } from '@/lib/utils/sunburst-layout';

const { t } = useI18n({ useScope: 'global' });
const props = defineProps<{
  result: AnalysisResult;
  depth?: number;
  openDisabled: boolean;
  deleteDisabled: boolean;
  deletingPath?: string | null;
  hoveredEntryPath?: string | null;
}>();
const emit = defineEmits<{
  navigate: [path: string];
  openEntry: [entry: DirectoryEntryInfo];
  reveal: [path: string];
  delete: [entry: DirectoryEntryInfo];
  hoverEntry: [path: string | null];
  showRemainder: [selection: AnalysisRemainderSelection];
}>();
const contextSector = ref<SunburstSector | null>(null);
const contextMenuOpen = ref(false);
const contextEntry = computed<DirectoryEntryInfo | null>(() => {
  const sector = contextSector.value;
  if (!sector?.path) return null;
  // Hierarchy projections carry read-only metadata. They never grant the
  // deletion authority reserved for complete direct-child scan snapshots.
  return (
    sector.entry ?? {
      name: sector.name,
      path: sector.path,
      bytes: sector.bytes,
      fileCount: sector.fileCount,
      isDirectory: true,
      modifiedAtMs: null,
      contentFingerprint: null,
    }
  );
});
function handleContextMenu(sector: SunburstSector, event: MouseEvent) {
  leave();
  if (props.openDisabled || !sector.path) {
    event.preventDefault();
    return;
  }
  contextSector.value = sector;
}
function setContextMenuOpen(open: boolean) {
  contextMenuOpen.value = open;
  if (open) leave();
}
function resetInteraction() {
  leave();
  contextSector.value = null;
  contextMenuOpen.value = false;
}
const depth = computed(() => props.depth ?? 3);
const centerWidth = ref(120);
const chartElement = ref<HTMLElement | null>(null);
const tooltipElement = ref<HTMLElement | null>(null);
const hoveredSector = ref<SunburstSector | null>(null);
const pointerTooltip = ref(false);
const tooltipPosition = ref({ left: 0, top: 0, visible: false });
const sectors = computed(() => SunburstLayoutUtils.layout(props.result, depth.value));
const summary = computed(
  () =>
    hoveredSector.value ?? {
      name: PathUtils.fileName(props.result.root),
      bytes: props.result.totalBytes,
    }
);
const remainderLabel = computed(() => t('analysis.sunburstRemainder'));
let pointer = { x: 0, y: 0 };
let resizeObserver: ResizeObserver | null = null;
onMounted(() => {
  if (!chartElement.value) return;
  resizeObserver = new ResizeObserver(([entry]) => {
    if (!entry) return;
    centerWidth.value = Math.min(entry.contentRect.width, entry.contentRect.height) * 0.24;
    leave();
  });
  resizeObserver.observe(chartElement.value);
});
onBeforeUnmount(() => {
  resizeObserver?.disconnect();
  leave();
});

function percentage(bytes: number, total = props.result.totalBytes) {
  const value = FormatUtils.percent(bytes, total);
  return value > 0 && value < 1 ? '<1%' : `${Math.round(value)}%`;
}

function positionTooltip() {
  const element = tooltipElement.value;
  const chart = chartElement.value;
  if (!element || !chart) return;
  const bounds = chart.getBoundingClientRect();
  const leftEdge = Math.max(8, bounds.left + 8);
  const rightEdge = Math.min(window.innerWidth - 8, bounds.right - 8);
  const topEdge = Math.max(8, bounds.top + 8);
  const bottomEdge = Math.min(window.innerHeight - 8, bounds.bottom - 8);
  const left =
    pointer.x + 12 + element.offsetWidth <= rightEdge ? pointer.x + 12 : pointer.x - 12 - element.offsetWidth;
  const top =
    pointer.y + 12 + element.offsetHeight <= bottomEdge ? pointer.y + 12 : pointer.y - 12 - element.offsetHeight;
  tooltipPosition.value = {
    left: Math.max(leftEdge, Math.min(left, rightEdge - element.offsetWidth)),
    top: Math.max(topEdge, Math.min(top, bottomEdge - element.offsetHeight)),
    visible: true,
  };
}
function hover(sector: SunburstSector, event?: PointerEvent) {
  if (props.openDisabled || contextMenuOpen.value) return;
  const changed = sector.key !== hoveredSector.value?.key || !pointerTooltip.value;
  hoveredSector.value = sector;
  // Deeper sectors link to the direct child shown in the adjacent rank pane.
  emit('hoverEntry', sector.branchPath);
  pointerTooltip.value = !!event;
  if (event) {
    pointer = { x: event.clientX, y: event.clientY };
    if (changed) void nextTick(positionTooltip);
    else positionTooltip();
  }
}
function leave() {
  hoveredSector.value = null;
  pointerTooltip.value = false;
  tooltipPosition.value.visible = false;
  emit('hoverEntry', null);
}
function activate(sector: SunburstSector) {
  if (props.openDisabled) return;
  if (sector.remainder) {
    leave();
    emit('showRemainder', sector.remainder);
    return;
  }
  if (!sector.path || (sector.entry && !sector.entry.isDirectory)) return;
  leave();
  emit('navigate', sector.path);
}
function openEntry(sector: SunburstSector) {
  if (props.openDisabled) return;
  if (sector.entry && !sector.entry.isDirectory) emit('openEntry', sector.entry);
  else activate(sector);
}
watch([() => props.result, () => props.openDisabled, depth], resetInteraction);
onDeactivated(resetInteraction);
</script>

<template>
  <div class="sunburst-workspace">
    <div ref="chartElement" class="sunburst-canvas" @pointerleave="leave">
      <MdFileEntryContextMenu
        :entry-key="`${result.scanId}:${contextSector?.key ?? ''}`"
        :enabled="!openDisabled"
        :open-disabled="openDisabled || !contextEntry"
        :delete-disabled="deleteDisabled || !contextSector?.entry || contextSector.depth > 1"
        :reveal-disabled="!contextEntry || deletingPath === contextEntry.path"
        @menu-state-change="setContextMenuOpen"
        @open="contextEntry && emit('openEntry', contextEntry)"
        @reveal="contextEntry && emit('reveal', contextEntry.path)"
        @delete="contextSector?.entry && contextSector.depth === 1 && emit('delete', contextSector.entry)"
      >
        <MdIconSunburst
          :sectors="sectors"
          :highlighted-key="hoveredSector?.key ?? null"
          :highlighted-branch="hoveredSector ? null : (hoveredEntryPath ?? null)"
          :disabled="openDisabled"
          :remainder-label="t('analysis.other')"
          :chart-label="t('analysis.sunburst')"
          @hover="hover"
          @focus="hover($event)"
          @leave="leave"
          @activate="activate"
          @open-entry="openEntry"
          @sector-context-menu="handleContextMenu"
        />
      </MdFileEntryContextMenu>
      <div class="sunburst-center" :style="{ width: `${centerWidth}px` }" aria-hidden="true">
        <span>{{ summary.name || t('analysis.other') }}</span>
        <strong>{{ ByteSizeService.bytes(summary.bytes) }}</strong>
        <small v-if="hoveredSector">{{ percentage(summary.bytes) }}</small>
      </div>
    </div>

    <Teleport to="body">
      <div
        v-if="hoveredSector && pointerTooltip && !openDisabled"
        ref="tooltipElement"
        class="sunburst-tooltip"
        :style="{
          left: `${tooltipPosition.left}px`,
          top: `${tooltipPosition.top}px`,
          visibility: tooltipPosition.visible ? 'visible' : 'hidden',
        }"
        aria-hidden="true"
      >
        <strong>{{ hoveredSector.name || remainderLabel }}</strong>
        <small
          >{{ ByteSizeService.bytes(hoveredSector.bytes)
          }}<template v-if="hoveredSector.path">
            ·
            {{
              t('common.fileCount', { count: FormatUtils.integer(hoveredSector.fileCount) }, hoveredSector.fileCount)
            }}</template
          ></small
        >
        <small class="tooltip-share">
          <span>{{ t('analysis.shareOfCurrentFolder') }}</span>
          <span>{{ percentage(hoveredSector.bytes) }}</span>
        </small>
        <small v-if="hoveredSector.depth > 1" class="tooltip-share">
          <span>{{ t('analysis.shareOfParentFolder') }}</span>
          <span>{{ percentage(hoveredSector.bytes, hoveredSector.parentBytes) }}</span>
        </small>
        <span class="tooltip-path">{{ hoveredSector.path ?? hoveredSector.parentPath }}</span>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
@reference "@assets/main.css";
.sunburst-workspace {
  display: flex;
  min-height: 0;
  min-width: 0;
  flex: 1;
  flex-direction: column;
  margin: 0 12px 12px;
}
.sunburst-canvas {
  position: relative;
  flex: 1;
  min-height: 0;
  min-width: 0;
  overflow: hidden;
}
.sunburst-center {
  position: absolute;
  top: 50%;
  left: 50%;
  display: flex;
  width: 24%;
  max-width: 150px;
  transform: translate(-50%, -50%);
  flex-direction: column;
  gap: 5px;
  align-items: center;
  text-align: center;
  pointer-events: none;
}
.sunburst-center span {
  display: -webkit-box;
  overflow: hidden;
  -webkit-box-orient: vertical;
  -webkit-line-clamp: 2;
  overflow-wrap: anywhere;
  font-size: var(--font-content-secondary);
}
.sunburst-center strong {
  font-size: var(--font-content-section-title);
  font-variant-numeric: tabular-nums;
}
.sunburst-center small {
  @apply text-muted-foreground;
  font-size: var(--font-content-meta);
}
.sunburst-tooltip {
  position: fixed;
  z-index: 80;
  display: flex;
  max-width: min(300px, calc(100vw - 24px));
  flex-direction: column;
  gap: 4px;
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 10px;
  @apply bg-popover text-popover-foreground;
  box-shadow: 0 4px 16px var(--shadow-subtle);
  pointer-events: none;
  overflow-wrap: anywhere;
}
.sunburst-tooltip strong {
  font-size: var(--font-content-primary);
}
.sunburst-tooltip small,
.sunburst-tooltip span {
  @apply text-muted-foreground;
  font-size: var(--font-content-secondary);
}
.sunburst-tooltip .tooltip-path {
  font-size: var(--font-content-meta);
}
.tooltip-share {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  font-variant-numeric: tabular-nums;
}
.tooltip-share > :last-child {
  flex: none;
  white-space: nowrap;
}
</style>

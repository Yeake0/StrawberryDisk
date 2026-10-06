<script setup lang="ts">
import { useI18n } from 'vue-i18n';
import { computed, nextTick, onBeforeUnmount, onDeactivated, onMounted, ref, watch } from 'vue';

import MdFileEntryContextMenu from '@/components/custom/md-file-entry-context-menu.vue';
import MdAnalysisEntryIcon from './md-analysis-entry-icon.vue';
import MdNativeFileIcon from '@/components/custom/md-native-file-icon.vue';
import MdIcon from '@/components/icons/md-icon.vue';
import { ICON_NAMES } from '@/lib/models/ui';
import { TREEMAP_TILE_KINDS } from '@/lib/models/analysis';
import type { AnalysisRemainderSelection, AnalysisResult, DirectoryEntryInfo } from '@/lib/models/analysis';
import { ByteSizeService } from '@/lib/services/byte-size-service';
import * as FormatUtils from '@/lib/utils/format';
import * as TreemapLayoutUtils from '@/lib/utils/hierarchical-treemap-layout';
import type { HierarchicalTreemapTile } from '@/lib/utils/hierarchical-treemap-layout';

const { t, locale } = useI18n({ useScope: 'global' });

const props = defineProps<{
  result: AnalysisResult;
  depth?: number;
  openDisabled: boolean;
  deleteDisabled: boolean;
  deletingPath?: string | null;
  hoveredEntryPath?: string | null;
}>();

const emit = defineEmits<{
  activate: [entry: DirectoryEntryInfo];
  navigate: [path: string];
  openEntry: [entry: DirectoryEntryInfo];
  reveal: [path: string];
  delete: [entry: DirectoryEntryInfo];
  hoverEntry: [path: string | null];
  showRemainder: [selection: AnalysisRemainderSelection];
}>();

const treemapElement = ref<HTMLElement | null>(null);
const viewport = ref({ width: 0, height: 0 });
const viewportOrigin = ref({ left: 0, top: 0 });
const pixelRatio = ref(1);
let resizeObserver: ResizeObserver | null = null;
let pixelRatioQuery: MediaQueryList | null = null;
const depth = computed(() => props.depth ?? 1);
const tiles = computed(() => TreemapLayoutUtils.layout(props.result, depth.value, viewport.value));
const rootTiles = computed(() => tiles.value.filter(tile => tile.depth === 1));

function activate(tile: HierarchicalTreemapTile) {
  if (props.openDisabled || !tile.entry) return;
  leaveTile();
  if (tile.depth > 1 && tile.entry.isDirectory) emit('navigate', tile.entry.path);
  else emit('activate', tile.entry);
}
function showRemainder(tile: HierarchicalTreemapTile) {
  if (props.openDisabled || !tile.remainder) return;
  leaveTile();
  emit('showRemainder', tile.remainder);
}
function remainderName(tile: HierarchicalTreemapTile) {
  const count = tile.kind === TREEMAP_TILE_KINDS.remainder ? tile.entryCount : null;
  if (count === null) return t('analysis.other');
  return t('analysis.treemapRemainder', { count: FormatUtils.integer(count) }, count);
}
const hoveredTile = ref<HierarchicalTreemapTile | null>(null);
const highlightedTile = computed(() => {
  if (props.openDisabled) return null;
  if (contextMenuOpen.value && contextTile.value) return contextTile.value;
  if (hoveredTile.value) return hoveredTile.value;
  return props.hoveredEntryPath
    ? (tiles.value.find(tile => tile.entry?.path === props.hoveredEntryPath) ?? null)
    : null;
});
const contextMenuOpen = ref(false);
const contextTile = ref<HierarchicalTreemapTile | null>(null);
const tooltipElement = ref<HTMLElement | null>(null);
let pointerPosition = { x: 0, y: 0 };
const tooltipPosition = ref({
  left: 0,
  top: 0,
  visible: false,
});

function pixelCoordinate(value: number, limit: number, origin: number) {
  // Snap shared edges in screen coordinates, keeping viewport edges inside its clip.
  if (value <= 0) return 0;
  if (value >= limit) return limit;
  return Math.max(
    0,
    Math.min(limit, Math.round((origin + value) * pixelRatio.value + 1e-7) / pixelRatio.value - origin)
  );
}

function tileStyle(tile: HierarchicalTreemapTile, fullBox = false) {
  /*
   * Color only separates adjacent regions; it does not encode file type or
   * risk. Cycling theme chart colors in stable layout order keeps every skin
   * legible without deriving style data from private paths. Aggregate tiles
   * stay neutral so they cannot be mistaken for real directories.
   */
  const paletteColor = tile.kind === TREEMAP_TILE_KINDS.entry ? `var(--chart-${tile.colorIndex + 1})` : 'var(--muted)';
  const header = !fullBox && tile.headerHeight > 0;
  const { width, height } = viewport.value;
  const origin = viewportOrigin.value;
  const left = pixelCoordinate((tile.left / 100) * width, width, origin.left);
  const right = pixelCoordinate(((tile.left + tile.width) / 100) * width, width, origin.left);
  const top = pixelCoordinate((tile.top / 100) * height, height, origin.top);
  const bottom = pixelCoordinate(
    (tile.top / 100) * height + (header ? tile.headerHeight : (tile.height / 100) * height),
    height,
    origin.top
  );
  return {
    left: `${left}px`,
    top: `${top}px`,
    width: `${Math.max(0, right - left)}px`,
    height: `${Math.max(0, bottom - top)}px`,
    visibility: right > left && bottom > top ? undefined : ('hidden' as const),
    '--treemap-tile-color': paletteColor,
    '--treemap-card-overlay-opacity':
      tile.kind === TREEMAP_TILE_KINDS.entry && tile.depth > 1 ? 0.8 + Math.min(tile.depth - 2, 4) * 0.025 : undefined,
  };
}

function tileDimensions(tile: HierarchicalTreemapTile) {
  return {
    width: (tile.width / 100) * viewport.value.width,
    height: (tile.height / 100) * viewport.value.height,
  };
}

function highlightStyle(tile: HierarchicalTreemapTile) {
  const edgeX = 100 / viewport.value.width + 0.001;
  const edgeY = 100 / viewport.value.height + 0.001;
  const left = tile.left <= edgeX;
  const top = tile.top <= edgeY;
  const right = tile.left + tile.width >= 100 - edgeX;
  const bottom = tile.top + tile.height >= 100 - edgeY;
  const radius = (corner: boolean) => (corner ? 'var(--radius)' : '0');
  return {
    ...tileStyle(tile, true),
    borderRadius: `${radius(left && top)} ${radius(right && top)} ${radius(right && bottom)} ${radius(left && bottom)}`,
  };
}

function tileClass(tile: HierarchicalTreemapTile) {
  const { width, height } = tileDimensions(tile);
  const tiny = width < 72 || height < 52;
  return {
    compact: width < 160 || height < 88,
    tiny,
    unlabeled: width < 40 || height < 28,
    'metrics-hidden': tile.headerHeight === 0 && (width < 48 || height < (tiny ? 42 : 56)),
    'percentage-hidden': width < 72 || height < 56 || (width < 130 && height < 80),
    remainder: tile.kind === TREEMAP_TILE_KINDS.remainder,
    'parent-tile': tile.headerHeight > 0,
    'root-tile': tile.depth === 1,
    'is-highlighted': tile.kind === TREEMAP_TILE_KINDS.entry && highlightedTile.value?.key === tile.key,
  };
}

function tilePercentage(tile: HierarchicalTreemapTile) {
  return Math.round(FormatUtils.percent(tile.bytes, props.result.totalBytes));
}

function tooltipPercentage(bytes: number, total: number) {
  const value = FormatUtils.percent(bytes, total);
  return value > 0 && value < 1 ? '<1%' : `${Math.round(value)}%`;
}

function shouldShowTileIcon(tile: HierarchicalTreemapTile) {
  const { width, height } = tileDimensions(tile);
  return tile.headerHeight === 0 && tile.depth === 1 && width >= 180 && height >= 100;
}

function measureViewport(size?: { width: number; height: number }) {
  const element = treemapElement.value;
  if (!element) return;
  const bounds = element.getBoundingClientRect();
  const width = size?.width ?? (bounds.width || element.clientWidth);
  const height = size?.height ?? (bounds.height || element.clientHeight);
  if (width <= 0 || height <= 0) return;
  const sizeChanged = width !== viewport.value.width || height !== viewport.value.height;
  const originChanged = bounds.left !== viewportOrigin.value.left || bounds.top !== viewportOrigin.value.top;
  // KeepAlive activation and window/observer notifications may report the same
  // geometry. Preserve the layout and hover instead of repartitioning every tile.
  if (!sizeChanged && !originChanged) return;
  leaveTile();
  if (sizeChanged) viewport.value = { width, height };
  if (originChanged) viewportOrigin.value = { left: bounds.left, top: bounds.top };
}

function handleViewportResize() {
  measureViewport();
}

function watchPixelRatio() {
  pixelRatioQuery?.removeEventListener('change', watchPixelRatio);
  pixelRatio.value = window.devicePixelRatio || 1;
  pixelRatioQuery = window.matchMedia(`(resolution: ${pixelRatio.value}dppx)`);
  pixelRatioQuery.addEventListener('change', watchPixelRatio);
  measureViewport();
}

// The context-menu trigger can replace its slotted canvas when scanning disables
// interaction. Follow the live element rather than retaining the initial node.
watch(
  treemapElement,
  element => {
    resizeObserver?.disconnect();
    resizeObserver = null;
    if (!element) return;
    measureViewport();
    resizeObserver = new ResizeObserver(entries => {
      const entry = entries.find(item => item.target === treemapElement.value);
      if (entry) measureViewport(entry.contentRect);
    });
    resizeObserver.observe(element);
  },
  { flush: 'post' }
);

onMounted(() => {
  watchPixelRatio();
  window.addEventListener('resize', handleViewportResize);
});

onBeforeUnmount(() => {
  resizeObserver?.disconnect();
  pixelRatioQuery?.removeEventListener('change', watchPixelRatio);
  window.removeEventListener('resize', handleViewportResize);
  leaveTile();
});

function updateTooltipPosition(event: PointerEvent) {
  pointerPosition = { x: event.clientX, y: event.clientY };
  positionTooltip();
}

function positionTooltip() {
  const tooltip = tooltipElement.value;
  const treemap = treemapElement.value;
  if (!tooltip || !treemap || !hoveredTile.value) return;
  const bounds = treemap.getBoundingClientRect();
  const inset = 8;
  const offset = 12;
  const leftEdge = Math.max(inset, bounds.left + inset);
  const rightEdge = Math.min(window.innerWidth - inset, bounds.right - inset);
  const topEdge = Math.max(inset, bounds.top + inset);
  const bottomEdge = Math.min(window.innerHeight - inset, bounds.bottom - inset);
  const width = tooltip.offsetWidth;
  const height = tooltip.offsetHeight;
  // Keep details over the visualization rather than covering the adjacent
  // file list. Measure after mounting to account for wrapped localized names.
  const left =
    pointerPosition.x + offset + width <= rightEdge ? pointerPosition.x + offset : pointerPosition.x - offset - width;
  const top =
    pointerPosition.y + offset + height <= bottomEdge
      ? pointerPosition.y + offset
      : pointerPosition.y - offset - height;
  tooltipPosition.value = {
    left: Math.max(leftEdge, Math.min(left, rightEdge - width)),
    top: Math.max(topEdge, Math.min(top, bottomEdge - height)),
    visible: true,
  };
}

function showTooltip(tile: HierarchicalTreemapTile, event: PointerEvent) {
  hideTooltip();
  if (contextMenuOpen.value || props.openDisabled) return;
  emit('hoverEntry', tile.branchPath);
  updateTooltipPosition(event);
  hoveredTile.value = tile;
  void nextTick(positionTooltip);
}

function resetInteraction() {
  leaveTile();
  contextTile.value = null;
  contextMenuOpen.value = false;
}
onDeactivated(resetInteraction);

function hideTooltip() {
  hoveredTile.value = null;
  tooltipPosition.value.visible = false;
}

function leaveTile() {
  hideTooltip();
  emit('hoverEntry', null);
}

watch([() => props.result, () => props.openDisabled, depth], resetInteraction);

function handleContextMenu(tile: HierarchicalTreemapTile, event: MouseEvent) {
  leaveTile();
  if (props.openDisabled || !tile.entry) {
    event.preventDefault();
    return;
  }
  contextTile.value = tile;
}

function handleCanvasContextMenu(event: MouseEvent) {
  // Empty canvas and aggregate tiles must not reuse a previous file target.
  if (!(event.target instanceof Element) || !event.target.closest('.treemap-tile:not(.remainder)')) {
    event.preventDefault();
  }
}

function setContextMenuOpen(open: boolean) {
  contextMenuOpen.value = open;
  // A context menu is the active interaction surface. Remove the cursor
  // tooltip immediately instead of relying on portal z-index ordering, which
  // would leave two overlapping surfaces visible during menu animation.
  if (open) leaveTile();
}

function tooltipStyle() {
  const position = tooltipPosition.value;
  return {
    left: `${position.left}px`,
    top: `${position.top}px`,
    maxWidth: `${Math.max(0, Math.min(300, viewport.value.width - 16))}px`,
    maxHeight: `${Math.max(0, viewport.value.height - 16)}px`,
    visibility: position.visible ? ('visible' as const) : ('hidden' as const),
  };
}
</script>

<template>
  <div class="treemap-workspace">
    <MdFileEntryContextMenu
      :entry-key="`${result.scanId}:${contextTile?.key ?? ''}`"
      :enabled="!openDisabled"
      :open-disabled="openDisabled || !contextTile?.entry"
      :delete-disabled="deleteDisabled || !contextTile?.entry || contextTile.depth > 1"
      :reveal-disabled="!contextTile?.entry || deletingPath === contextTile.entry.path"
      @menu-state-change="setContextMenuOpen"
      @open="contextTile?.entry && emit('openEntry', contextTile.entry)"
      @reveal="contextTile?.entry && emit('reveal', contextTile.entry.path)"
      @delete="contextTile?.entry && contextTile.depth === 1 && emit('delete', contextTile.entry)"
    >
      <div ref="treemapElement" class="treemap" @pointerleave="leaveTile" @contextmenu="handleCanvasContextMenu">
        <button
          v-for="tile in tiles"
          :key="tile.key"
          v-memo="[
            tile,
            viewport,
            viewportOrigin,
            pixelRatio,
            openDisabled,
            deletingPath === tile.entry?.path,
            highlightedTile?.key === tile.key,
            locale,
          ]"
          type="button"
          class="treemap-tile"
          :class="tileClass(tile)"
          :style="tileStyle(tile)"
          :aria-label="
            tile.entry
              ? `${tile.entry.name} · ${deletingPath === tile.entry.path ? t('analysis.deleting') : ByteSizeService.bytes(tile.bytes)}`
              : `${remainderName(tile)} · ${ByteSizeService.bytes(tile.bytes)} · ${t('common.open')}`
          "
          :disabled="openDisabled"
          :aria-busy="(tile.entry && deletingPath === tile.entry.path) || undefined"
          @pointerenter="showTooltip(tile, $event)"
          @pointermove="updateTooltipPosition"
          @pointerleave="leaveTile"
          @focus="tile.entry && !openDisabled && emit('hoverEntry', tile.branchPath)"
          @blur="leaveTile"
          @contextmenu="handleContextMenu(tile, $event)"
          @click="tile.entry ? activate(tile) : showRemainder(tile)"
          @dblclick="tile.entry && !tile.entry.isDirectory && emit('openEntry', tile.entry)"
          @keydown.enter="tile.entry && !tile.entry.isDirectory && emit('openEntry', tile.entry)"
        >
          <MdAnalysisEntryIcon
            v-if="tile.entry && (shouldShowTileIcon(tile) || deletingPath === tile.entry.path)"
            :entry="tile.entry"
            :deleting="deletingPath === tile.entry.path"
            compact
          />
          <span class="tile-copy">
            <strong class="md-result-primary">{{ tile.entry?.name ?? remainderName(tile) }}</strong>
            <span class="tile-metrics">
              <small>{{
                tile.entry && deletingPath === tile.entry.path
                  ? t('analysis.deleting')
                  : ByteSizeService.bytes(tile.bytes)
              }}</small>
              <em>{{ tilePercentage(tile) }}%</em>
            </span>
          </span>
        </button>
        <div
          v-for="tile in rootTiles"
          :key="`${tile.key}:outline`"
          class="treemap-root-outline"
          :style="highlightStyle(tile)"
          aria-hidden="true"
        />
        <div
          v-if="highlightedTile"
          class="treemap-hover-outline"
          :style="highlightStyle(highlightedTile)"
          aria-hidden="true"
        />
      </div>
    </MdFileEntryContextMenu>
  </div>

  <Teleport to="body">
    <div
      v-if="!openDisabled && hoveredTile && !contextMenuOpen"
      ref="tooltipElement"
      class="treemap-pointer-tooltip"
      :style="tooltipStyle()"
      aria-hidden="true"
    >
      <MdNativeFileIcon
        v-if="hoveredTile.kind === TREEMAP_TILE_KINDS.entry"
        :path="hoveredTile.entry.path"
        :name="hoveredTile.entry.name"
        :directory="hoveredTile.entry.isDirectory"
        directory-mode="generic"
        compact
      />
      <span v-else class="tooltip-remainder-icon">
        <MdIcon :name="ICON_NAMES.list" :size="18" />
      </span>

      <span class="tooltip-copy">
        <strong v-if="hoveredTile.kind === TREEMAP_TILE_KINDS.entry" class="md-result-primary">
          {{ hoveredTile.entry.name }}
        </strong>
        <strong v-else class="md-result-primary">
          {{ remainderName(hoveredTile) }}
        </strong>
        <small>
          {{ ByteSizeService.bytes(hoveredTile.bytes) }}
          <template v-if="hoveredTile.kind === TREEMAP_TILE_KINDS.entry">
            ·
            {{
              t(
                'common.fileCount',
                { count: FormatUtils.integer(hoveredTile.entry.fileCount) },
                hoveredTile.entry.fileCount
              )
            }}
          </template>
        </small>
        <small class="tooltip-share">
          <span>{{ t('analysis.shareOfCurrentFolder') }}</span>
          <span>{{ tooltipPercentage(hoveredTile.bytes, result.totalBytes) }}</span>
        </small>
        <small v-if="hoveredTile.depth > 1" class="tooltip-share">
          <span>{{ t('analysis.shareOfParentFolder') }}</span>
          <span>{{ tooltipPercentage(hoveredTile.bytes, hoveredTile.parentBytes) }}</span>
        </small>
        <span class="tooltip-path">{{ hoveredTile.entry?.path ?? hoveredTile.parentPath }}</span>
      </span>
    </div>
  </Teleport>
</template>

<style scoped>
@reference "@assets/main.css";

.treemap-workspace {
  display: flex;
  min-width: 0;
  min-height: 0;
  flex: 1;
  flex-direction: column;
  margin: 0 12px 12px;
}
.treemap {
  position: relative;
  min-height: 0;
  max-height: 100%;
  flex: 1;
  overflow: hidden;
  border-radius: var(--radius);
  @apply bg-card;
  contain: layout paint;
  isolation: isolate;
}

.treemap::after {
  /* Draw rounded corners above the rectangular tile borders clipped by the viewport. */
  position: absolute;
  z-index: 4;
  inset: 0;
  border: 1px solid var(--border);
  border-radius: inherit;
  content: '';
  pointer-events: none;
}

.treemap-root-outline,
.treemap-hover-outline {
  /* Keep the complete highlight above all tiles and the rounded viewport border. */
  position: absolute;
  z-index: 5;
  border: 1px solid var(--border);
  pointer-events: none;
}

.treemap-hover-outline {
  z-index: 6;
  border-color: var(--muted-foreground);
}

.treemap-tile {
  --treemap-card-overlay-opacity: 0.74;

  position: absolute;
  min-width: 0;
  min-height: 0;
  display: flex;
  align-items: flex-start;
  justify-content: flex-start;
  gap: 8px;
  overflow: hidden;
  border-width: 0;
  border-radius: 0;
  padding: 12px;
  background: var(--treemap-tile-color, var(--secondary));
  @apply text-card-foreground transition-[color,background-color,border-color] duration-150;
  font: inherit;
  text-align: left;
  cursor: pointer;
}

.treemap-tile::before {
  position: absolute;
  inset: 0;
  background: var(--card);
  content: '';
  opacity: var(--treemap-hover-overlay-opacity, var(--treemap-card-overlay-opacity));
  pointer-events: none;
  transition: opacity 0.15s ease;
}

.treemap-tile::after {
  /* Paint each shared edge once; the viewport owns the outer perimeter. */
  position: absolute;
  z-index: 2;
  inset: 0;
  border: 1px solid var(--border);
  border-width: 1px 0 0 1px;
  border-radius: inherit;
  content: '';
  pointer-events: none;
  transition: border-color 0.15s ease;
}

.treemap-tile > * {
  /*
   * Keep labels and the deletion indicator out of pointer hit testing so
   * the whole region remains one consistent tooltip and activation target.
   */
  pointer-events: none;
  z-index: 1;
}

.treemap-tile:is(:hover, [data-state='open'], .is-highlighted) {
  /* Match the sunburst's highlighted color strength regardless of depth. */
  --treemap-hover-overlay-opacity: 0.4;

  z-index: 2;
}

.treemap-tile:is(:hover, [data-state='open'], .is-highlighted)::after {
  border-color: var(--muted-foreground);
}

.treemap-tile.remainder:is(:hover, [data-state='open'], .is-highlighted) {
  --treemap-hover-overlay-opacity: 0.15;
}

.treemap-tile:focus-visible {
  z-index: 2;
  outline: 2px solid var(--focus-ring-subtle);
  outline-offset: -2px;
}

.treemap-tile.remainder {
  --treemap-card-overlay-opacity: 0.3;

  background: var(--muted);
  @apply text-muted-foreground;
  cursor: pointer;
}

.tile-copy {
  display: flex;
  min-width: 0;
  flex: 1;
  flex-direction: column;
  gap: 5px;
}

.tile-copy strong {
  display: block;
  overflow: hidden;
  @apply text-card-foreground;
  font-size: var(--font-content-primary);
  line-height: 1.4;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.tile-metrics {
  display: flex;
  min-width: 0;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 2px 8px;
  @apply text-muted-foreground;
  font-size: var(--font-content-secondary);
  font-variant-numeric: tabular-nums;
}

.tile-metrics small,
.tile-metrics em {
  font-size: inherit;
  font-style: normal;
  white-space: nowrap;
}

.tile-metrics em {
  flex: none;
  font-size: var(--font-content-meta);
}

.tile-metrics small {
  min-width: 0;
  max-width: 100%;
  overflow: hidden;
  @apply text-card-foreground;
  font-weight: 500;
  text-overflow: ellipsis;
}

.treemap-tile.remainder .tile-metrics small {
  @apply text-muted-foreground;
}

.treemap-tile.compact {
  --treemap-card-overlay-opacity: 0.8;

  gap: 7px;
  padding: 8px;
}

.treemap-tile.compact .tile-copy {
  gap: 3px;
}

.treemap-tile.percentage-hidden .tile-metrics em {
  display: none;
}

.treemap-tile.tiny .tile-copy strong {
  font-size: var(--font-content-meta);
}

.treemap-tile.metrics-hidden .tile-metrics,
.treemap-tile.unlabeled .tile-copy {
  display: none;
}

.treemap-tile.tiny .tile-metrics {
  font-size: var(--font-content-meta);
  line-height: 1.4;
}

.treemap-tile.tiny {
  --treemap-card-overlay-opacity: 0.84;

  padding: 5px;
}

.treemap-tile.unlabeled {
  /* Padding can force a subpixel box beyond its assigned parent rectangle. */
  gap: 0;
  padding: 0;
}

.treemap-tile.remainder:is(.compact, .tiny) {
  --treemap-card-overlay-opacity: 0.3;
}

.treemap-pointer-tooltip {
  position: fixed;
  z-index: 80;
  display: flex;
  width: max-content;
  max-width: min(300px, calc(100vw - 24px));
  align-items: flex-start;
  gap: 8px;
  overflow: hidden;
  border-width: 1px;
  border-radius: 8px;
  padding: 10px;
  @apply border-border bg-popover text-popover-foreground;
  box-shadow: 0 4px 16px var(--shadow-subtle);
  pointer-events: none;
  will-change: left, top;
}

.tooltip-remainder-icon {
  display: grid;
  width: 28px;
  height: 28px;
  flex: none;
  place-items: center;
  border-radius: 6px;
  @apply bg-muted text-muted-foreground;
}

.tooltip-copy {
  display: flex;
  min-width: 0;
  flex-direction: column;
  gap: 3px;
}

.tooltip-copy strong {
  overflow-wrap: anywhere;
  font-size: var(--font-content-primary);
  line-height: 1.25;
}

.tooltip-copy .tooltip-path {
  overflow-wrap: anywhere;
  @apply text-muted-foreground;
  font-size: var(--font-content-meta);
}

.tooltip-copy small {
  @apply text-muted-foreground;
  font-size: var(--font-content-secondary);
  line-height: 1.3;
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

.treemap-tile.parent-tile {
  align-items: center;
  padding: 0 6px;
}
.parent-tile .tile-copy {
  flex-direction: row;
  align-items: center;
  gap: 6px;
}
.parent-tile .tile-copy strong {
  min-width: 0;
  flex: 1;
  font-size: var(--font-content-meta);
  line-height: 14px;
}
.parent-tile.root-tile .tile-copy strong {
  font-size: var(--font-content-secondary);
  font-weight: 600;
  line-height: 16px;
}
.parent-tile .tile-metrics {
  flex: none;
  flex-wrap: nowrap;
  gap: 4px;
  font-size: var(--font-content-meta);
}
.parent-tile.compact .tile-metrics em {
  display: none;
}
</style>

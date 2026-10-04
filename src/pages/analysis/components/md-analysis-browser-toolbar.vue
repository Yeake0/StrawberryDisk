<script setup lang="ts">
import MdTooltip from '@/components/custom/md-tooltip.vue';
import { useI18n } from 'vue-i18n';
import { onDeactivated, onMounted, ref, watch } from 'vue';
import {
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuPortal,
  DropdownMenuRoot,
  DropdownMenuTrigger,
} from 'reka-ui';

import MdIconAction from '@/components/custom/md-icon-action.vue';
import MdIcon from '@/components/icons/md-icon.vue';
import { ICON_NAMES } from '@/lib/models/ui';
import type { AnalysisBreadcrumb, AnalysisSiblingFolder } from '@/lib/utils/analysis-breadcrumb';
import { ByteSizeService } from '@/lib/services/byte-size-service';
import * as FormatUtils from '@/lib/utils/format';
import * as PathUtils from '@/lib/utils/path';

const { t } = useI18n({ useScope: 'global' });

const props = defineProps<{
  breadcrumbs: AnalysisBreadcrumb[];
  busy: boolean;
  preserveBusyAppearance: boolean;
  canGoBack: boolean;
  canGoForward: boolean;
  homeDisabled: boolean;
  listCollapsed: boolean;
}>();

const emit = defineEmits<{
  back: [];
  forward: [];
  home: [];
  toggleList: [];
  navigate: [path: string];
}>();

const breadcrumbsElement = ref<HTMLElement | null>(null);
const openMenuPath = ref<string | null>(null);

function hasSiblingMenu(segment: AnalysisBreadcrumb) {
  return (segment.siblings?.length ?? 0) > 1;
}

function folderShare(folder: AnalysisSiblingFolder, segment: AnalysisBreadcrumb) {
  return FormatUtils.percent(folder.bytes, segment.siblingParentBytes ?? 0);
}

function folderPercentage(folder: AnalysisSiblingFolder, segment: AnalysisBreadcrumb) {
  const share = folderShare(folder, segment);
  return share > 0 && share < 1 ? '<1%' : `${Math.round(share)}%`;
}

function isCurrentFolder(path: string, segment: AnalysisBreadcrumb) {
  return PathUtils.comparisonKey(path) === PathUtils.comparisonKey(segment.path);
}

function navigateSibling(path: string) {
  openMenuPath.value = null;
  if (!props.busy) emit('navigate', path);
}

watch([() => props.breadcrumbs, () => props.busy], () => {
  openMenuPath.value = null;
});
onDeactivated(() => {
  openMenuPath.value = null;
});

function scrollBreadcrumbsToEnd() {
  const element = breadcrumbsElement.value;
  if (!element) return;
  element.scrollLeft = element.scrollWidth;
}

watch(
  () => props.breadcrumbs,
  // A post-flush watcher already runs after Vue updates the breadcrumb DOM.
  // Scrolling here avoids painting the old position for one frame before a
  // second `nextTick` moves the current folder into view.
  scrollBreadcrumbsToEnd,
  { flush: 'post' }
);

onMounted(scrollBreadcrumbsToEnd);
</script>

<template>
  <div class="browser-toolbar md-workspace-toolbar">
    <div class="history-actions">
      <MdIconAction
        appearance="unstyled"
        class="history-action"
        :label="t('analysis.back')"
        :disabled="busy || !canGoBack"
        :data-busy-disabled="preserveBusyAppearance && busy && canGoBack"
        @click="emit('back')"
      >
        <MdIcon :name="ICON_NAMES.chevronLeft" :size="15" />
      </MdIconAction>
      <MdIconAction
        appearance="unstyled"
        class="history-action"
        :label="t('analysis.forward')"
        :disabled="busy || !canGoForward"
        :data-busy-disabled="preserveBusyAppearance && busy && canGoForward"
        @click="emit('forward')"
      >
        <MdIcon :name="ICON_NAMES.chevronRight" :size="15" />
      </MdIconAction>
      <MdIconAction
        appearance="unstyled"
        class="history-action history-action-home"
        :label="t('analysis.home')"
        :disabled="busy || homeDisabled"
        :data-busy-disabled="preserveBusyAppearance && busy && !homeDisabled"
        @click="emit('home')"
      >
        <MdIcon :name="ICON_NAMES.home" :size="15" />
      </MdIconAction>
    </div>
    <nav ref="breadcrumbsElement" class="breadcrumbs scrollbar-hidden" :aria-label="t('analysis.pathLabel')">
      <template v-for="(segment, index) in breadcrumbs" :key="`${segment.path}-${index}`">
        <span class="breadcrumb-segment">
          <MdTooltip v-if="index < breadcrumbs.length - 1 || !hasSiblingMenu(segment)" :text="segment.path"
            ><button
              type="button"
              :disabled="busy || !segment.path || index === breadcrumbs.length - 1"
              :aria-current="index === breadcrumbs.length - 1 ? 'page' : undefined"
              @click="emit('navigate', segment.path)"
            >
              {{ segment.label }}
            </button></MdTooltip
          >
          <DropdownMenuRoot
            v-if="hasSiblingMenu(segment)"
            :open="openMenuPath === segment.path"
            @update:open="openMenuPath = $event ? segment.path : null"
          >
            <DropdownMenuTrigger as-child>
              <button
                type="button"
                :class="index === breadcrumbs.length - 1 ? 'current-folder-trigger' : 'sibling-trigger'"
                :disabled="busy"
                :aria-label="t('analysis.switchSiblingFolder', { name: segment.label })"
                :aria-current="index === breadcrumbs.length - 1 ? 'page' : undefined"
              >
                <MdTooltip
                  v-if="index === breadcrumbs.length - 1"
                  :text="openMenuPath === segment.path || busy ? null : segment.path"
                  :delay-duration="0"
                  ><span class="current-folder-label">{{ segment.label }}</span></MdTooltip
                >
                <MdIcon :name="ICON_NAMES.chevronDown" :size="11" />
              </button>
            </DropdownMenuTrigger>
            <DropdownMenuPortal>
              <DropdownMenuContent align="start" :side-offset="6" class="md-analysis-sibling-menu">
                <DropdownMenuItem
                  v-for="folder in segment.siblings"
                  :key="folder.path"
                  class="sibling-menu-item"
                  :class="{ 'current-folder': isCurrentFolder(folder.path, segment) }"
                  :style="{ '--sibling-share': `${folderShare(folder, segment)}%` }"
                  :disabled="busy || isCurrentFolder(folder.path, segment)"
                  :aria-label="`${folder.path} · ${ByteSizeService.bytes(folder.bytes)}`"
                  @select="navigateSibling(folder.path)"
                >
                  <MdIcon
                    :name="ICON_NAMES.check"
                    :size="13"
                    :class="{ 'sibling-check-hidden': !isCurrentFolder(folder.path, segment) }"
                  />
                  <MdTooltip :text="folder.path" :delay-duration="0"
                    ><span class="sibling-name">{{ folder.name }}</span></MdTooltip
                  >
                  <span class="sibling-metrics">
                    <small>{{ ByteSizeService.bytes(folder.bytes) }}</small>
                    <small v-if="segment.siblingParentBytes !== undefined" class="sibling-percentage">{{
                      folderPercentage(folder, segment)
                    }}</small>
                  </span>
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenuPortal>
          </DropdownMenuRoot>
        </span>
        <MdIcon v-if="index < breadcrumbs.length - 1" :name="ICON_NAMES.chevronRight" :size="12" />
      </template>
    </nav>
    <MdIconAction
      appearance="unstyled"
      class="history-action list-toggle"
      :label="t(listCollapsed ? 'analysis.expandFileList' : 'analysis.collapseFileList')"
      :aria-expanded="!listCollapsed"
      aria-controls="analysis-file-list"
      tooltip-side="bottom"
      @click="emit('toggleList')"
    >
      <MdIcon :name="listCollapsed ? ICON_NAMES.sidebarExpand : ICON_NAMES.sidebarCollapse" :size="17" />
    </MdIconAction>
  </div>
</template>

<style scoped>
@reference "@assets/main.css";

.browser-toolbar {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr) auto;
  align-items: center;
  gap: 10px;
  border-bottom-width: 1px;
  padding: 3px 10px;
  @apply border-border;
}

.history-actions {
  display: flex;
  gap: 2px;
}

.browser-toolbar :deep(.history-action) {
  display: grid;
  width: 28px;
  height: 28px;
  place-items: center;
  border: 0;
  border-radius: 7px;
  background: transparent;
  @apply text-muted-foreground transition-colors duration-200;
  cursor: pointer;
}

.browser-toolbar :deep(.history-action-home) {
  margin-left: 6px;
}

.browser-toolbar :deep(.history-action:hover:not([aria-disabled='true'])) {
  @apply bg-muted text-card-foreground;
}

.browser-toolbar :deep(.history-action:focus-visible) {
  @apply outline-none ring-2 ring-inset ring-ring/35;
}

.browser-toolbar :deep(.history-action[aria-disabled='true']) {
  cursor: not-allowed;
  opacity: 0.45;
}

/*
 * Preserve the pre-navigation visual state while the shared action blocks
 * repeated requests. Controls unavailable before navigation remain dimmed
 * because they do not receive this data attribute.
 */
.browser-toolbar :deep(.history-action[data-busy-disabled='true']) {
  opacity: 1;
}

.breadcrumbs {
  display: flex;
  min-width: 0;
  align-items: center;
  gap: 2px;
  overflow-x: auto;
  font-size: var(--font-content-body);
}

.breadcrumbs > svg {
  flex: none;
  @apply text-muted-foreground;
}

.breadcrumb-segment {
  display: inline-flex;
  flex: none;
  align-items: center;
}

.breadcrumbs button {
  min-width: 0;
  max-width: 280px;
  flex: none;
  overflow: hidden;
  border: 0;
  border-radius: 6px;
  padding: 4px;
  background: transparent;
  @apply text-muted-foreground transition-colors duration-200;
  font: inherit;
  text-overflow: ellipsis;
  white-space: nowrap;
  cursor: pointer;
}

.breadcrumbs button:hover:not(:disabled) {
  @apply bg-accent/65 text-accent-foreground;
}

.breadcrumbs button:focus-visible {
  @apply outline-none ring-2 ring-ring/35;
}

.breadcrumbs button:disabled {
  opacity: 1;
}

.breadcrumbs button[aria-current='page'] {
  @apply text-card-foreground;
}

.breadcrumbs .sibling-trigger {
  display: grid;
  width: 20px;
  height: 24px;
  place-items: center;
  padding: 0;
}

.breadcrumbs .current-folder-trigger {
  display: inline-flex;
  height: 28px;
  align-items: center;
  gap: 6px;
  padding: 4px 6px;
}

.current-folder-label {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.breadcrumbs .current-folder-trigger > svg {
  flex: none;
}

.breadcrumbs :is(.sibling-trigger, .current-folder-trigger)[data-state='open'] {
  @apply bg-accent/65 text-accent-foreground;
}

@container analysis (max-width: 671px) {
  .browser-toolbar :deep(.list-toggle) {
    display: none;
  }
}
</style>

<style>
@reference "@assets/main.css";

/* Reka's portaled content does not inherit the toolbar's scoped style attribute. */
.md-analysis-sibling-menu {
  z-index: 50;
  width: 380px;
  max-width: calc(100vw - 24px);
  max-height: min(360px, var(--reka-dropdown-menu-content-available-height));
  overflow-y: auto;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 4px;
  @apply bg-popover text-popover-foreground;
  box-shadow: 0 4px 16px var(--shadow-subtle);
}

.md-analysis-sibling-menu .sibling-menu-item {
  position: relative;
  display: flex;
  align-items: center;
  gap: 8px;
  border-radius: calc(var(--radius) - 2px);
  min-height: 28px;
  padding: 4px 8px;
  overflow: hidden;
  isolation: isolate;
  outline: none;
  font-size: var(--font-content-body);
  line-height: 20px;
  cursor: default;
  user-select: none;
}

.md-analysis-sibling-menu .sibling-menu-item::before {
  position: absolute;
  z-index: -1;
  inset: 2px auto 2px 0;
  width: var(--sibling-share, 0%);
  border-radius: calc(var(--radius) - 2px) 0 0 calc(var(--radius) - 2px);
  background: var(--muted);
  content: '';
  opacity: 0.65;
  pointer-events: none;
}

.md-analysis-sibling-menu .sibling-menu-item::after {
  position: absolute;
  z-index: -2;
  inset: 0;
  border-radius: inherit;
  background: var(--muted);
  content: '';
  opacity: 0;
  pointer-events: none;
}

.md-analysis-sibling-menu .sibling-menu-item[data-highlighted] {
  @apply text-popover-foreground;
}

.md-analysis-sibling-menu .sibling-menu-item[data-highlighted]::after {
  opacity: 0.35;
}

.md-analysis-sibling-menu .sibling-menu-item[data-highlighted]::before {
  opacity: 0.85;
}

.md-analysis-sibling-menu .sibling-menu-item.current-folder .sibling-name {
  font-weight: 600;
}

.md-analysis-sibling-menu .sibling-menu-item.current-folder::after {
  opacity: 0.15;
}

.md-analysis-sibling-menu .sibling-menu-item > svg,
.md-analysis-sibling-menu .sibling-menu-item small {
  flex: none;
}

.md-analysis-sibling-menu .sibling-metrics {
  display: flex;
  flex: none;
  align-items: baseline;
  gap: 8px;
}

.md-analysis-sibling-menu .sibling-menu-item .sibling-percentage {
  width: 4ch;
  color: var(--muted-foreground);
  text-align: right;
}

.md-analysis-sibling-menu .sibling-menu-item small {
  @apply text-muted-foreground;
  font-size: var(--font-content-meta);
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}

.md-analysis-sibling-menu .sibling-name {
  min-width: 0;
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.md-analysis-sibling-menu .sibling-check-hidden {
  visibility: hidden;
}
</style>

<script setup lang="ts">
import type { SunburstSector } from '@/lib/utils/sunburst-layout';
import { ByteSizeService } from '@/lib/services/byte-size-service';

const props = defineProps<{
  sectors: SunburstSector[];
  highlightedKey: string | null;
  highlightedBranch: string | null;
  disabled: boolean;
  remainderLabel: string;
  chartLabel: string;
}>();
const emit = defineEmits<{
  hover: [sector: SunburstSector, event: PointerEvent];
  leave: [];
  activate: [sector: SunburstSector];
  openEntry: [sector: SunburstSector];
  focus: [sector: SunburstSector];
  sectorContextMenu: [sector: SunburstSector, event: MouseEvent];
}>();
function handleCanvasContextMenu(event: MouseEvent) {
  // Only a real sector can supply a menu target; the central hole and empty
  // outer ring areas must not reopen a menu for the last hovered directory.
  if (!(event.target instanceof Element) || !event.target.closest('.sunburst-sector')) event.preventDefault();
}
function label(sector: SunburstSector) {
  const name = sector.name || props.remainderLabel;
  let length = 0;
  let text = '';
  for (const character of name) {
    length += (character.codePointAt(0) ?? 0) < 128 ? 1 : 1.8;
    if (length > sector.labelLength - 1) return `${text}…`;
    text += character;
  }
  return text;
}
</script>

<template>
  <svg
    viewBox="-240 -240 480 480"
    class="sunburst-chart"
    :aria-label="chartLabel"
    @contextmenu="handleCanvasContextMenu"
  >
    <g
      v-for="sector in sectors"
      :key="sector.key"
      v-memo="[
        sector,
        disabled,
        highlightedKey === sector.key,
        !!highlightedBranch && highlightedBranch === sector.branchPath,
        remainderLabel,
      ]"
    >
      <path
        :d="sector.arc"
        class="sunburst-sector"
        :class="{
          remainder: !sector.path,
          'is-highlighted':
            !disabled &&
            (highlightedKey === sector.key || (!!highlightedBranch && highlightedBranch === sector.branchPath)),
        }"
        :style="{
          '--sector-color': `var(--chart-${sector.colorIndex + 1})`,
          '--sector-opacity': Math.max(0.24, 0.38 - (sector.depth - 1) * 0.04),
        }"
        role="button"
        :tabindex="!disabled ? 0 : undefined"
        :aria-disabled="disabled"
        :aria-label="`${sector.name || remainderLabel} · ${ByteSizeService.bytes(sector.bytes)}`"
        @contextmenu="emit('sectorContextMenu', sector, $event)"
        @pointerenter="!disabled && emit('hover', sector, $event)"
        @pointermove="!disabled && emit('hover', sector, $event)"
        @pointerleave="emit('leave')"
        @focus="!disabled && emit('focus', sector)"
        @blur="emit('leave')"
        @click="!disabled && emit('activate', sector)"
        @dblclick="sector.path && !disabled && emit('openEntry', sector)"
        @keydown.enter.prevent="!disabled && emit('openEntry', sector)"
        @keydown.space.prevent="!disabled && emit('activate', sector)"
      />
      <text
        v-if="sector.labelLength >= 8"
        :transform="sector.labelTransform"
        text-anchor="middle"
        dominant-baseline="middle"
      >
        {{ label(sector) }}
      </text>
    </g>
  </svg>
</template>

<style scoped>
@reference "@assets/main.css";
.sunburst-chart {
  display: block;
  width: 100%;
  height: 100%;
}
.sunburst-sector {
  fill: var(--sector-color);
  fill-opacity: var(--sector-opacity);
  stroke: var(--card);
  stroke-width: 1;
  outline: none;
  cursor: pointer;
  transition:
    fill-opacity 0.15s ease,
    stroke 0.15s ease;
}
.sunburst-sector.remainder {
  fill: var(--muted);
  fill-opacity: 0.85;
}
.sunburst-sector.is-highlighted {
  fill-opacity: 0.6;
  stroke: var(--muted-foreground);
}
.sunburst-sector:focus-visible {
  outline: none;
  fill-opacity: 0.65;
  stroke: var(--ring);
  stroke-width: 2;
}
text {
  fill: var(--card-foreground);
  font-size: 10px;
  pointer-events: none;
}
</style>

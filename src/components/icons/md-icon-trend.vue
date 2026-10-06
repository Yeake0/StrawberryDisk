<script setup lang="ts">
// Geometry is supplied by the owning chart; this component only paints paths.
defineProps<{ series: { line: string; area: string; color: string }[]; amplitude?: number; offset?: number }>();
</script>

<template>
  <svg class="trend-lines" viewBox="0 0 100 100" preserveAspectRatio="none" aria-hidden="true">
    <g :transform="`translate(${offset ?? 0} 50) scale(1 ${amplitude ?? 1}) translate(0 -50)`">
      <g v-for="(seriesItem, index) in series" :key="index" :style="{ color: seriesItem.color }">
        <path :d="seriesItem.area" fill="currentColor" opacity="0.07" />
        <path
          :d="seriesItem.line"
          fill="none"
          stroke="currentColor"
          stroke-width="1.5"
          stroke-linecap="round"
          stroke-linejoin="round"
          vector-effect="non-scaling-stroke"
          opacity="0.85"
        />
      </g>
    </g>
  </svg>
</template>

<style scoped>
.trend-lines {
  display: block;
  width: 100%;
  height: 100%;
  /* The chart viewport clips scrolling history, not this rebased SVG canvas. */
  overflow: visible;
}
</style>

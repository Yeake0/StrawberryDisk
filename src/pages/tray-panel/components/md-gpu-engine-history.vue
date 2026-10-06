<script setup lang="ts">
import type { TrendPoint } from '@/lib/models/system-resources';
import MdResourceTrend from './md-resource-trend.vue';

defineProps<{ history: TrendPoint[]; observedAtMs: number; active: boolean; label: string }>();
</script>

<template>
  <div class="engine-history">
    <span class="scale-top">100%</span>
    <span class="scale-bottom">0%</span>
    <MdResourceTrend metric="gpu" :history="history" :observed-at-ms="observedAtMs" :active="active" :label="label" />
  </div>
</template>

<style scoped>
@reference "@assets/main.css";
.engine-history {
  @apply rounded-lg bg-muted/30;
  position: relative;
  padding: 8px 8px 8px 30px;
  margin-top: 6px;
}
.engine-history :deep(.resource-trend) {
  height: 72px;
  margin-top: 0;
  border-bottom: 0;
  background: linear-gradient(to bottom, transparent 49.5%, var(--border) 50%, transparent 50.5%);
}
.scale-top,
.scale-bottom {
  @apply text-muted-foreground;
  position: absolute;
  left: 5px;
  font-size: 8px;
  line-height: 12px;
}
.scale-top {
  top: 8px;
}
.scale-bottom {
  bottom: 8px;
}
</style>

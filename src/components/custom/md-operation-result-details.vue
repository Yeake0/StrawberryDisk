<script setup lang="ts">
import MdIcon from '@/components/icons/md-icon.vue';
import { ICON_NAMES } from '@/lib/models/ui';

interface OperationResultStat {
  key: string;
  label: string;
  value: string;
  tone?: 'neutral' | 'warning';
}

interface OperationResultItem {
  key: string;
  title: string;
  description: string;
  value: string;
  tone?: 'positive' | 'warning';
}

defineProps<{
  items: readonly OperationResultItem[];
  stats: readonly OperationResultStat[];
}>();
</script>

<template>
  <div class="operation-result-stats flex-none" :data-count="Math.min(stats.length, 3)">
    <span v-for="stat in stats" :key="stat.key" :class="{ warn: stat.tone === 'warning' }">
      <small>{{ stat.label }}</small>
      <strong>{{ stat.value }}</strong>
    </span>
  </div>
  <div class="operation-result-items scrollbar-stable min-h-0 flex-1">
    <div v-for="item in items" :key="item.key">
      <span :class="{ warn: item.tone === 'warning' }">
        <MdIcon :name="item.tone === 'warning' ? ICON_NAMES.info : ICON_NAMES.check" :size="13" />
      </span>
      <span>
        <strong>{{ item.title }}</strong>
        <small>{{ item.description }}</small>
      </span>
      <strong>{{ item.value }}</strong>
    </div>
  </div>
</template>

<style scoped>
@reference "@assets/main.css";

.operation-result-stats {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 8px;
  margin: 0 20px;
}

.operation-result-stats[data-count='1'] {
  grid-template-columns: minmax(0, 1fr);
}

.operation-result-stats[data-count='3'] {
  grid-template-columns: repeat(3, minmax(0, 1fr));
}

.operation-result-stats > span {
  @apply border border-border/60 bg-muted/30;
  display: flex;
  min-width: 0;
  flex-direction: column;
  border-radius: 9px;
  padding: 10px 11px;
}

.operation-result-stats small,
.operation-result-items small {
  @apply text-muted-foreground;
}

.operation-result-stats small {
  font-size: 10.5px;
}

.operation-result-stats strong {
  margin-top: 3px;
  font-size: 18px;
  font-variant-numeric: tabular-nums;
}

.operation-result-stats > span.warn strong {
  @apply text-warning-foreground;
}

.operation-result-items {
  @apply border border-border/70;
  margin: 10px 20px;
  border-radius: 9px;
}

.operation-result-items > div {
  @apply border-t border-border/70;
  display: grid;
  grid-template-columns: 22px minmax(0, 1fr) auto;
  align-items: center;
  gap: 8px;
  padding: 7px 10px;
}

.operation-result-items > div:first-child {
  border-top: 0;
}

.operation-result-items div > span:nth-child(2) {
  display: flex;
  min-width: 0;
  flex-direction: column;
}

.operation-result-items small {
  margin-top: 1px;
  font-size: 10.5px;
  line-height: 1.35;
}

.operation-result-items > div > span:nth-child(2) > strong,
.operation-result-items > div > strong {
  font-size: 13px;
  line-height: 1.35;
}

.operation-result-items > div > strong {
  font-variant-numeric: tabular-nums;
}

.operation-result-items > div > span:first-child {
  @apply text-success;
  background: var(--surface-success-subtle);
  display: grid;
  width: 20px;
  height: 20px;
  place-items: center;
  border-radius: 50%;
}

.operation-result-items > div > span.warn {
  @apply text-warning-foreground;
  background: var(--surface-warning-subtle);
}
</style>

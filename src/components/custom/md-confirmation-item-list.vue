<script setup lang="ts">
import MdTooltip from '@/components/custom/md-tooltip.vue';
interface MdConfirmationItem {
  key: string;
  title: string;
  description?: string;
  badge?: string;
  badgeTone?: 'positive' | 'warning' | 'neutral';
  value: string;
}

defineProps<{
  items: readonly MdConfirmationItem[];
}>();
</script>

<template>
  <div class="confirmation-item-list">
    <div v-for="item in items" :key="item.key">
      <span class="confirmation-item-copy">
        <span class="confirmation-item-title">
          <strong>{{ item.title }}</strong>
          <em v-if="item.badge" class="confirmation-item-badge" :class="item.badgeTone">{{ item.badge }}</em>
        </span>
        <MdTooltip v-if="item.description" :text="item.description"
          ><small>{{ item.description }}</small></MdTooltip
        >
      </span>
      <strong class="confirmation-item-value">{{ item.value }}</strong>
    </div>
  </div>
</template>

<style scoped>
@reference "@assets/main.css";

.confirmation-item-list {
  @apply border border-border/70;
  border-radius: 9px;
}

.confirmation-item-list > div {
  @apply border-t border-border/70;
  display: grid;
  min-height: 52px;
  grid-template-columns: minmax(0, 1fr) auto;
  align-items: center;
  gap: 16px;
  padding: 7px 12px;
}

.confirmation-item-list > div:first-child {
  border-top: 0;
}

.confirmation-item-copy {
  display: flex;
  min-width: 0;
  flex-direction: column;
  gap: 2px;
}

.confirmation-item-title > strong,
.confirmation-item-value {
  font-size: 13px;
  font-weight: 500;
  line-height: 1.35;
}

.confirmation-item-title {
  display: flex;
  min-width: 0;
  align-items: center;
  gap: 7px;
}

.confirmation-item-title > strong {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.confirmation-item-badge {
  display: inline-flex;
  flex: none;
  align-items: center;
  border-radius: 999px;
  padding: 2px 6px;
  font-size: 9px;
  font-style: normal;
  font-weight: 500;
  white-space: nowrap;
}

.confirmation-item-badge.positive {
  @apply text-success-foreground;
  background: var(--surface-success-subtle);
}

.confirmation-item-badge.warning {
  @apply text-warning-foreground;
  background: var(--surface-warning-subtle);
}

.confirmation-item-badge.neutral {
  @apply bg-muted text-muted-foreground;
}

.confirmation-item-copy small {
  @apply text-muted-foreground;
  overflow: hidden;
  font-size: 10.5px;
  line-height: 1.45;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.confirmation-item-value {
  white-space: nowrap;
}
</style>

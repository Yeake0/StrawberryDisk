<script setup lang="ts">
import MdIcon from '@/components/icons/md-icon.vue';
import type { IconName } from '@/lib/models/ui';

defineProps<{
  active: boolean;
  title: string;
  description: string;
  iconName: IconName;
  selectedSummary?: string;
  selectedAriaLabel?: string;
}>();
const emit = defineEmits<{
  select: [];
}>();
</script>

<template>
  <button
    class="result-category-item"
    :class="{ active }"
    type="button"
    :aria-current="active ? 'page' : undefined"
    @click="emit('select')"
  >
    <span class="result-category-icon"><MdIcon :name="iconName" :size="19" /></span>
    <span class="result-category-copy">
      <strong>{{ title }}</strong>
      <small>{{ description }}</small>
    </span>
    <span v-if="selectedSummary" class="result-category-selected" :aria-label="selectedAriaLabel">
      {{ selectedSummary }}
    </span>
  </button>
</template>

<style scoped>
@reference "@assets/main.css";

.result-category-item {
  position: relative;
  display: grid;
  width: 100%;
  min-width: 0;
  grid-template-columns: 28px minmax(0, 1fr) auto;
  align-items: center;
  gap: 9px;
  border-radius: var(--radius);
  padding: 8px 10px;
  text-align: left;
  transition:
    background-color 0.16s ease,
    color 0.16s ease;
}

.result-category-item + .result-category-item {
  margin-top: 2px;
}

.result-category-item:hover {
  @apply bg-muted/60;
}

.result-category-item:focus-visible {
  @apply outline-none ring-2 ring-inset ring-ring/45;
}

.result-category-item.active {
  @apply text-foreground;
  background: var(--surface-primary-subtle);
}

.result-category-item.active::before {
  @apply bg-primary;
  position: absolute;
  top: 10px;
  bottom: 10px;
  left: 0;
  width: 2px;
  border-radius: 999px;
  content: '';
}

.result-category-icon {
  @apply text-muted-foreground;
  display: grid;
  width: 28px;
  height: 32px;
  flex: none;
  place-items: center;
}

.result-category-item.active .result-category-icon,
.result-category-item.active .result-category-selected {
  @apply text-primary;
}

.result-category-copy {
  display: flex;
  min-width: 0;
  flex-direction: column;
  gap: 2px;
}

.result-category-copy strong,
.result-category-copy small,
.result-category-selected {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.result-category-copy strong {
  color: inherit;
  font-size: 13px;
  font-weight: 500;
}

.result-category-copy small,
.result-category-selected {
  @apply text-muted-foreground;
  font-size: 11px;
}

.result-category-selected {
  max-width: 76px;
  padding-left: 6px;
  font-weight: 500;
  text-align: right;
}
</style>

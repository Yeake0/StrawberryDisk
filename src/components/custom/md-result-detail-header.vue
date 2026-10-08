<script setup lang="ts">
import MdIconAction from '@/components/custom/md-icon-action.vue';
import MdResultCheckbox from '@/components/custom/md-result-checkbox.vue';
import MdIcon from '@/components/icons/md-icon.vue';
import { ICON_NAMES } from '@/lib/models/ui';

withDefaults(
  defineProps<{
    title: string;
    selection: 'all' | 'partial' | 'none';
    selectLabel: string;
    disabled?: boolean;
    description?: string;
  }>(),
  { description: undefined, disabled: false }
);
const emit = defineEmits<{
  'update:selected': [selected: boolean];
}>();
</script>

<template>
  <header class="result-detail-header">
    <span class="result-detail-heading">
      <strong class="result-detail-title">{{ title }}</strong>
      <MdIconAction
        v-if="description"
        appearance="unstyled"
        class="md-help-action"
        :label="description"
        tooltip-side="bottom"
        tooltip-class="max-w-72 leading-relaxed"
      >
        <MdIcon :name="ICON_NAMES.info" :size="15" />
      </MdIconAction>
    </span>
    <span class="result-detail-metric"><slot name="metric" /></span>
    <label class="result-detail-selection">
      <MdResultCheckbox
        :checked="selection === 'all'"
        :indeterminate="selection === 'partial'"
        :disabled="disabled"
        @update:checked="emit('update:selected', $event)"
      />
      <span>{{ selectLabel }}</span>
    </label>
  </header>
</template>

<style scoped>
@reference "@assets/main.css";

.result-detail-header {
  @apply border-border;
  display: grid;
  min-height: 46px;
  flex: none;
  grid-template-columns: minmax(0, 1fr) auto auto;
  align-items: center;
  gap: 12px;
  border-bottom-width: 1px;
  padding: 5px 12px;
}

.result-detail-heading {
  display: flex;
  min-width: 0;
  align-items: center;
  gap: 6px;
}

.result-detail-title {
  min-width: 0;
  overflow: hidden;
  font-size: var(--font-content-primary);
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.result-detail-metric {
  display: flex;
  align-items: baseline;
  gap: 6px;
  white-space: nowrap;
}

.result-detail-metric :deep(small) {
  @apply text-muted-foreground;
  font-size: 10px;
}

.result-detail-metric :deep(strong) {
  @apply text-primary;
  font-size: 15px;
}

.result-detail-metric :deep(i) {
  @apply text-muted-foreground;
  font-size: 11px;
  font-style: normal;
}

.result-detail-selection {
  display: flex;
  align-items: center;
  gap: 7px;
  font-size: 12px;
  white-space: nowrap;
  cursor: pointer;
}

@container (max-width: 760px) {
  .result-detail-header {
    grid-template-columns: minmax(0, 1fr) auto;
    padding-inline: 10px;
  }

  .result-detail-metric,
  .result-detail-selection span {
    display: none;
  }
}
</style>

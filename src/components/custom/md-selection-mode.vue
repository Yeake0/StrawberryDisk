<script setup lang="ts">
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';

export interface MdSelectionModeOption {
  value: string;
  label: string;
  disabled?: boolean;
}

defineProps<{
  busy: boolean;
  displayValue: string;
  label: string;
  modelValue: string;
  options: readonly MdSelectionModeOption[];
}>();

const emit = defineEmits<{
  'update:modelValue': [value: unknown];
}>();
</script>

<template>
  <div class="selection-mode">
    <span>{{ label }}</span>
    <Select :model-value="modelValue" :disabled="busy" @update:model-value="emit('update:modelValue', $event)">
      <SelectTrigger :aria-label="label">
        <SelectValue>{{ displayValue }}</SelectValue>
      </SelectTrigger>
      <SelectContent>
        <SelectItem v-for="option in options" :key="option.value" :value="option.value" :disabled="option.disabled">
          {{ option.label }}
        </SelectItem>
      </SelectContent>
    </Select>
  </div>
</template>

<style scoped>
@reference "@assets/main.css";

.selection-mode {
  display: flex;
  min-width: 0;
  align-items: center;
  gap: 8px;
}

.selection-mode > span {
  @apply text-muted-foreground;
  flex: none;
  font-size: 11px;
}

.selection-mode :deep([data-slot='select-trigger']) {
  width: 202px;
  min-width: 202px;
  height: 38px;
}

.selection-mode :deep([data-slot='select-value']) {
  overflow: hidden;
  text-overflow: ellipsis;
}

@container (max-width: 760px) {
  .selection-mode > span {
    display: none;
  }

  .selection-mode :deep([data-slot='select-trigger']) {
    width: 148px;
    min-width: 148px;
  }
}
</style>

<script setup lang="ts">
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';

import MdSelectionMode, { type MdSelectionModeOption } from '@/components/custom/md-selection-mode.vue';
import type { CleanupSelectionMode } from '@/lib/utils/cleanup-rule-selection';
import { ByteSizeService } from '@/lib/services/byte-size-service';

const emit = defineEmits<{
  change: [value: unknown];
}>();

const { t } = useI18n({ useScope: 'global' });

const props = defineProps<{
  busy: boolean;
  mode: CleanupSelectionMode;
  recommendedBytes: number;
  totalBytes: number;
}>();

const options = computed<MdSelectionModeOption[]>(() => {
  const values: MdSelectionModeOption[] = [
    {
      value: 'smart',
      label: `${t('cleanup.selectionMode.smart')} · ${ByteSizeService.bytes(props.recommendedBytes)}`,
    },
    {
      value: 'all',
      label: `${t('cleanup.selectionMode.all')} · ${ByteSizeService.bytes(props.totalBytes)}`,
    },
    { value: 'none', label: t('cleanup.selectionMode.none') },
  ];
  if (props.mode === 'manual') {
    values.push({ value: 'manual', label: t('cleanup.selectionMode.manual'), disabled: true });
  }
  return values;
});
</script>

<template>
  <MdSelectionMode
    :busy="busy"
    :display-value="t(`cleanup.selectionMode.${mode}`)"
    :label="t('cleanup.selectionMode.label')"
    :model-value="mode"
    :options="options"
    @update:model-value="emit('change', $event)"
  />
</template>

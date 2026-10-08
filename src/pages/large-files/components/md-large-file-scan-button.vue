<script setup lang="ts">
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';

import MdSplitActionButton from '@/components/custom/md-split-action-button.vue';
import { LARGE_FILE_SCAN_MODES, type LargeFileScanMode } from '@/lib/models/large-file';
import { ICON_NAMES } from '@/lib/models/ui';

const props = withDefaults(
  defineProps<{
    action?: 'start' | 'rescan';
    busy: boolean;
    mode: LargeFileScanMode;
    selectableModes?: boolean;
    emphasized?: boolean;
  }>(),
  {
    action: 'start',
    selectableModes: false,
    emphasized: false,
  }
);

const emit = defineEmits<{
  scan: [mode: LargeFileScanMode];
}>();

const { t } = useI18n({ useScope: 'global' });

const menuItems = computed(() =>
  props.selectableModes
    ? [
        {
          value: LARGE_FILE_SCAN_MODES.quick,
          icon: ICON_NAMES.search,
          label: t('largeFiles.scanMode.quick'),
          description: t('largeFiles.scanMode.quickDescription'),
        },
        {
          value: LARGE_FILE_SCAN_MODES.complete,
          icon: ICON_NAMES.hardDrive,
          label: t('largeFiles.scanMode.complete'),
          description: t('largeFiles.scanMode.completeDescription'),
        },
      ]
    : []
);

function selectMode(value: string): void {
  if (value === LARGE_FILE_SCAN_MODES.quick || value === LARGE_FILE_SCAN_MODES.complete) {
    emit('scan', value);
  }
}
</script>

<template>
  <MdSplitActionButton
    :accessible-label="t('largeFiles.scanMode.label')"
    :disabled="busy"
    :items="menuItems"
    :primary-icon="action === 'start' ? ICON_NAMES.largeFiles : ICON_NAMES.refresh"
    :primary-label="t(action === 'start' ? 'largeFiles.start' : 'largeFiles.rescan')"
    :size="action === 'start' ? 'lg' : 'default'"
    :variant="action === 'rescan' && !emphasized ? 'outline' : 'default'"
    @primary="emit('scan', mode)"
    @select="selectMode"
  />
</template>

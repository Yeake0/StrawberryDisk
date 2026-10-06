<script setup lang="ts">
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';

import MdSplitActionButton from '@/components/custom/md-split-action-button.vue';
import { ICON_NAMES } from '@/lib/models/ui';

const props = withDefaults(
  defineProps<{
    action?: 'start' | 'rescan';
    busy: boolean;
  }>(),
  {
    action: 'start',
  }
);

const emit = defineEmits<{
  primary: [];
  standard: [];
  'select-volumes': [];
  custom: [];
}>();

const { t } = useI18n({ useScope: 'global' });

const menuItems = computed(() => [
  {
    value: 'standard',
    icon: ICON_NAMES.scan,
    label: t('cleanup.scanMode.standard'),
    description: t('cleanup.scanMode.standardDescription'),
  },
  {
    value: 'select-volumes',
    icon: ICON_NAMES.hardDrive,
    label: t('cleanup.scanMode.selectVolumes'),
    description: t('cleanup.scanMode.selectVolumesDescription'),
  },
  {
    value: 'custom',
    icon: ICON_NAMES.folderPlus,
    label: t('cleanup.scanMode.custom'),
    description: t('cleanup.scanMode.customDescription'),
  },
]);

function selectMode(value: string): void {
  if (value === 'standard') emit('standard');
  else if (value === 'select-volumes') emit('select-volumes');
  else if (value === 'custom') emit('custom');
}
</script>

<template>
  <MdSplitActionButton
    :accessible-label="t('cleanup.scanMode.label')"
    :disabled="busy"
    :items="menuItems"
    :primary-icon="props.action === 'start' ? ICON_NAMES.deepCleanup : ICON_NAMES.refresh"
    :primary-label="t(props.action === 'start' ? 'overview.startScan' : 'overview.rescan')"
    :size="props.action === 'start' ? 'lg' : 'default'"
    :variant="props.action === 'rescan' ? 'outline' : 'default'"
    @primary="emit('primary')"
    @select="selectMode"
  />
</template>

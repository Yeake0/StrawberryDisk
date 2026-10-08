<script setup lang="ts">
import { useI18n } from 'vue-i18n';

import MdIcon from '@/components/icons/md-icon.vue';
import { ICON_NAMES } from '@/lib/models/ui';

import MdTooltip from './md-tooltip.vue';

defineProps<{ hint: string }>();
const emit = defineEmits<{ open: [] }>();
const { t } = useI18n({ useScope: 'global' });
</script>

<template>
  <MdTooltip :text="hint">
    <button type="button" class="exclusion-link" @click="emit('open')">
      <MdIcon :name="ICON_NAMES.info" :size="13" aria-hidden="true" />
      {{ t('storageScanExclusions.activeLabel') }}
    </button>
  </MdTooltip>
</template>

<style scoped>
@reference "@assets/main.css";

.exclusion-link {
  display: inline-flex;
  flex: none;
  align-items: center;
  gap: 3px;
  border: 0;
  padding: 0;
  background: transparent;
  @apply text-primary;
  font: inherit;
  font-size: var(--font-content-meta);
  text-decoration: underline;
  text-underline-offset: 2px;
  white-space: nowrap;
  cursor: pointer;
}

.exclusion-link:hover {
  opacity: 0.8;
}

.exclusion-link:focus-visible {
  border-radius: 3px;
  @apply outline-none ring-2 ring-ring;
}
</style>

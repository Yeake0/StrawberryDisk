<script setup lang="ts">
import { ref } from 'vue';

import MdDialogContent from '@/components/custom/md-dialog-content.vue';
import MdDialogFooter from '@/components/custom/md-dialog-footer.vue';
import MdDialogHeader from '@/components/custom/md-dialog-header.vue';
import MdInlineNotice from '@/components/custom/md-inline-notice.vue';
import MdIcon from '@/components/icons/md-icon.vue';
import { Button } from '@/components/ui/button';
import { Dialog, DialogDescription, DialogTitle } from '@/components/ui/dialog';
import { ICON_NAMES } from '@/lib/models/ui';

const props = defineProps<{
  modelValue: boolean;
  summary: string;
  title: string;
  description: string;
  instructions: string;
  skipLabel: string;
  openSettingsLabel: string;
  openSettings: () => Promise<boolean>;
}>();
const emit = defineEmits<{
  'update:modelValue': [value: boolean];
}>();

const openingSettings = ref(false);

async function requestOpenSettings() {
  if (openingSettings.value) return;
  openingSettings.value = true;
  try {
    const opened = await props.openSettings();
    if (opened) emit('update:modelValue', false);
  } finally {
    openingSettings.value = false;
  }
}
</script>

<template>
  <button
    class="permission-summary"
    type="button"
    aria-haspopup="dialog"
    :aria-expanded="modelValue"
    :disabled="openingSettings"
    @click="emit('update:modelValue', true)"
  >
    <span>{{ summary }}</span>
    <MdIcon :name="ICON_NAMES.info" :size="13" aria-hidden="true" />
  </button>

  <Dialog :open="modelValue" @update:open="emit('update:modelValue', $event)">
    <MdDialogContent size="compact">
      <MdDialogHeader>
        <DialogTitle>{{ title }}</DialogTitle>
        <DialogDescription>{{ description }}</DialogDescription>
      </MdDialogHeader>
      <MdInlineNotice class="permission-instructions" :icon-name="ICON_NAMES.info" tone="info">
        {{ instructions }}
      </MdInlineNotice>
      <MdDialogFooter>
        <Button variant="outline" type="button" :disabled="openingSettings" @click="emit('update:modelValue', false)">
          {{ skipLabel }}
        </Button>
        <Button type="button" :disabled="openingSettings" @click="requestOpenSettings">
          <MdIcon :name="ICON_NAMES.external" :size="15" />
          {{ openSettingsLabel }}
        </Button>
      </MdDialogFooter>
    </MdDialogContent>
  </Dialog>
</template>

<style scoped>
@reference "@assets/main.css";

.permission-summary {
  display: flex;
  min-width: 0;
  max-width: min(440px, 46vw);
  align-items: center;
  gap: 5px;
  border: 0;
  padding: 4px 0;
  background: transparent;
  color: var(--primary);
  font: inherit;
  font-size: 12px;
  cursor: pointer;
}

.permission-summary span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.permission-summary :deep(svg) {
  flex: none;
}

.permission-summary:not(:disabled):hover {
  text-decoration: underline;
}

.permission-summary:disabled {
  cursor: default;
  opacity: 0.65;
}

.permission-instructions {
  margin: 0 var(--layout-dialog-body-inline-padding) 14px;
}
</style>

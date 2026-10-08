<script setup lang="ts">
import { useI18n } from 'vue-i18n';

import MdDialogContent from '@/components/custom/md-dialog-content.vue';
import MdDialogFooter from '@/components/custom/md-dialog-footer.vue';
import MdDialogHeader from '@/components/custom/md-dialog-header.vue';
import MdIcon from '@/components/icons/md-icon.vue';
import { Button } from '@/components/ui/button';
import { Dialog, DialogDescription, DialogTitle } from '@/components/ui/dialog';
import { ICON_NAMES } from '@/lib/models/ui';

defineProps<{
  open: boolean;
  itemNames: string[];
}>();

const emit = defineEmits<{
  'update:open': [value: boolean];
  confirm: [];
}>();

const { t } = useI18n({ useScope: 'global' });
</script>

<template>
  <Dialog :open="open" @update:open="emit('update:open', $event)">
    <MdDialogContent class="risk-dialog" size="standard">
      <MdDialogHeader variant="alert">
        <div class="risk-dialog-icon" aria-hidden="true">
          <MdIcon :name="ICON_NAMES.shield" :size="21" />
        </div>
        <div class="md-dialog-header-copy">
          <DialogTitle>{{ t('systemOptimization.riskDialog.title') }}</DialogTitle>
          <DialogDescription>{{ t('systemOptimization.riskDialog.description') }}</DialogDescription>
        </div>
      </MdDialogHeader>

      <ul class="risk-dialog-items">
        <li v-for="itemName in itemNames" :key="itemName">{{ itemName }}</li>
      </ul>

      <p class="risk-dialog-note">
        <MdIcon :name="ICON_NAMES.info" :size="15" />
        <span>{{ t('systemOptimization.riskDialog.note') }}</span>
      </p>

      <MdDialogFooter class="risk-dialog-footer">
        <Button variant="outline" type="button" @click="emit('update:open', false)">
          {{ t('common.cancel') }}
        </Button>
        <Button type="button" @click="emit('confirm')">
          {{ t('systemOptimization.riskDialog.confirm') }}
        </Button>
      </MdDialogFooter>
    </MdDialogContent>
  </Dialog>
</template>

<style scoped>
@reference "@assets/main.css";

.risk-dialog-icon {
  display: grid;
  width: 40px;
  height: 40px;
  place-items: center;
  border-radius: 10px;
  color: var(--warning-foreground);
  background: color-mix(in oklab, var(--warning) 14%, transparent);
}

.risk-dialog-items {
  display: grid;
  max-height: 180px;
  gap: 7px;
  margin: 0 var(--layout-dialog-body-inline-padding) 10px;
  overflow-y: auto;
  border-width: 1px;
  border-radius: 10px;
  padding: 9px 12px 9px 28px;
  @apply border-border bg-muted/40;
  font-size: 12px;
}

.risk-dialog-note {
  display: flex;
  align-items: flex-start;
  gap: 7px;
  margin: 0 var(--layout-dialog-body-inline-padding) 12px;
  color: var(--muted-foreground);
  font-size: 11px;
  line-height: 1.5;
}

.risk-dialog-note :deep(svg) {
  flex: none;
  margin-top: 1px;
  color: var(--primary);
}

.risk-dialog-footer :deep(button) {
  min-width: 96px;
}
</style>

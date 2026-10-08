<script setup lang="ts">
import MdTooltip from '@/components/custom/md-tooltip.vue';
import MdDialogContent from '@/components/custom/md-dialog-content.vue';
import MdDialogFooter from '@/components/custom/md-dialog-footer.vue';
import MdDialogHeader from '@/components/custom/md-dialog-header.vue';
import MdSpinner from '@/components/custom/md-spinner.vue';
import MdIcon from '@/components/icons/md-icon.vue';
import { Button } from '@/components/ui/button';
import { Dialog, DialogDescription, DialogTitle } from '@/components/ui/dialog';
import { ICON_NAMES } from '@/lib/models/ui';

const props = withDefaults(
  defineProps<{
    open: boolean;
    title: string;
    description: string;
    summaryLabel?: string;
    summaryValue?: string;
    note?: string;
    cancelLabel: string;
    confirmLabel: string;
    busy?: boolean;
    loading?: boolean;
    loadingLabel?: string;
    showDetails?: boolean;
  }>(),
  {
    summaryLabel: '',
    summaryValue: '',
    note: '',
    busy: false,
    loading: false,
    loadingLabel: '',
    showDetails: false,
  }
);

const emit = defineEmits<{
  'update:open': [value: boolean];
  confirm: [];
}>();

function updateOpen(open: boolean) {
  // A destructive command continues in Core after confirmation. Keep its
  // activity dialog visible until the owning Store settles so Escape, an
  // outside click, or the close control cannot hide the only progress signal.
  if (!open && props.busy) return;
  emit('update:open', open);
}
</script>

<template>
  <Dialog :open="open" @update:open="updateOpen">
    <!-- Shared hierarchy keeps risk, target summary, and actions consistent. -->
    <MdDialogContent class="flex min-h-0 flex-col" :size="showDetails ? 'large' : 'standard'" :show-close="!busy">
      <MdDialogHeader class="flex-none" variant="alert">
        <div class="destructive-dialog-icon" aria-hidden="true">
          <MdIcon :name="ICON_NAMES.trash" :size="20" />
        </div>
        <div class="md-dialog-header-copy">
          <DialogTitle>{{ title }}</DialogTitle>
          <DialogDescription>{{ description }}</DialogDescription>
        </div>
      </MdDialogHeader>

      <div v-if="summaryLabel || summaryValue" class="destructive-dialog-summary" aria-live="polite">
        <MdTooltip :text="summaryLabel"
          ><strong>
            <MdSpinner v-if="loading || busy" />
            <span class="destructive-dialog-summary-label">
              {{ loading && loadingLabel ? loadingLabel : summaryLabel }}
            </span>
          </strong></MdTooltip
        >
        <span>{{ summaryValue }}</span>
      </div>
      <p v-if="note" class="destructive-dialog-note">
        <MdIcon :name="ICON_NAMES.info" :size="15" />
        <span>{{ note }}</span>
      </p>
      <div v-if="showDetails && $slots.default" class="destructive-dialog-content">
        <slot />
      </div>

      <MdDialogFooter v-if="!showDetails" class="destructive-dialog-footer">
        <Button variant="outline" type="button" :disabled="busy" @click="updateOpen(false)">
          {{ cancelLabel }}
        </Button>
        <Button variant="destructive" type="button" :disabled="busy || loading" @click="emit('confirm')">
          <MdSpinner v-if="busy" />
          {{ confirmLabel }}
        </Button>
      </MdDialogFooter>
    </MdDialogContent>
  </Dialog>
</template>

<style scoped>
@reference "@assets/main.css";

.destructive-dialog-icon {
  display: grid;
  width: 40px;
  height: 40px;
  place-items: center;
  border-radius: 10px;
  @apply text-destructive;
  background: var(--surface-destructive-subtle);
}

.destructive-dialog-summary {
  display: flex;
  min-width: 0;
  min-height: 52px;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  margin: 0 var(--layout-dialog-body-inline-padding) 10px;
  border-width: 1px;
  border-radius: 10px;
  padding: 9px 12px;
  @apply border-border bg-muted/60;
}

.destructive-dialog-summary strong {
  display: flex;
  min-width: 0;
  flex: 1;
  align-items: center;
  gap: 9px;
}

.destructive-dialog-summary-label {
  min-width: 0;
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.destructive-dialog-summary > span {
  flex: none;
  @apply text-muted-foreground;
}

.destructive-dialog-note {
  display: flex;
  align-items: flex-start;
  gap: 7px;
  margin: 0 var(--layout-dialog-body-inline-padding) 12px;
  @apply text-muted-foreground;
  font-size: 11px;
  line-height: 1.5;
}

.destructive-dialog-note :deep(svg) {
  flex: none;
  margin-top: 1px;
  @apply text-primary;
}

.destructive-dialog-content {
  min-height: 0;
  margin: 0 var(--layout-dialog-body-inline-padding) 14px;
  overflow: hidden;
}

.destructive-dialog-footer :deep(button) {
  min-width: 96px;
}
</style>

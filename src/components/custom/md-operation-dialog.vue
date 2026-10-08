<script setup lang="ts">
import MdDialogContent from '@/components/custom/md-dialog-content.vue';
import MdDialogFooter from '@/components/custom/md-dialog-footer.vue';
import MdDialogHeader from '@/components/custom/md-dialog-header.vue';
import MdIcon from '@/components/icons/md-icon.vue';
import { Button } from '@/components/ui/button';
import { Dialog, DialogDescription, DialogTitle } from '@/components/ui/dialog';
import type { IconName } from '@/lib/models/ui';

interface OperationStat {
  key: string;
  label: string;
  value: string;
}

withDefaults(
  defineProps<{
    open: boolean;
    title: string;
    description: string;
    iconName: IconName;
    size?: 'compact' | 'standard' | 'large' | 'wide';
    progress?: number;
    progressLabel: string;
    stats?: readonly OperationStat[];
    cancelable?: boolean;
    cancelDisabled?: boolean;
    cancelLabel?: string;
  }>(),
  {
    size: 'large',
    progress: undefined,
    stats: () => [],
    cancelable: false,
    cancelDisabled: false,
    cancelLabel: undefined,
  }
);
defineEmits<{ cancel: [] }>();
</script>

<template>
  <!-- Only the owning workflow may close an active operation; dismissal never cancels Core work. -->
  <Dialog :open="open">
    <MdDialogContent
      class="flex min-h-0 flex-col"
      :size="size"
      :show-close="false"
      @escape-key-down.prevent
      @interact-outside.prevent
    >
      <MdDialogHeader class="flex-none" variant="alert" role="status" aria-live="polite" aria-atomic="true">
        <span class="operation-dialog-icon"><MdIcon :name="iconName" :size="24" /></span>
        <div class="md-dialog-header-copy min-w-0 break-words">
          <DialogTitle>{{ title }}</DialogTitle>
          <DialogDescription>{{ description }}</DialogDescription>
        </div>
      </MdDialogHeader>
      <div class="operation-dialog-body scrollbar-stable">
        <slot />
        <div
          class="operation-dialog-progress"
          :class="{ indeterminate: progress === undefined }"
          role="progressbar"
          :aria-label="progressLabel"
          :aria-valuemin="0"
          :aria-valuemax="100"
          :aria-valuenow="progress === undefined ? undefined : Math.round(progress)"
        >
          <span class="md-operational-motion" :style="progress === undefined ? undefined : { width: `${progress}%` }" />
        </div>
        <div v-if="stats.length" class="operation-dialog-stats" :style="{ '--stat-count': stats.length }">
          <span v-for="stat in stats" :key="stat.key">
            <small>{{ stat.label }}</small>
            <strong>{{ stat.value }}</strong>
          </span>
        </div>
      </div>
      <MdDialogFooter v-if="cancelable">
        <Button variant="outline" type="button" :disabled="cancelDisabled" @click="$emit('cancel')">
          {{ cancelLabel }}
        </Button>
      </MdDialogFooter>
    </MdDialogContent>
  </Dialog>
</template>

<style scoped>
@reference "@assets/main.css";

.operation-dialog-icon {
  display: grid;
  width: 40px;
  height: 40px;
  place-items: center;
  border-radius: 10px;
  @apply text-primary;
  background: var(--surface-primary-subtle);
}
.operation-dialog-body {
  display: flex;
  min-height: 0;
  flex: 1;
  flex-direction: column;
  gap: 14px;
  overflow-y: auto;
  overscroll-behavior: contain;
  padding: 14px var(--layout-dialog-body-inline-padding) 20px;
}
.operation-dialog-progress {
  height: 4px;
  flex: none;
  overflow: hidden;
  border-radius: 999px;
  background: var(--surface-primary-subtle);
}
.operation-dialog-progress > span {
  display: block;
  height: 100%;
  border-radius: inherit;
  @apply bg-primary transition-[width] duration-300 ease-out;
}
.operation-dialog-progress.indeterminate > span {
  width: 38%;
  animation: operation-dialog-activity 1.35s ease-in-out infinite;
}
.operation-dialog-stats {
  display: grid;
  flex: none;
  grid-template-columns: repeat(var(--stat-count), minmax(0, 1fr));
  gap: 8px;
}
.operation-dialog-stats > span {
  display: flex;
  min-width: 0;
  flex-direction: column;
  gap: 3px;
  border-radius: 9px;
  padding: 9px 10px;
  @apply bg-muted/45;
}
.operation-dialog-stats small {
  @apply text-muted-foreground;
  font-size: 10.5px;
  line-height: 1.35;
}
.operation-dialog-stats strong {
  overflow: hidden;
  font-size: 13px;
  font-variant-numeric: tabular-nums;
  line-height: 1.25;
  text-overflow: ellipsis;
  white-space: nowrap;
}
@keyframes operation-dialog-activity {
  0% {
    transform: translateX(-110%);
  }
  100% {
    transform: translateX(280%);
  }
}
</style>

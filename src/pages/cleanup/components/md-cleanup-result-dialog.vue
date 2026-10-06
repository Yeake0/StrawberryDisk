<script setup lang="ts">
import { useI18n } from 'vue-i18n';
import { computed } from 'vue';
import MdDialogContent from '@/components/custom/md-dialog-content.vue';
import MdDialogFooter from '@/components/custom/md-dialog-footer.vue';
import MdDialogHeader from '@/components/custom/md-dialog-header.vue';
import MdOperationResultDetails from '@/components/custom/md-operation-result-details.vue';
import { Button } from '@/components/ui/button';
import { Dialog, DialogDescription, DialogTitle } from '@/components/ui/dialog';
import type { ApplicationLeftoverResult } from '@/lib/models/application';
import type { PresentedCleanupResult } from '@/lib/models/cleanup';
import { ByteSizeService } from '@/lib/services/byte-size-service';
import * as FormatUtils from '@/lib/utils/format';

const { t } = useI18n({ useScope: 'global' });

const props = defineProps<{
  leftoverResult: ApplicationLeftoverResult | null;
  modelValue: boolean;
  result: PresentedCleanupResult | null;
}>();
const emit = defineEmits<{
  'update:modelValue': [open: boolean];
}>();

const availableResults = computed(() => [props.result, props.leftoverResult].filter(result => result !== null));
const hasResult = computed(() => availableResults.value.length > 0);
const dryRun = computed(() => availableResults.value.every(result => result.dryRun));
const cancelled = computed(
  () =>
    props.result?.record.outcome === 'cancelled' ||
    props.leftoverResult?.actions.some(action => action.status === 'cancelled') === true
);
const expectedBytes = computed(() => availableResults.value.reduce((total, result) => total + result.expectedBytes, 0));
const releasedBytes = computed(() => availableResults.value.reduce((total, result) => total + result.releasedBytes, 0));
const affectedItemCount = computed(() =>
  availableResults.value.reduce((total, result) => total + result.affectedItemCount, 0)
);
const failedItemCount = computed(() =>
  availableResults.value.reduce((total, result) => total + result.failedItemCount, 0)
);
const resultActions = computed(() => {
  const actions = (props.result?.actions ?? []).map(action => ({
    failed: Boolean(action.failedItemCount),
    key: `cleanup:${action.ruleId}`,
    message: action.message,
    name: action.name,
    releasedBytes: action.releasedBytes,
  }));
  const leftoverResult = props.leftoverResult;
  if (!leftoverResult) return actions;

  return [
    ...actions,
    {
      failed: Boolean(leftoverResult.failedItemCount),
      key: 'application-leftovers',
      message: leftoverResult.dryRun
        ? t('cleanup.previewPassed')
        : t('applicationLeftovers.executionSummary', {
            count: FormatUtils.integer(leftoverResult.affectedItemCount),
            size: ByteSizeService.bytes(leftoverResult.releasedBytes),
            failed: FormatUtils.integer(leftoverResult.failedItemCount),
          }),
      name: t('applicationLeftovers.categoryTitle'),
      releasedBytes: leftoverResult.releasedBytes,
    },
  ];
});
const resultStats = computed(() => [
  {
    key: 'released-bytes',
    label: dryRun.value ? t('cleanup.estimated') : t('cleanup.actualReleased'),
    value: ByteSizeService.bytes(dryRun.value ? expectedBytes.value : releasedBytes.value),
  },
  {
    key: 'processed-items',
    label: t('cleanup.processedItems'),
    value: FormatUtils.integer(affectedItemCount.value),
  },
  ...(failedItemCount.value
    ? [
        {
          key: 'failed-items',
          label: t('cleanup.failedItems'),
          value: FormatUtils.integer(failedItemCount.value),
          tone: 'warning' as const,
        },
      ]
    : []),
]);
const resultDetailItems = computed(() =>
  resultActions.value.map(action => ({
    key: action.key,
    title: action.name,
    description: action.message,
    value: ByteSizeService.bytes(action.releasedBytes),
    tone: action.failed ? ('warning' as const) : ('positive' as const),
  }))
);
const usesScrollableLayout = computed(() => resultActions.value.length > 5);

function updateOpen(open: boolean) {
  // A cleanup result may not exist immediately after execution starts. Sync
  // parent state only on an actual close; the open expression displays each new
  // result without a page-level watcher.
  if (!open) emit('update:modelValue', false);
}

function preventOutsideDismiss(event: Event) {
  event.preventDefault();
}
</script>

<template>
  <Dialog :open="modelValue && hasResult" @update:open="updateOpen">
    <MdDialogContent
      class="flex min-h-0 flex-col"
      :height="usesScrollableLayout ? 'tall' : 'auto'"
      size="large"
      @interact-outside="preventOutsideDismiss"
    >
      <template v-if="hasResult">
        <MdDialogHeader class="flex-none">
          <DialogTitle>{{
            cancelled ? t('cleanup.cancelled') : dryRun ? t('cleanup.previewCompleted') : t('cleanup.completed')
          }}</DialogTitle>
          <DialogDescription>{{
            cancelled ? t('cleanup.cancelledResultDescription') : t('cleanup.resultDescription')
          }}</DialogDescription>
        </MdDialogHeader>

        <MdOperationResultDetails :stats="resultStats" :items="resultDetailItems" />

        <MdDialogFooter>
          <Button variant="outline" type="button" @click="emit('update:modelValue', false)">
            {{ t('common.close') }}
          </Button>
        </MdDialogFooter>
      </template>
    </MdDialogContent>
  </Dialog>
</template>

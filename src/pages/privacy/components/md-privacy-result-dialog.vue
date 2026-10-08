<script setup lang="ts">
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';

import MdDialogContent from '@/components/custom/md-dialog-content.vue';
import MdDialogFooter from '@/components/custom/md-dialog-footer.vue';
import MdDialogHeader from '@/components/custom/md-dialog-header.vue';
import MdOperationResultDetails from '@/components/custom/md-operation-result-details.vue';
import { Button } from '@/components/ui/button';
import { Dialog, DialogDescription, DialogTitle } from '@/components/ui/dialog';
import type { PrivacyExecutionPlan, PrivacyExecutionResult } from '@/lib/models/privacy';
import * as FormatUtils from '@/lib/utils/format';

const props = defineProps<{
  modelValue: boolean;
  plan: PrivacyExecutionPlan | null;
  result: PrivacyExecutionResult | null;
}>();
const emit = defineEmits<{
  'update:modelValue': [open: boolean];
}>();
const { t } = useI18n({ useScope: 'global' });

const planItems = computed(() => new Map(props.plan?.items.map(item => [item.token, item]) ?? []));
const incompleteCount = computed(
  () => props.result?.items.filter(item => item.status === 'failed' || item.status === 'cancelled').length ?? 0
);
const stats = computed(() => [
  {
    key: 'cleared-traces',
    label: t('privacy.result.cleanedTraces'),
    value: FormatUtils.integer(props.result?.affectedItemCount ?? 0),
  },
  {
    key: 'processed-items',
    label: t('privacy.result.processedItems'),
    value: FormatUtils.integer(props.result?.items.length ?? 0),
  },
  ...(incompleteCount.value
    ? [
        {
          key: 'incomplete-items',
          label: t('privacy.result.incompleteItems'),
          value: FormatUtils.integer(incompleteCount.value),
          tone: 'warning' as const,
        },
      ]
    : []),
]);
const items = computed(() =>
  (props.result?.items ?? []).map(result => {
    const plan = planItems.value.get(result.token);
    return {
      key: result.token,
      title: plan ? [plan.sourceName, plan.profileName].filter(Boolean).join(' · ') : t('privacy.result.unknownSource'),
      description: plan
        ? `${t(`privacy.kinds.${plan.kind}`)} · ${t(`privacy.result.statuses.${result.status}`)}`
        : t(`privacy.result.statuses.${result.status}`),
      value: t(
        'privacy.traceCount',
        { count: FormatUtils.integer(result.affectedItemCount) },
        result.affectedItemCount
      ),
      tone: result.status === 'failed' || result.status === 'cancelled' ? ('warning' as const) : ('positive' as const),
    };
  })
);
const usesScrollableLayout = computed(() => items.value.length > 5);

function updateOpen(open: boolean) {
  if (!open) emit('update:modelValue', false);
}

function preventOutsideDismiss(event: Event) {
  event.preventDefault();
}
</script>

<template>
  <Dialog :open="modelValue && Boolean(result)" @update:open="updateOpen">
    <MdDialogContent
      class="flex min-h-0 flex-col"
      :height="usesScrollableLayout ? 'tall' : 'auto'"
      size="large"
      @interact-outside="preventOutsideDismiss"
    >
      <template v-if="result">
        <MdDialogHeader class="flex-none">
          <DialogTitle>{{ t('privacy.result.title') }}</DialogTitle>
          <DialogDescription>
            {{
              incompleteCount
                ? t('privacy.result.partialDescription', {
                    count: result.affectedItemCount,
                    failed: incompleteCount,
                  })
                : t('privacy.result.successDescription', { count: result.affectedItemCount })
            }}
          </DialogDescription>
        </MdDialogHeader>

        <MdOperationResultDetails :stats="stats" :items="items" />

        <MdDialogFooter>
          <Button variant="outline" type="button" @click="emit('update:modelValue', false)">
            {{ t('common.close') }}
          </Button>
        </MdDialogFooter>
      </template>
    </MdDialogContent>
  </Dialog>
</template>

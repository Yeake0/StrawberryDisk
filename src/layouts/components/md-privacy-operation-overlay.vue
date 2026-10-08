<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';

import MdConfirmDialog from '@/components/custom/md-confirm-dialog.vue';
import MdOperationDialog from '@/components/custom/md-operation-dialog.vue';
import MdIcon from '@/components/icons/md-icon.vue';
import type { PrivacyExecutionItemResult } from '@/lib/models/privacy';
import { ICON_NAMES } from '@/lib/models/ui';
import * as FormatUtils from '@/lib/utils/format';
import { usePrivacyStore } from '@/stores/privacy-store';

const emit = defineEmits<{ cancel: [] }>();
const { t } = useI18n({ useScope: 'global' });
const privacyStore = usePrivacyStore();
const cancellationConfirmOpen = ref(false);
const clockMs = ref(Date.now());
const itemListElement = ref<HTMLElement | null>(null);
let clockTimer: ReturnType<typeof setInterval> | null = null;
type ExecutionItemState = PrivacyExecutionItemResult['status'] | 'active' | 'pending';

const progress = computed(() => privacyStore.executionProgress);
const planItems = computed(() => privacyStore.executionItems);
const total = computed(() => progress.value?.totalItemCount ?? planItems.value.length);
const progressDivisor = computed(() => Math.max(total.value, 1));
const completed = computed(() => Math.min(progress.value?.completedItemCount ?? 0, total.value));
const elapsedMs = computed(() => {
  const reported = progress.value?.elapsedMs ?? 0;
  const startedAt = privacyStore.executionStartedAtMs;
  const live = startedAt === null ? 0 : Math.max(0, clockMs.value - startedAt);
  return Math.max(reported, live);
});
const elapsedSeconds = computed(() => Math.floor(elapsedMs.value / 1000));
const percent = computed(() => {
  if (!progress.value || progress.value.stage === 'validating') return 5;
  if (progress.value.stage === 'finalizing') return 98;
  return 10 + (completed.value / progressDivisor.value) * 85;
});
const stageLabel = computed(() => {
  if (privacyStore.cancellingExecution) return t('loading.cancellingCleanup');
  if (progress.value?.stage === 'validating') return t('loading.validating');
  if (progress.value?.stage === 'finalizing') return t('loading.finalizing');
  return t('privacy.executing');
});
const activeItem = computed(() => {
  if (progress.value?.stage !== 'cleaning') return null;
  return planItems.value.find(item => item.token === progress.value?.currentToken) ?? null;
});
const title = computed(() =>
  !privacyStore.cancellingExecution && activeItem.value
    ? t('loading.cleaningCurrentItem', { name: activeItem.value.sourceName })
    : stageLabel.value
);
const items = computed(() => {
  const terminalStates = new Map(progress.value?.completedItems.map(item => [item.token, item.status]) ?? []);
  return planItems.value.map(item => ({
    ...item,
    state: (terminalStates.get(item.token) ??
      (progress.value?.stage === 'cleaning' && item.token === progress.value.currentToken
        ? 'active'
        : 'pending')) as ExecutionItemState,
  }));
});

const description = computed(() =>
  privacyStore.cancellingExecution
    ? t('loading.cancellingCleanupHint')
    : t('loading.cleanupProgressSummary', {
        completed: FormatUtils.integer(completed.value),
        total: FormatUtils.integer(total.value),
      })
);
const stats = computed(() => [
  {
    key: 'items',
    label: t('loading.ruleProgress'),
    value: t('loading.ruleProgressValue', {
      completed: FormatUtils.integer(completed.value),
      total: FormatUtils.integer(total.value),
    }),
  },
  {
    key: 'elapsed',
    label: t('loading.elapsed'),
    value: t('loading.elapsedSeconds', { count: FormatUtils.integer(elapsedSeconds.value) }, elapsedSeconds.value),
  },
  {
    key: 'processed',
    label: t('loading.processedItems'),
    value: FormatUtils.integer(progress.value?.affectedItemCount ?? 0),
  },
]);

function itemStatusLabel(state: ExecutionItemState): string {
  switch (state) {
    case 'cleared':
      return t('loading.cleanupItemDone');
    case 'unchanged':
      return t('loading.cleanupItemUnchanged');
    case 'failed':
      return t('loading.cleanupItemFailed');
    case 'cancelled':
      return t('loading.cleanupItemCancelled');
    case 'active':
      return t('loading.cleanupItemActive');
    default:
      return t('loading.cleanupItemPending');
  }
}

watch(
  () => privacyStore.executing,
  executing => {
    if (!executing) cancellationConfirmOpen.value = false;
  }
);
watch(completed, async () => {
  await nextTick();
  itemListElement.value?.querySelector<HTMLElement>('.privacy-operation-item.is-active')?.scrollIntoView({
    block: 'nearest',
  });
});

function requestCancellation() {
  if (privacyStore.cancellingExecution) return;
  cancellationConfirmOpen.value = true;
}

function confirmCancellation() {
  cancellationConfirmOpen.value = false;
  emit('cancel');
}

onMounted(() => {
  clockTimer = window.setInterval(() => {
    if (privacyStore.executing) clockMs.value = Date.now();
  }, 1000);
});
onBeforeUnmount(() => {
  if (clockTimer) window.clearInterval(clockTimer);
});
</script>

<template>
  <MdOperationDialog
    :open="privacyStore.executing"
    :title="title"
    :description="description"
    :icon-name="ICON_NAMES.shield"
    :progress="percent"
    :progress-label="stageLabel"
    :stats="stats"
    cancelable
    :cancel-disabled="privacyStore.cancellingExecution"
    :cancel-label="
      privacyStore.cancellingExecution ? t('loading.cancellingCleanupAction') : t('loading.cancelCleanupAction')
    "
    @cancel="requestCancellation"
  >
    <div
      ref="itemListElement"
      class="privacy-operation-list scrollbar-stable"
      :aria-label="t('loading.cleanupItemList')"
    >
      <div v-for="item in items" :key="item.token" class="privacy-operation-item" :class="`is-${item.state}`">
        <span class="privacy-operation-item-status" aria-hidden="true">
          <MdIcon v-if="item.state === 'cleared' || item.state === 'unchanged'" :name="ICON_NAMES.check" :size="14" />
          <MdIcon
            v-else-if="item.state === 'failed' || item.state === 'cancelled'"
            :name="ICON_NAMES.info"
            :size="14"
          />
          <i v-else-if="item.state === 'active'" class="md-operational-motion" />
          <i v-else />
        </span>
        <span class="privacy-operation-item-content">
          <strong>{{ item.sourceName }}</strong>
          <small>{{ t(`privacy.kinds.${item.kind}`) }}</small>
        </span>
        <small class="privacy-operation-item-label">
          {{ itemStatusLabel(item.state) }}
        </small>
      </div>
    </div>
  </MdOperationDialog>

  <MdConfirmDialog
    v-model:open="cancellationConfirmOpen"
    :title="t('loading.cancelCleanupConfirmTitle')"
    :description="t('loading.cancelCleanupConfirmDescription')"
    :cancel-label="t('common.cancel')"
    :confirm-label="t('loading.stopCleanupAction')"
    confirm-variant="destructive"
    @confirm="confirmCancellation"
  />
</template>

<style scoped>
@reference "@assets/main.css";
.privacy-operation-list {
  min-height: 0;
  max-height: 230px;
  overscroll-behavior: contain;
  overflow: auto;
  border-width: 1px;
  border-radius: 12px;
  @apply border-border;
}
.privacy-operation-item {
  display: grid;
  grid-template-columns: 26px minmax(0, 1fr) auto;
  align-items: center;
  gap: 11px;
  min-height: 66px;
  padding: 10px 15px;
  border-bottom-width: 1px;
  @apply border-border;
}
.privacy-operation-item:last-child {
  border-bottom: 0;
}
.privacy-operation-item.is-active {
  background: var(--surface-primary-subtle);
}
.privacy-operation-item-status {
  display: grid;
  width: 22px;
  height: 22px;
  place-items: center;
  border-width: 1px;
  border-radius: 999px;
  @apply border-border text-muted-foreground;
}
.privacy-operation-item.is-cleared .privacy-operation-item-status,
.privacy-operation-item.is-unchanged .privacy-operation-item-status {
  @apply border-primary/25 text-primary;
  background: var(--surface-primary-subtle);
}
.privacy-operation-item.is-failed .privacy-operation-item-status,
.privacy-operation-item.is-cancelled .privacy-operation-item-status {
  @apply border-destructive/25 text-destructive;
  background: var(--surface-destructive-subtle);
}
.privacy-operation-item-status i {
  width: 8px;
  height: 8px;
  border-radius: 999px;
  @apply bg-muted-foreground/35;
}
.privacy-operation-item-status i.md-operational-motion {
  width: 13px;
  height: 13px;
  border-width: 2px;
  @apply border-primary/20 border-t-primary bg-transparent;
  animation: privacy-operation-spin 0.8s linear infinite;
}
.privacy-operation-item-content {
  display: flex;
  min-width: 0;
  flex-direction: column;
  gap: 4px;
}
.privacy-operation-item-content strong,
.privacy-operation-item-content small {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.privacy-operation-item-content strong {
  font-size: 13px;
}
.privacy-operation-item-content small,
.privacy-operation-item-label {
  @apply text-muted-foreground;
  font-size: 11px;
}
@keyframes privacy-operation-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>

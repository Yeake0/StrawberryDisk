<script setup lang="ts">
import MdTooltip from '@/components/custom/md-tooltip.vue';
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';

import MdConfirmDialog from '@/components/custom/md-confirm-dialog.vue';
import MdMiddleEllipsis from '@/components/custom/md-middle-ellipsis.vue';
import MdOperationDialog from '@/components/custom/md-operation-dialog.vue';
import MdIcon from '@/components/icons/md-icon.vue';
import { CLEANUP_OPERATION_IDS } from '@/lib/models/cleanup';
import { ICON_NAMES } from '@/lib/models/ui';
import { ByteSizeService } from '@/lib/services/byte-size-service';
import * as CleanupRuleTextUtils from '@/lib/utils/cleanup-rule-text';
import * as FormatUtils from '@/lib/utils/format';
import * as PathUtils from '@/lib/utils/path';
import { useApplicationStore } from '@/stores/application-store';
import { useCleanupStore } from '@/stores/cleanup-store';

import { isWaitingForPreviousWindowsInstallationCleanup } from './cleanup-operation-presentation';

interface CleanupRuleSummary {
  fileCount: number;
  name: string;
  ruleId: string;
}

const props = defineProps<{
  cancelling: boolean;
  rules: CleanupRuleSummary[];
}>();
const emit = defineEmits<{ cancel: [] }>();
const { t } = useI18n({ useScope: 'global' });
const cleanupStore = useCleanupStore();
const applicationStore = useApplicationStore();
const cancellationConfirmOpen = ref(false);
const loadingClockMs = ref(Date.now());
const executionListElement = ref<HTMLElement | null>(null);
let loadingClockTimer: ReturnType<typeof setInterval> | null = null;

const cleanupScanning = computed(
  () =>
    cleanupStore.loading &&
    (cleanupStore.operation === CLEANUP_OPERATION_IDS.scanning ||
      cleanupStore.operation === CLEANUP_OPERATION_IDS.cancelling)
);
const visible = computed(() => (cleanupStore.loading && !cleanupScanning.value) || applicationStore.deletingLeftovers);
const executionActive = computed(
  () =>
    cleanupStore.loading &&
    (cleanupStore.operation === CLEANUP_OPERATION_IDS.cleaning ||
      cleanupStore.operation === CLEANUP_OPERATION_IDS.previewing)
);
const destructiveActive = computed(
  () =>
    (cleanupStore.loading && cleanupStore.operation === CLEANUP_OPERATION_IDS.cleaning) ||
    applicationStore.deletingLeftovers
);
const progress = computed(() => cleanupStore.executionProgress);
const waitingForPreviousWindowsInstallationCleanup = computed(() =>
  isWaitingForPreviousWindowsInstallationCleanup(cleanupStore.operation, progress.value)
);

watch(destructiveActive, active => {
  if (!active) cancellationConfirmOpen.value = false;
});
watch(
  () => progress.value?.currentRuleId,
  async currentRuleId => {
    if (!currentRuleId) return;
    await nextTick();
    executionListElement.value
      ?.querySelector<HTMLElement>('.cleanup-execution-item.is-active')
      ?.scrollIntoView({ block: 'nearest' });
  }
);

const loadingMessage = computed(() => {
  if (props.cancelling) return t('loading.cancellingCleanup');
  if (applicationStore.deletingLeftovers) return t('loading.cleaningApplicationLeftovers');
  if (cleanupStore.operation === CLEANUP_OPERATION_IDS.previewing) return t('loading.previewing');
  if (cleanupStore.operation === CLEANUP_OPERATION_IDS.cleaning) return t('loading.cleaning');
  return t('loading.starting');
});
const loadingHint = computed(() => {
  if (props.cancelling) return t('loading.cancellingCleanupHint');
  if (waitingForPreviousWindowsInstallationCleanup.value) {
    return t('loading.windowsPreviousInstallationsCleanupHint');
  }
  return cleanupStore.operation === CLEANUP_OPERATION_IDS.previewing
    ? t('loading.previewingSafetyHint')
    : t('loading.cleaningSafetyHint');
});
const elapsedMs = computed(() => {
  const reported = progress.value?.elapsedMs ?? 0;
  const startedAt = cleanupStore.executionStartedAtMs;
  const live = startedAt === null ? 0 : Math.max(0, loadingClockMs.value - startedAt);
  return Math.max(reported, live);
});
const elapsedSeconds = computed(() => Math.floor(elapsedMs.value / 1000));
const items = computed(() => {
  const currentProgress = progress.value;
  const completed = new Map(currentProgress?.completedRuleResults.map(result => [result.ruleId, result]) ?? []);
  const ruleIds = cleanupStore.executionRuleIds.length ? cleanupStore.executionRuleIds : cleanupStore.selectedRuleIds;
  return ruleIds.map(ruleId => {
    const rule = props.rules.find(item => item.ruleId === ruleId);
    const result = completed.get(ruleId);
    const active = !result && currentProgress?.currentRuleId === ruleId;
    const detailIsPath = Boolean(active && currentProgress?.currentItemPath);
    let detail = t('loading.cleanupItemWaiting');
    if (result?.status === 'previewed') {
      detail = t('loading.cleanupItemChecked');
    } else if (result?.status === 'partial') {
      detail = t('loading.cleanupItemPartial', {
        count: FormatUtils.integer(result.affectedItemCount),
        size: ByteSizeService.bytes(result.releasedBytes),
      });
    } else if (result && ['blocked', 'failed'].includes(result.status)) {
      detail = t('loading.cleanupItemSkipped');
    } else if (result) {
      detail = t('loading.cleanupItemCompleted', {
        count: FormatUtils.integer(result.affectedItemCount),
        size: ByteSizeService.bytes(result.releasedBytes),
      });
    } else if (active && currentProgress?.stage === 'validating') {
      detail = t('loading.cleanupItemValidating');
    } else if (active && currentProgress?.currentItemPath) {
      detail = PathUtils.display(currentProgress.currentItemPath);
    } else if (active && waitingForPreviousWindowsInstallationCleanup.value) {
      detail = t('loading.windowsPreviousInstallationsCleanupWaiting');
    } else if (active) {
      detail = t('loading.cleanupItemSystemProcessing');
    }
    return {
      detail,
      detailIsPath,
      name: rule?.name ?? CleanupRuleTextUtils.fallbackName(ruleId),
      ruleId,
      state: result
        ? ['completed', 'previewed'].includes(result.status)
          ? 'completed'
          : 'skipped'
        : active
          ? 'active'
          : 'pending',
    };
  });
});
const stageLabel = computed(() => {
  switch (progress.value?.stage) {
    case 'validating':
      return t('loading.validating');
    case 'finalizing':
      return t('loading.finalizing');
    default:
      return cleanupStore.operation === CLEANUP_OPERATION_IDS.previewing
        ? t('loading.previewing')
        : t('loading.cleaning');
  }
});
const ruleProgress = computed(() => {
  const currentProgress = progress.value;
  const total = Math.max(currentProgress?.totalRuleCount ?? cleanupStore.selectedRuleIds.length, 1);
  const completed =
    currentProgress?.stage === 'validating'
      ? (currentProgress.validatedRuleCount ?? 0)
      : currentProgress?.stage === 'finalizing'
        ? total
        : (currentProgress?.completedRuleCount ?? 0);
  return { completed: Math.min(completed, total), total };
});
const activeRuleFraction = computed(() => {
  const currentProgress = progress.value;
  if (!currentProgress?.currentRuleId || currentProgress.stage !== 'cleaning') return 0;
  const activeRule = props.rules.find(rule => rule.ruleId === currentProgress.currentRuleId);
  return activeRule?.fileCount
    ? Math.min(0.95, currentProgress.currentRuleAffectedItemCount / activeRule.fileCount)
    : 0;
});
const percent = computed(() => {
  const currentProgress = progress.value;
  const { completed, total } = ruleProgress.value;
  if (!currentProgress) return 2;
  if (currentProgress.stage === 'validating') return 3 + (completed / total) * 22;
  if (currentProgress.stage === 'finalizing') return 98;
  // Whole-rule cleanup starts deletion without a measured preview. Reserve the validation segment
  // only when Core reports validated rules so the indicator does not jump at startup.
  const validationCompleted = currentProgress.validatedRuleCount > 0;
  const cleaningStart = validationCompleted ? 25 : 3;
  const cleaningRange = validationCompleted ? 70 : 92;
  return cleaningStart + ((completed + activeRuleFraction.value) / total) * cleaningRange;
});
const activeItem = computed(() => items.value.find(item => item.state === 'active'));
const title = computed(() => {
  if (props.cancelling) return loadingMessage.value;
  const activeItemName = activeItem.value?.name;
  if (!activeItemName) return stageLabel.value;
  return cleanupStore.operation === CLEANUP_OPERATION_IDS.previewing
    ? t('loading.previewingCurrentItem', { name: activeItemName })
    : t('loading.cleaningCurrentItem', { name: activeItemName });
});
const summary = computed(() =>
  t('loading.cleanupProgressSummary', {
    completed: FormatUtils.integer(ruleProgress.value.completed),
    total: FormatUtils.integer(ruleProgress.value.total),
  })
);
const primaryMetric = computed(() => {
  if (progress.value?.stage === 'validating') {
    return { label: t('loading.checkedItems'), value: FormatUtils.integer(progress.value.checkedItemCount) };
  }
  return { label: t('loading.processedItems'), value: FormatUtils.integer(progress.value?.affectedItemCount ?? 0) };
});
const secondaryMetric = computed(() => {
  if (progress.value?.stage === 'validating') {
    return { label: t('loading.checkedData'), value: ByteSizeService.bytes(progress.value.checkedBytes) };
  }
  return {
    label:
      cleanupStore.operation === CLEANUP_OPERATION_IDS.previewing ? t('cleanup.estimated') : t('loading.releasedSpace'),
    value: ByteSizeService.bytes(progress.value?.releasedBytes ?? 0),
  };
});

const stats = computed(() => [
  {
    key: 'rules',
    label: t('loading.ruleProgress'),
    value: t('loading.ruleProgressValue', {
      completed: FormatUtils.integer(ruleProgress.value.completed),
      total: FormatUtils.integer(ruleProgress.value.total),
    }),
  },
  {
    key: 'elapsed',
    label: t('loading.elapsed'),
    value: t('loading.elapsedSeconds', { count: FormatUtils.integer(elapsedSeconds.value) }, elapsedSeconds.value),
  },
  { key: 'items', ...primaryMetric.value },
  { key: 'bytes', ...secondaryMetric.value },
]);

function requestCancellation() {
  if (props.cancelling || !destructiveActive.value) return;
  cancellationConfirmOpen.value = true;
}

function confirmCancellation() {
  cancellationConfirmOpen.value = false;
  emit('cancel');
}

onMounted(() => {
  loadingClockTimer = window.setInterval(() => {
    if (executionActive.value) loadingClockMs.value = Date.now();
  }, 1000);
});
onBeforeUnmount(() => {
  if (loadingClockTimer) window.clearInterval(loadingClockTimer);
});
</script>

<template>
  <MdOperationDialog
    :open="visible"
    :title="executionActive ? title : loadingMessage"
    :description="executionActive && !cancelling ? summary : loadingHint"
    :icon-name="ICON_NAMES.deepCleanup"
    :size="executionActive ? 'large' : 'compact'"
    :progress="executionActive ? percent : undefined"
    :progress-label="stageLabel"
    :stats="executionActive ? stats : []"
    :cancelable="destructiveActive"
    :cancel-disabled="cancelling"
    :cancel-label="cancelling ? t('loading.cancellingCleanupAction') : t('loading.cancelCleanupAction')"
    @cancel="requestCancellation"
  >
    <div
      v-if="executionActive"
      ref="executionListElement"
      class="cleanup-execution-list scrollbar-stable"
      :aria-label="t('loading.cleanupItemList')"
    >
      <div v-for="item in items" :key="item.ruleId" class="cleanup-execution-item" :class="`is-${item.state}`">
        <span class="cleanup-execution-item-status" aria-hidden="true">
          <MdIcon v-if="item.state === 'completed'" :name="ICON_NAMES.check" :size="14" />
          <b v-else-if="item.state === 'skipped'">!</b>
          <i v-else-if="item.state === 'active'" class="md-operational-motion" />
          <i v-else />
        </span>
        <span class="cleanup-execution-item-content">
          <span class="cleanup-execution-item-title">
            <strong>{{ item.name }}</strong>
            <small v-if="item.state === 'active' && elapsedSeconds >= 20" class="cleanup-execution-item-slow-hint">
              {{ t('loading.stepMayTakeMinutes') }}
            </small>
          </span>
          <MdTooltip :text="item.detail"
            ><small class="cleanup-execution-item-detail">
              <MdMiddleEllipsis v-if="item.detailIsPath" :text="item.detail" :tail-length="40" :show-tooltip="false" />
              <template v-else>{{ item.detail }}</template>
            </small></MdTooltip
          >
        </span>
        <small class="cleanup-execution-item-label">
          {{
            item.state === 'completed'
              ? t('loading.cleanupItemDone')
              : item.state === 'skipped'
                ? t('loading.cleanupItemSkippedLabel')
                : item.state === 'active'
                  ? t('loading.cleanupItemActive')
                  : t('loading.cleanupItemPending')
          }}
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
.cleanup-execution-list {
  min-height: 0;
  max-height: 230px;
  overflow-y: auto;
  overscroll-behavior: contain;
  border-width: 1px;
  border-radius: 11px;
  @apply border-border/80 bg-muted/20;
}
.cleanup-execution-item {
  display: grid;
  min-width: 0;
  grid-template-columns: 24px minmax(0, 1fr) auto;
  align-items: center;
  gap: 9px;
  border-top-width: 1px;
  padding: 9px 11px;
  @apply border-border/60;
}
.cleanup-execution-item:first-child {
  border-top: 0;
}
.cleanup-execution-item.is-active {
  background: var(--surface-primary-subtle);
}
.cleanup-execution-item-status {
  display: grid;
  width: 22px;
  height: 22px;
  place-items: center;
  border-radius: 50%;
  @apply bg-muted text-muted-foreground;
}
.cleanup-execution-item.is-completed .cleanup-execution-item-status {
  @apply text-success;
  background: var(--surface-success-subtle);
}
.cleanup-execution-item.is-skipped .cleanup-execution-item-status {
  @apply text-warning-foreground;
  background: var(--surface-warning-subtle);
}
.cleanup-execution-item.is-active .cleanup-execution-item-status {
  @apply text-primary;
  background: var(--surface-primary-subtle);
}
.cleanup-execution-item-status i {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  @apply bg-muted-foreground/45;
}
.cleanup-execution-item-status b {
  font-size: 12px;
  line-height: 1;
}
.cleanup-execution-item.is-active .cleanup-execution-item-status i {
  width: 13px;
  height: 13px;
  border-width: 2px;
  @apply border-primary/20 border-t-primary bg-transparent;
  animation: cleanup-icon-spin 0.8s linear infinite;
}
.cleanup-execution-item-content {
  display: flex;
  min-width: 0;
  flex-direction: column;
}
.cleanup-execution-item-title {
  display: flex;
  min-width: 0;
  align-items: baseline;
  gap: 8px;
}
.cleanup-execution-item-title strong {
  overflow: hidden;
  min-width: 0;
  font-size: 12.5px;
  line-height: 1.4;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.cleanup-execution-item-slow-hint {
  overflow: hidden;
  min-width: 0;
  max-width: 42%;
  flex: none;
  @apply text-muted-foreground;
  font-size: 9.5px;
  line-height: 1.4;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.cleanup-execution-item-detail {
  overflow: hidden;
  margin-top: 1px;
  @apply text-muted-foreground;
  font-size: 10.5px;
  line-height: 1.4;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.cleanup-execution-item-label {
  @apply text-muted-foreground;
  font-size: 10.5px;
  white-space: nowrap;
}
@keyframes cleanup-icon-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>

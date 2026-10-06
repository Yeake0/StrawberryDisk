<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';

import MdDialogContent from '@/components/custom/md-dialog-content.vue';
import MdDialogFooter from '@/components/custom/md-dialog-footer.vue';
import MdDialogHeader from '@/components/custom/md-dialog-header.vue';
import MdTooltip from '@/components/custom/md-tooltip.vue';
import MdSwitch from '@/components/custom/md-switch.vue';
import MdIcon from '@/components/icons/md-icon.vue';
import { Button } from '@/components/ui/button';
import { Dialog, DialogDescription, DialogTitle } from '@/components/ui/dialog';
import type { CleanupReadFailureDetail } from '@/lib/models/cleanup';
import { ICON_NAMES } from '@/lib/models/ui';
import { ClipboardService } from '@/lib/services/clipboard-service';

const props = withDefaults(
  defineProps<{
    modelValue: boolean;
    failureCount: number;
    failureDetails?: CleanupReadFailureDetail[];
    retryDisabled: boolean;
    hideReadFailureAlerts?: boolean;
  }>(),
  { failureDetails: () => [], hideReadFailureAlerts: false }
);
const emit = defineEmits<{
  'update:modelValue': [value: boolean];
  'update:hideReadFailureAlerts': [value: boolean];
  openLogs: [];
  retry: [];
  error: [error: unknown];
}>();
const { t } = useI18n({ useScope: 'global' });
const FAILURE_REASON_LABELS: Record<CleanupReadFailureDetail['reason'], string> = {
  permissionDenied: 'cleanup.permission.reasons.permissionDenied',
  ioError: 'cleanup.permission.reasons.ioError',
};
const visibleDetails = computed(() => props.failureDetails.slice(0, 50));
const copiedPath = ref<string | null>(null);
let copyFeedbackTimer: ReturnType<typeof setTimeout> | undefined;
let disposed = false;
function resetCopyFeedback() {
  clearTimeout(copyFeedbackTimer);
  copyFeedbackTimer = undefined;
  copiedPath.value = null;
}
watch(() => props.modelValue, resetCopyFeedback);
onBeforeUnmount(() => {
  disposed = true;
  resetCopyFeedback();
});
function requestRetry() {
  emit('update:modelValue', false);
  emit('retry');
}
async function copyPath(path: string) {
  try {
    await ClipboardService.writeText(path);
    if (disposed || !props.modelValue) return;
    resetCopyFeedback();
    copiedPath.value = path;
    copyFeedbackTimer = setTimeout(resetCopyFeedback, 1000);
  } catch (error) {
    if (!disposed) emit('error', error);
  }
}
function displayPath(path: string): string {
  // Keep control characters visible without changing the value copied to the clipboard.
  return Array.from(path, character => {
    const code = character.charCodeAt(0);
    return code <= 31 || code === 127 ? `\\u${code.toString(16).padStart(4, '0')}` : character;
  }).join('');
}
</script>

<template>
  <button
    type="button"
    class="scan-warning-trigger"
    aria-haspopup="dialog"
    :aria-expanded="modelValue"
    @click="emit('update:modelValue', true)"
  >
    <span>{{ t('cleanup.permission.otherWarning') }}</span>
    <MdIcon :name="ICON_NAMES.info" :size="13" aria-hidden="true" />
  </button>

  <Dialog :open="modelValue" @update:open="emit('update:modelValue', $event)">
    <MdDialogContent size="standard">
      <MdDialogHeader class="scan-failure-header">
        <DialogTitle class="text-base leading-snug">{{ t('cleanup.permission.otherTitle') }}</DialogTitle>
        <DialogDescription>
          {{ t('cleanup.permission.otherDescription', { count: failureCount }) }}
        </DialogDescription>
      </MdDialogHeader>
      <div class="scan-failure-body">
        <p class="text-content-body text-muted-foreground">{{ t('cleanup.permission.otherInstructions') }}</p>
        <template v-if="visibleDetails.length">
          <p v-if="visibleDetails.length < failureCount" class="text-content-secondary text-muted-foreground">
            {{ t('cleanup.permission.detailsShown', { count: visibleDetails.length, total: failureCount }) }}
          </p>
          <ul class="scan-failure-list scrollbar-stable" tabindex="0" :aria-label="t('cleanup.permission.otherTitle')">
            <li v-for="(detail, index) in visibleDetails" :key="index" class="scan-failure-row">
              <div class="scan-failure-path-row">
                <span class="scan-failure-path" dir="auto">{{ displayPath(detail.path) }}</span>
                <MdTooltip :text="t(copiedPath === detail.path ? 'common.copied' : 'common.copy')">
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon"
                    class="size-7 shrink-0"
                    :aria-label="t('cleanup.permission.copyPath', { path: detail.path })"
                    @click="copyPath(detail.path)"
                  >
                    <MdIcon :name="copiedPath === detail.path ? ICON_NAMES.check : ICON_NAMES.copy" :size="14" />
                  </Button>
                </MdTooltip>
              </div>
              <details class="scan-failure-error">
                <summary>{{ t(FAILURE_REASON_LABELS[detail.reason]) }}</summary>
                <p class="scan-failure-native-error">{{ detail.error }}</p>
              </details>
            </li>
          </ul>
        </template>
        <p v-else class="text-content-body text-muted-foreground">{{ t('cleanup.permission.detailsUnavailable') }}</p>
      </div>
      <MdDialogFooter align="between" class="flex-row flex-wrap">
        <div class="scan-failure-ignore flex min-w-0 flex-1 items-center">
          <MdTooltip :text="t('cleanup.permission.ignoreWarningsHint')">
            <div class="inline-flex min-w-0 max-w-full items-center gap-2">
              <MdSwitch
                id="cleanup-hide-read-failure-alerts"
                :model-value="hideReadFailureAlerts"
                :aria-label="t('cleanup.permission.ignoreWarnings')"
                aria-describedby="cleanup-hide-read-failure-alerts-hint"
                @update:model-value="emit('update:hideReadFailureAlerts', $event)"
              />
              <label
                for="cleanup-hide-read-failure-alerts"
                class="min-w-0 text-content-secondary text-muted-foreground [overflow-wrap:anywhere]"
              >
                {{ t('cleanup.permission.ignoreWarnings') }}
              </label>
              <span id="cleanup-hide-read-failure-alerts-hint" class="sr-only">
                {{ t('cleanup.permission.ignoreWarningsHint') }}
              </span>
            </div>
          </MdTooltip>
        </div>
        <div class="scan-failure-actions flex shrink-0 items-center gap-2">
          <Button type="button" variant="ghost" size="sm" @click="emit('openLogs')">
            {{ t('settings.feedbackDialog.openLogFolder') }}
          </Button>
          <Button type="button" variant="outline" size="sm" :disabled="retryDisabled" @click="requestRetry">
            {{ t('overview.rescan') }}
          </Button>
        </div>
      </MdDialogFooter>
    </MdDialogContent>
  </Dialog>
</template>

<style scoped>
@reference "@assets/main.css";

.scan-warning-trigger {
  display: inline-flex;
  min-width: 0;
  max-width: min(440px, 46vw);
  align-items: center;
  gap: 5px;
  border: 0;
  padding: 4px 0;
  background: transparent;
  color: var(--muted-foreground);
  font: inherit;
  font-size: 12px;
  cursor: pointer;
}

.scan-warning-trigger span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.scan-warning-trigger :deep(svg) {
  flex: none;
}

.scan-warning-trigger:hover {
  color: var(--foreground);
  text-decoration: underline;
}

.scan-warning-trigger:focus-visible {
  border-radius: 3px;
  @apply outline-none ring-2 ring-ring;
}

.scan-failure-header {
  padding-inline-start: var(--layout-dialog-body-inline-padding);
}

.scan-failure-body {
  display: flex;
  min-height: 0;
  flex-direction: column;
  gap: 10px;
  padding: 0 var(--layout-dialog-body-inline-padding) 16px;
}

.scan-failure-list {
  max-height: min(300px, 40vh);
  overflow-y: auto;
  overscroll-behavior: contain;
  @apply divide-y divide-border/70;
}

.scan-failure-list:focus-visible {
  border-radius: 3px;
  @apply outline-none ring-2 ring-ring;
}

.scan-failure-row {
  padding-block: 9px;
}

.scan-failure-path-row {
  display: flex;
  align-items: flex-start;
  gap: 8px;
}

.scan-failure-path {
  min-width: 0;
  flex: 1;
  padding-top: 3px;
  overflow-wrap: anywhere;
  @apply text-content-body text-foreground;
}

.scan-failure-error {
  margin-top: 3px;
  @apply text-content-secondary text-muted-foreground;
}

.scan-failure-error summary {
  width: fit-content;
  cursor: pointer;
}

.scan-failure-error summary:focus-visible {
  @apply outline-none ring-2 ring-ring;
}

.scan-failure-native-error {
  padding-block: 6px 2px;
  overflow-wrap: anywhere;
  white-space: pre-wrap;
}
</style>

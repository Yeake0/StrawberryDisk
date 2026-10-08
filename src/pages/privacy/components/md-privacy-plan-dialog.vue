<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';

import MdApplicationClosePanel from '@/components/custom/md-application-close-panel.vue';
import MdApplicationIcon from '@/components/custom/md-application-icon.vue';
import MdConfirmationItemList from '@/components/custom/md-confirmation-item-list.vue';
import MdDialogContent from '@/components/custom/md-dialog-content.vue';
import MdDialogFooter from '@/components/custom/md-dialog-footer.vue';
import MdDialogHeader from '@/components/custom/md-dialog-header.vue';
import { Button } from '@/components/ui/button';
import MdCheckbox from '@/components/custom/md-checkbox.vue';
import { Dialog, DialogDescription, DialogTitle } from '@/components/ui/dialog';
import type {
  ApplicationCloseBatchResult,
  ApplicationCloseItem,
  ApplicationCloseMode,
} from '@/lib/models/application-close';
import type { PrivacyBrowserStatusResult, PrivacyDataKind, PrivacyExecutionPlan } from '@/lib/models/privacy';
import * as FormatUtils from '@/lib/utils/format';

import {
  privacyBrowserCloseItems,
  privacyBrowserCloseRetry,
  privacyBrowserStatusRetry,
} from '../privacy-browser-close';

const FORCE_STATUS_REFRESH_INTERVAL_MS = 1_000;

const { t } = useI18n({ useScope: 'global' });

const props = defineProps<{
  modelValue: boolean;
  plan: PrivacyExecutionPlan;
  busy: boolean;
  closingBrowsers: boolean;
  closeResult: ApplicationCloseBatchResult | null;
  browserStatusResult: PrivacyBrowserStatusResult | null;
  sourceIconPaths?: Readonly<Record<string, string>>;
  sourceIconUrlsByName?: Readonly<Record<string, string>>;
}>();
const emit = defineEmits<{
  closeBrowsers: [sourceIds: string[], mode: ApplicationCloseMode];
  refreshBrowserStatus: [sourceIds: string[]];
  continue: [excludedSourceIds: string[]];
  execute: [];
  'update:modelValue': [open: boolean];
}>();

const closePhase = ref<'selection' | 'force'>('selection');
const selectedCloseSourceIds = ref<string[]>([]);
const remainingSourceIds = ref<string[]>([]);
const remainingCloseItems = ref<ApplicationCloseItem[]>([]);
const riskAccepted = ref(false);
let statusRefreshTimer: ReturnType<typeof setInterval> | null = null;

const closeItems = computed(() => privacyBrowserCloseItems(props.plan.browserCloseRequirements, props.sourceIconPaths));
const allCloseSourceIds = computed(() => props.plan.browserCloseRequirements.map(item => item.sourceId));
const selectedCloseRequirements = computed(() => {
  const selected = new Set(selectedCloseSourceIds.value);
  return props.plan.browserCloseRequirements.filter(requirement => selected.has(requirement.sourceId));
});
const unselectedCloseSourceIds = computed(() => {
  const selected = new Set(selectedCloseSourceIds.value);
  return allCloseSourceIds.value.filter(sourceId => !selected.has(sourceId));
});
const highImpactSelected = computed(() =>
  props.plan.items.some(item => ['signOut', 'crossDevice', 'dataLoss', 'unknown'].includes(item.impact))
);
const interactionBusy = computed(() => props.busy || props.closingBrowsers);
const selectedTraceCount = computed(() => props.plan.items.reduce((total, item) => total + item.itemCount, 0));
const planSourceGroups = computed(() => {
  const sources = new Map<
    string,
    {
      sourceName: string;
      profileName: string | null;
      itemCount: number;
      traceCount: number;
      kinds: Map<PrivacyDataKind, { count: number; impact: string }>;
    }
  >();
  for (const item of props.plan.items) {
    // The same cleanup kind can belong to several applications. Keep the
    // confirmation boundary visible so users never approve an anonymous sum.
    const sourceKey = `${item.sourceId}\u0000${item.profileName ?? ''}`;
    let source = sources.get(sourceKey);
    if (!source) {
      source = {
        sourceName: item.sourceName,
        profileName: item.profileName,
        itemCount: 0,
        traceCount: 0,
        kinds: new Map(),
      };
      sources.set(sourceKey, source);
    }
    source.itemCount += 1;
    source.traceCount += item.itemCount;
    const existing = source.kinds.get(item.kind);
    if (existing) existing.count += item.itemCount;
    else source.kinds.set(item.kind, { count: item.itemCount, impact: item.impact });
  }
  return [...sources.entries()]
    .sort(
      ([, left], [, right]) => right.traceCount - left.traceCount || left.sourceName.localeCompare(right.sourceName)
    )
    .map(([key, source]) => ({
      key,
      sourceName: source.sourceName,
      profileName: source.profileName,
      iconUrl: props.sourceIconUrlsByName?.[source.sourceName],
      itemCount: source.itemCount,
      traceCount: source.traceCount,
      items: [...source.kinds.entries()]
        .sort((left, right) => right[1].count - left[1].count)
        .map(([kind, value]) => ({
          key: kind,
          title: t(`privacy.kinds.${kind}`),
          description: value.impact === 'low' ? undefined : t(`privacy.impacts.${value.impact}`),
          badge: value.impact === 'low' ? t('common.safe') : undefined,
          badgeTone: value.impact === 'low' ? ('positive' as const) : undefined,
          value: t('privacy.traceCount', { count: FormatUtils.integer(value.count) }, value.count),
        })),
    }));
});
const usesScrollableLayout = computed(
  () => props.plan.items.length > 6 || closeItems.value.length > 0 || closePhase.value === 'force'
);

watch(
  [() => props.modelValue, () => props.plan.planId],
  ([open]) => {
    if (!open) return;
    closePhase.value = 'selection';
    selectedCloseSourceIds.value = [];
    remainingSourceIds.value = [];
    remainingCloseItems.value = [];
    riskAccepted.value = false;
  },
  { immediate: true }
);

watch(
  () => props.closeResult,
  result => {
    if (!props.modelValue || !result) return;
    const retry = privacyBrowserCloseRetry(selectedCloseRequirements.value, result, props.sourceIconPaths);
    remainingSourceIds.value = retry.sourceIds;
    remainingCloseItems.value = retry.items;
    if (!remainingSourceIds.value.length) {
      emit('continue', unselectedCloseSourceIds.value);
      return;
    }
    closePhase.value = 'force';
  }
);

watch(
  () => props.browserStatusResult,
  result => {
    if (!props.modelValue || closePhase.value !== 'force' || !result) return;
    const remaining = new Set(remainingSourceIds.value);
    const requirements = selectedCloseRequirements.value.filter(requirement => remaining.has(requirement.sourceId));
    const retry = privacyBrowserStatusRetry(requirements, result, props.sourceIconPaths);
    remainingSourceIds.value = retry.sourceIds;
    remainingCloseItems.value = retry.items;
    if (!remainingSourceIds.value.length) emit('continue', unselectedCloseSourceIds.value);
  }
);

watch(
  [() => props.modelValue, closePhase],
  ([open, phase]) => {
    stopStatusRefresh();
    if (!open || phase !== 'force') return;
    // The initial graceful result is already current. Start one second later
    // and keep at most one native status request active through `interactionBusy`.
    statusRefreshTimer = setInterval(() => {
      if (!interactionBusy.value && remainingSourceIds.value.length) {
        emit('refreshBrowserStatus', [...remainingSourceIds.value]);
      }
    }, FORCE_STATUS_REFRESH_INTERVAL_MS);
  },
  { flush: 'post' }
);

onBeforeUnmount(stopStatusRefresh);

function stopStatusRefresh() {
  if (statusRefreshTimer === null) return;
  clearInterval(statusRefreshTimer);
  statusRefreshTimer = null;
}

function closeBrowsers(mode: ApplicationCloseMode) {
  const sourceIds = mode === 'force' ? remainingSourceIds.value : selectedCloseSourceIds.value;
  if (interactionBusy.value || !sourceIds.length) return;
  emit('closeBrowsers', sourceIds, mode);
}

function executeSelection() {
  if (highImpactSelected.value && !riskAccepted.value) return;
  if (!allCloseSourceIds.value.length) {
    emit('execute');
    return;
  }
  if (!selectedCloseSourceIds.value.length) {
    emit('continue', allCloseSourceIds.value);
    return;
  }
  closeBrowsers('graceful');
}

function skipRemainingBrowsers() {
  // A graceful batch can partially succeed. Preserve the cleanup selection
  // for browsers that are already closed, and exclude only sources the user
  // never selected plus the exact targets that Core still reports as active.
  emit('continue', [...new Set([...unselectedCloseSourceIds.value, ...remainingSourceIds.value])]);
}

function preventOutsideDismiss(event: Event) {
  event.preventDefault();
}
</script>

<template>
  <Dialog :open="modelValue" @update:open="emit('update:modelValue', $event)">
    <MdDialogContent
      class="privacy-plan-dialog"
      :height="usesScrollableLayout ? 'tall' : 'auto'"
      size="wide"
      @interact-outside="preventOutsideDismiss"
    >
      <MdDialogHeader>
        <DialogTitle>{{ t('privacy.confirmation.title') }}</DialogTitle>
        <DialogDescription class="plan-summary">
          <span>{{ t('common.itemCount', { count: FormatUtils.integer(plan.items.length) }, plan.items.length) }}</span>
          <span aria-hidden="true">·</span>
          <strong>
            {{ t('privacy.traceCount', { count: FormatUtils.integer(selectedTraceCount) }, selectedTraceCount) }}
          </strong>
        </DialogDescription>
      </MdDialogHeader>

      <div class="plan-dialog-body scrollbar-stable">
        <template v-if="closePhase === 'selection'">
          <p v-if="closeItems.length" class="process-warning">{{ t('privacy.confirmation.closeBrowsers') }}</p>
          <div v-if="closeItems.length" class="application-close-container">
            <MdApplicationClosePanel
              v-model:selected-ids="selectedCloseSourceIds"
              :items="closeItems"
              :disabled="interactionBusy"
            />
          </div>
        </template>
        <div v-else class="application-close-container">
          <p class="force-close-warning">
            <strong>{{ t('applicationClose.normalCloseFailed') }}</strong>
            <span>{{ t('applicationClose.forceWarning') }}</span>
          </p>
          <MdApplicationClosePanel :items="remainingCloseItems" :selectable="false" />
        </div>

        <div class="plan-source-groups">
          <section v-for="group in planSourceGroups" :key="group.key" class="plan-source-group">
            <header class="plan-source-header">
              <MdApplicationIcon :src="group.iconUrl" :size="30" :artwork-size="26" />
              <span class="plan-source-copy">
                <strong>{{ group.sourceName }}</strong>
                <small v-if="group.profileName">{{ group.profileName }}</small>
              </span>
              <span class="plan-source-summary">
                <span>{{
                  t('common.itemCount', { count: FormatUtils.integer(group.itemCount) }, group.itemCount)
                }}</span>
                <span aria-hidden="true">·</span>
                <strong>
                  {{ t('privacy.traceCount', { count: FormatUtils.integer(group.traceCount) }, group.traceCount) }}
                </strong>
              </span>
            </header>
            <MdConfirmationItemList :items="group.items" />
          </section>
        </div>
      </div>

      <MdDialogFooter v-if="closePhase === 'selection'" :align="highImpactSelected ? 'between' : 'end'">
        <label v-if="highImpactSelected" class="risk-acceptance">
          <MdCheckbox v-model="riskAccepted" :disabled="interactionBusy" />
          <span>{{ t('privacy.confirmation.riskAcceptance') }}</span>
        </label>
        <div class="footer-actions">
          <Button variant="outline" type="button" :disabled="interactionBusy" @click="emit('update:modelValue', false)">
            {{ t('common.cancel') }}
          </Button>
          <Button
            type="button"
            :disabled="interactionBusy || (highImpactSelected && !riskAccepted)"
            @click="executeSelection"
          >
            {{
              closingBrowsers
                ? t('applicationClose.closing')
                : selectedCloseSourceIds.length
                  ? t(
                      'applicationClose.closeSelectedAndContinue',
                      { count: FormatUtils.integer(selectedCloseSourceIds.length) },
                      selectedCloseSourceIds.length
                    )
                  : closeItems.length
                    ? t('privacy.confirmation.skipRunningAndContinue')
                    : t('privacy.confirmation.confirm')
            }}
          </Button>
        </div>
      </MdDialogFooter>
      <MdDialogFooter v-else>
        <Button type="button" variant="outline" :disabled="interactionBusy" @click="skipRemainingBrowsers">
          {{ t('applicationClose.skipAndContinue') }}
        </Button>
        <Button type="button" variant="destructive" :disabled="interactionBusy" @click="closeBrowsers('force')">
          {{ closingBrowsers ? t('applicationClose.closing') : t('applicationClose.forceAndContinue') }}
        </Button>
      </MdDialogFooter>
    </MdDialogContent>
  </Dialog>
</template>

<style scoped>
@reference "@assets/main.css";

:global(.privacy-plan-dialog) {
  display: grid;
  min-height: 0;
  grid-template-rows: auto minmax(0, 1fr) auto;
}

.plan-summary {
  display: flex;
  align-items: baseline;
  gap: 6px;
  font-size: var(--font-content-secondary);
}

.plan-summary strong {
  @apply text-primary;
  font-size: 17px;
  font-weight: 600;
}

.plan-dialog-body {
  min-width: 0;
  min-height: 0;
  overflow-y: auto;
  overscroll-behavior: contain;
}

.process-warning {
  margin: 7px 20px 0;
  border-radius: 7px;
  padding: 5px 9px;
  @apply text-warning-foreground;
  background: var(--surface-warning-subtle);
  font-size: var(--font-content-secondary);
}

.application-close-container {
  margin: 7px 20px 0;
}

.force-close-warning {
  display: flex;
  flex-direction: column;
  gap: 3px;
  margin: 0 0 8px;
  border-radius: 8px;
  padding: 8px 10px;
  @apply text-destructive;
  background: var(--surface-destructive-subtle);
  font-size: var(--font-content-secondary);
}

.plan-source-groups {
  display: grid;
  gap: 7px;
  margin: 7px 20px 9px;
}

.plan-source-group {
  overflow: hidden;
  border-width: 1px;
  border-radius: 9px;
  @apply border-border/70;
}

.plan-source-header {
  @apply bg-muted/45;
  display: grid;
  min-height: 52px;
  grid-template-columns: 30px minmax(0, 1fr) auto;
  align-items: center;
  gap: 9px;
  padding: 7px 12px;
}

.plan-source-copy {
  display: flex;
  min-width: 0;
  flex-direction: column;
  gap: 2px;
}

.plan-source-copy strong {
  overflow: hidden;
  font-size: 13px;
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.plan-source-copy small,
.plan-source-summary {
  @apply text-muted-foreground;
  font-size: 10.5px;
}

.plan-source-summary {
  display: flex;
  align-items: baseline;
  gap: 4px;
  white-space: nowrap;
}

.plan-source-summary strong {
  @apply text-foreground;
  font-size: 12px;
  font-weight: 600;
}

.plan-source-group :deep(.confirmation-item-list) {
  border: 0;
  border-top-width: 1px;
  border-radius: 0;
  @apply border-border/70;
}

.risk-acceptance {
  display: flex;
  min-width: 0;
  align-items: center;
  gap: 8px;
  font-size: var(--font-content-secondary);
}

.risk-acceptance span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.footer-actions {
  display: flex;
  flex: none;
  gap: 8px;
}
</style>

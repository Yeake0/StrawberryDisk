<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';

import MdDialogContent from '@/components/custom/md-dialog-content.vue';
import MdDialogFooter from '@/components/custom/md-dialog-footer.vue';
import MdDialogHeader from '@/components/custom/md-dialog-header.vue';
import MdMiddleEllipsis from '@/components/custom/md-middle-ellipsis.vue';
import MdSpinner from '@/components/custom/md-spinner.vue';
import { Button } from '@/components/ui/button';
import { Dialog, DialogDescription, DialogTitle } from '@/components/ui/dialog';
import type { PrivacyDetailEntry, PrivacyDetailsPage, PrivacyItem } from '@/lib/models/privacy';
import { PrivacyService } from '@/lib/services/privacy-service';
import * as FormatUtils from '@/lib/utils/format';

const props = defineProps<{
  modelValue: boolean;
  scanId: string;
  item: PrivacyItem | null;
}>();
const emit = defineEmits<{
  'update:modelValue': [open: boolean];
}>();
const { t } = useI18n({ useScope: 'global' });

const PAGE_SIZE = 100;
const page = ref<PrivacyDetailsPage | null>(null);
const entries = ref<PrivacyDetailEntry[]>([]);
const loading = ref(false);
const loadingMore = ref(false);
const failed = ref(false);
let loadRevision = 0;

const sourceDescription = computed(() => {
  if (!props.item) return '';
  return [props.item.sourceName, props.item.profileName].filter(Boolean).join(' · ');
});

async function load(reset: boolean) {
  if (!props.modelValue || !props.item) return;
  if (!reset && (loading.value || loadingMore.value)) return;
  const revision = ++loadRevision;
  const scanId = props.scanId;
  const token = props.item.token;
  if (reset) {
    page.value = null;
    entries.value = [];
    failed.value = false;
    loading.value = true;
    loadingMore.value = false;
  } else {
    loadingMore.value = true;
  }
  try {
    const result = await PrivacyService.details({
      scanId,
      token,
      offset: reset ? 0 : (page.value?.nextOffset ?? 0),
      limit: PAGE_SIZE,
    });
    if (revision !== loadRevision) return;
    page.value = result;
    entries.value = reset ? result.entries : [...entries.value, ...result.entries];
  } catch {
    if (revision === loadRevision) failed.value = true;
  } finally {
    if (revision === loadRevision) {
      loading.value = false;
      loadingMore.value = false;
    }
  }
}

function detailLabel(entry: PrivacyDetailEntry): string {
  return entry.label || t('privacy.details.unknown');
}

function clearDetails() {
  loadRevision += 1;
  page.value = null;
  entries.value = [];
  loading.value = false;
  loadingMore.value = false;
  failed.value = false;
}

function updateOpen(open: boolean) {
  if (!open) {
    clearDetails();
    emit('update:modelValue', false);
  }
}

watch(
  () => [props.modelValue, props.scanId, props.item?.token] as const,
  ([open]) => {
    if (open) {
      void load(true);
    } else {
      clearDetails();
    }
  },
  { immediate: true }
);
</script>

<template>
  <Dialog :open="modelValue && Boolean(item)" @update:open="updateOpen">
    <MdDialogContent class="privacy-detail-dialog flex min-h-0 flex-col" height="tall" size="large">
      <template v-if="item">
        <MdDialogHeader class="flex-none">
          <DialogTitle>{{ t('privacy.details.title', { item: t(`privacy.kinds.${item.kind}`) }) }}</DialogTitle>
          <DialogDescription>
            {{ sourceDescription }} ·
            {{ t('privacy.traceCount', { count: FormatUtils.integer(item.itemCount) }, item.itemCount) }}
          </DialogDescription>
        </MdDialogHeader>

        <div class="detail-dialog-body">
          <div v-if="loading" class="detail-state">
            <MdSpinner />
            <span>{{ t('privacy.details.loading') }}</span>
          </div>
          <div v-else-if="failed" class="detail-state">
            <span>{{ t('privacy.details.failed') }}</span>
            <Button variant="outline" size="sm" type="button" @click="load(true)">
              {{ t('privacy.details.retry') }}
            </Button>
          </div>
          <div v-else-if="page?.presentation === 'aggregateOnly'" class="detail-state">
            <span>{{ t('privacy.details.aggregateOnly') }}</span>
          </div>
          <div v-else-if="!entries.length" class="detail-state">
            <span>{{ t('privacy.details.empty') }}</span>
          </div>
          <div v-else class="detail-entry-list">
            <div v-for="(entry, index) in entries" :key="`${entry.label}:${index}`">
              <MdMiddleEllipsis class="detail-entry-label" :text="detailLabel(entry)" :tail-length="32" />
              <strong>
                {{ t('privacy.traceCount', { count: FormatUtils.integer(entry.itemCount) }, entry.itemCount) }}
              </strong>
            </div>
            <Button
              v-if="page?.nextOffset !== null"
              class="detail-load-more"
              variant="ghost"
              type="button"
              :disabled="loadingMore"
              @click="load(false)"
            >
              <MdSpinner v-if="loadingMore" size="small" />
              {{ t('privacy.details.loadMore') }}
            </Button>
          </div>
        </div>

        <MdDialogFooter>
          <Button variant="outline" type="button" @click="updateOpen(false)">
            {{ t('common.close') }}
          </Button>
        </MdDialogFooter>
      </template>
    </MdDialogContent>
  </Dialog>
</template>

<style scoped>
@reference "@assets/main.css";

.detail-dialog-body {
  display: flex;
  min-height: 0;
  flex: 1;
  flex-direction: column;
  padding: 12px var(--layout-dialog-body-inline-padding) 14px;
}

.detail-state {
  @apply text-muted-foreground;
  display: flex;
  min-height: 180px;
  flex: 1;
  align-items: center;
  justify-content: center;
  gap: 10px;
  font-size: 12px;
}

.detail-entry-list {
  @apply border border-border/70;
  min-height: 0;
  flex: 1;
  overflow-y: auto;
  border-radius: 9px;
}

.detail-entry-list > div {
  @apply border-t border-border/65;
  display: grid;
  min-height: 42px;
  grid-template-columns: minmax(0, 1fr) auto;
  align-items: center;
  gap: 18px;
  padding: 6px 12px;
}

.detail-entry-list > div:first-child {
  border-top: 0;
}

.detail-entry-label {
  min-width: 0;
  font-size: 12px;
}

.detail-entry-list strong {
  @apply text-muted-foreground;
  font-size: 11px;
  font-weight: 500;
  white-space: nowrap;
}

.detail-load-more {
  width: 100%;
  border-radius: 0;
}
</style>

<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';

import MdDialogContent from '@/components/custom/md-dialog-content.vue';
import MdDialogFooter from '@/components/custom/md-dialog-footer.vue';
import MdDialogHeader from '@/components/custom/md-dialog-header.vue';
import MdResultCheckbox from '@/components/custom/md-result-checkbox.vue';
import MdStatusBadge from '@/components/custom/md-status-badge.vue';
import MdIcon from '@/components/icons/md-icon.vue';
import { Button } from '@/components/ui/button';
import { Dialog, DialogDescription, DialogTitle } from '@/components/ui/dialog';
import type { DiskInfo } from '@/lib/models/disk';
import { ICON_NAMES } from '@/lib/models/ui';
import { ByteSizeService } from '@/lib/services/byte-size-service';
import { OperatingSystemService } from '@/lib/services/operating-system-service';
import * as FormatUtils from '@/lib/utils/format';
import * as PathUtils from '@/lib/utils/path';

const props = defineProps<{
  disks: DiskInfo[];
  initialMountPoints: string[];
  modelValue: boolean;
  systemDisk: DiskInfo | null;
}>();
const emit = defineEmits<{
  confirm: [mountPoints: string[]];
  'update:modelValue': [open: boolean];
}>();

const { t } = useI18n({ useScope: 'global' });
const isMacOs = OperatingSystemService.isMacOs();
const selectedMountPoints = ref<string[]>([]);
const selectedKeys = computed(() => new Set(selectedMountPoints.value.map(PathUtils.comparisonKey)));

function isSystemDisk(disk: DiskInfo): boolean {
  return Boolean(
    props.systemDisk &&
    PathUtils.comparisonKey(props.systemDisk.mountPoint) === PathUtils.comparisonKey(disk.mountPoint)
  );
}

const orderedDisks = computed(() =>
  [...props.disks].sort((left, right) => Number(isSystemDisk(right)) - Number(isSystemDisk(left)))
);
const systemDiskIsAvailable = computed(() => orderedDisks.value.some(isSystemDisk));
const selectedVolumeCount = computed(() => selectedMountPoints.value.length + Number(systemDiskIsAvailable.value));

function isSelected(disk: DiskInfo): boolean {
  return isSystemDisk(disk) || selectedKeys.value.has(PathUtils.comparisonKey(disk.mountPoint));
}

function resetSelection() {
  const availableByKey = new Map(props.disks.map(disk => [PathUtils.comparisonKey(disk.mountPoint), disk.mountPoint]));
  const systemDiskKey = props.systemDisk ? PathUtils.comparisonKey(props.systemDisk.mountPoint) : null;
  selectedMountPoints.value = [
    ...new Set(
      props.initialMountPoints
        .map(mountPoint => availableByKey.get(PathUtils.comparisonKey(mountPoint)))
        .filter((mountPoint): mountPoint is string => {
          if (!mountPoint) return false;
          return PathUtils.comparisonKey(mountPoint) !== systemDiskKey;
        })
    ),
  ];
}

function setSelected(disk: DiskInfo, selected: boolean) {
  if (isSystemDisk(disk)) return;
  const key = PathUtils.comparisonKey(disk.mountPoint);
  selectedMountPoints.value = selected
    ? [...selectedMountPoints.value.filter(value => PathUtils.comparisonKey(value) !== key), disk.mountPoint]
    : selectedMountPoints.value.filter(value => PathUtils.comparisonKey(value) !== key);
}

function confirm() {
  if (!selectedVolumeCount.value) return;
  emit('confirm', [...selectedMountPoints.value]);
  emit('update:modelValue', false);
}

watch(
  () => props.modelValue,
  open => {
    if (open) resetSelection();
  }
);
</script>

<template>
  <Dialog :open="modelValue" @update:open="emit('update:modelValue', $event)">
    <!-- Backdrop clicks must not discard the current disk selection. -->
    <MdDialogContent class="flex min-h-0 flex-col" size="large" @pointer-down-outside.prevent>
      <MdDialogHeader class="flex-none">
        <DialogTitle>{{ t('cleanup.scanMode.volumeDialogTitle') }}</DialogTitle>
        <DialogDescription>
          {{ t('cleanup.scanMode.volumeDialogDescription') }}
          <template v-if="isMacOs"> {{ t('systemStatus.diskCapacityHint') }}</template>
        </DialogDescription>
      </MdDialogHeader>

      <div class="volume-list scrollbar-stable">
        <div v-if="orderedDisks.length" class="volume-list-frame">
          <label
            v-for="(disk, index) in orderedDisks"
            :key="disk.mountPoint"
            class="volume-option"
            :class="{ 'volume-option--fixed': isSystemDisk(disk) }"
          >
            <MdResultCheckbox
              :id="`cleanup-volume-${index}`"
              :checked="isSelected(disk)"
              :disabled="isSystemDisk(disk)"
              :class="{ 'system-volume-checkbox': isSystemDisk(disk) }"
              @update:checked="setSelected(disk, $event)"
            />
            <span class="volume-icon" aria-hidden="true">
              <MdIcon :name="ICON_NAMES.hardDrive" :size="20" />
            </span>
            <span class="volume-copy">
              <span class="volume-name-row">
                <strong>{{ disk.name || disk.mountPoint }}</strong>
                <MdStatusBadge v-if="isSystemDisk(disk)" size="compact" tone="primary">
                  {{ t('cleanup.scanMode.systemVolume') }}
                </MdStatusBadge>
              </span>
              <small>{{ disk.mountPoint }}</small>
            </span>
            <span class="volume-capacity">
              {{
                t('cleanup.scanMode.volumeCapacity', {
                  available: ByteSizeService.diskCapacity(disk.availableBytes),
                  total: ByteSizeService.diskCapacity(disk.totalBytes),
                })
              }}
            </span>
          </label>
        </div>
        <p v-else class="volume-empty">{{ t('cleanup.scanMode.noVolumes') }}</p>
      </div>

      <MdDialogFooter class="volume-dialog-footer" align="between">
        <span class="selected-volume-count">
          {{
            t(
              'cleanup.scanMode.selectedVolumeCount',
              { count: FormatUtils.integer(selectedVolumeCount) },
              selectedVolumeCount
            )
          }}
        </span>
        <span class="volume-dialog-actions">
          <Button variant="outline" type="button" @click="emit('update:modelValue', false)">
            {{ t('common.cancel') }}
          </Button>
          <Button type="button" :disabled="!selectedVolumeCount" @click="confirm">
            {{ t('cleanup.scanMode.startSelectedScan') }}
          </Button>
        </span>
      </MdDialogFooter>
    </MdDialogContent>
  </Dialog>
</template>

<style scoped>
@reference "@assets/main.css";

.volume-list {
  container-type: inline-size;
  min-height: 0;
  max-height: min(430px, 58dvh);
  overflow-y: auto;
  padding: 12px var(--layout-dialog-body-inline-padding) 14px;
}

.volume-list-frame {
  @apply divide-y divide-border/60 overflow-hidden border border-border/70;
  border-radius: 9px;
}

.volume-option {
  @apply hover:bg-accent/55;
  display: grid;
  min-height: 66px;
  cursor: pointer;
  grid-template-columns: auto auto minmax(0, 1fr) auto;
  align-items: center;
  gap: 12px;
  padding: 9px 12px;
  transition: background-color 140ms ease;
}

.volume-option--fixed {
  cursor: default;
}

.volume-option--fixed:hover {
  background-color: transparent;
}

:deep(.system-volume-checkbox:disabled) {
  cursor: default;
  opacity: 1;
}

.volume-icon {
  @apply text-muted-foreground;
  display: grid;
  place-items: center;
}

.volume-copy {
  display: grid;
  min-width: 0;
  gap: 2px;
}

.volume-copy > small,
.volume-capacity,
.selected-volume-count {
  @apply text-muted-foreground;
  font-size: var(--font-content-secondary);
}

.volume-copy > small,
.volume-name-row strong {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.volume-name-row {
  display: flex;
  min-width: 0;
  align-items: center;
  gap: 7px;
}

.volume-name-row strong {
  font-size: var(--font-content-primary);
  font-weight: 600;
}

.volume-capacity {
  white-space: nowrap;
}

.volume-empty {
  @apply text-muted-foreground;
  margin: 20px 0;
  text-align: center;
  font-size: var(--font-content-primary);
}

.volume-dialog-footer {
  align-items: center;
}

.volume-dialog-actions {
  display: flex;
  gap: 8px;
}

@container (max-width: 520px) {
  .volume-option {
    grid-template-columns: auto auto minmax(0, 1fr);
  }

  .volume-capacity {
    grid-column: 3;
  }
}
</style>

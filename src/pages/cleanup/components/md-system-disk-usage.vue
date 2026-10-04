<script setup lang="ts">
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';

import MdIcon from '@/components/icons/md-icon.vue';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import type { DiskInfo } from '@/lib/models/disk';
import { ICON_NAMES } from '@/lib/models/ui';
import { ByteSizeService } from '@/lib/services/byte-size-service';
import { OperatingSystemService } from '@/lib/services/operating-system-service';
import * as FormatUtils from '@/lib/utils/format';

const props = defineProps<{
  disk: DiskInfo;
}>();

const { t } = useI18n({ useScope: 'global' });
const isMacOs = OperatingSystemService.isMacOs();
const diskName = computed(() => props.disk.name || props.disk.mountPoint);
const usagePercent = computed(() => FormatUtils.percent(props.disk.usedBytes, props.disk.totalBytes));
const usedCapacity = computed(() => ByteSizeService.diskCapacity(props.disk.usedBytes));
const availableCapacity = computed(() => ByteSizeService.diskCapacity(props.disk.availableBytes));
const totalCapacity = computed(() => ByteSizeService.diskCapacity(props.disk.totalBytes));
const summary = computed(() => t('cleanup.diskUsageSummary', { available: availableCapacity.value }));
const accessibleDetails = computed(() =>
  t('cleanup.diskUsageDetails', {
    name: diskName.value,
    used: usedCapacity.value,
    available: availableCapacity.value,
    total: totalCapacity.value,
  })
);
</script>

<template>
  <Tooltip>
    <TooltipTrigger as-child>
      <div
        class="system-disk-usage"
        :class="{ 'system-disk-usage--tight': usagePercent >= 90 }"
        :aria-label="accessibleDetails"
        role="group"
        tabindex="0"
      >
        <MdIcon class="system-disk-icon" :name="ICON_NAMES.hardDrive" :size="16" />
        <span class="system-disk-content">
          <span class="system-disk-copy">
            <span class="system-disk-name">{{ diskName }}</span>
            <span class="system-disk-value">{{ summary }}</span>
          </span>
          <span class="system-disk-track" aria-hidden="true">
            <span class="system-disk-progress" :style="{ width: `${usagePercent}%` }" />
          </span>
        </span>
      </div>
    </TooltipTrigger>
    <TooltipContent
      class="system-disk-tooltip grid w-64 gap-2 px-3 py-2.5 text-left text-wrap"
      side="bottom"
      align="start"
      :side-offset="8"
    >
      <strong class="truncate text-sm font-medium">{{ diskName }}</strong>
      <span class="grid gap-1.5 border-t border-background/20 pt-2">
        <span class="system-disk-tooltip-row">
          <span class="text-background/70">{{ t('cleanup.diskUsageUsedLabel') }}</span>
          <strong>{{ usedCapacity }}</strong>
        </span>
        <span class="system-disk-tooltip-row">
          <span class="text-background/70">{{ t('cleanup.diskUsageAvailableLabel') }}</span>
          <strong>{{ availableCapacity }}</strong>
        </span>
        <span class="system-disk-tooltip-row">
          <span class="text-background/70">{{ t('cleanup.diskUsageTotalLabel') }}</span>
          <strong>{{ totalCapacity }}</strong>
        </span>
      </span>
      <span v-if="isMacOs" class="border-t border-background/20 pt-2 text-xs text-background/70">
        {{ t('systemStatus.diskCapacityHint') }}
      </span>
    </TooltipContent>
  </Tooltip>
</template>

<style scoped>
@reference "@assets/main.css";

.system-disk-usage {
  @apply border-border/70 bg-card/35 text-muted-foreground;
  display: grid;
  width: 270px;
  height: 40px;
  min-width: 0;
  grid-template-columns: auto minmax(0, 1fr);
  align-items: center;
  gap: 9px;
  border-width: 1px;
  border-radius: 8px;
  padding: 5px 10px;
  font-size: 13px;
}

.system-disk-usage:focus-visible {
  @apply border-ring outline-none ring-2 ring-ring/35;
}

.system-disk-icon {
  color: var(--muted-foreground);
}

.system-disk-content {
  display: flex;
  min-width: 0;
  flex-direction: column;
  justify-content: center;
  gap: 4px;
}

.system-disk-copy {
  display: flex;
  min-width: 0;
  align-items: center;
  gap: 7px;
  line-height: 1;
}

.system-disk-track {
  @apply bg-border/45;
  position: relative;
  display: block;
  height: 3px;
  overflow: hidden;
  border-radius: 999px;
}

.system-disk-progress {
  @apply bg-muted-foreground/45;
  position: absolute;
  top: 0;
  bottom: 0;
  left: 0;
  border-radius: inherit;
  pointer-events: none;
  transition:
    width 180ms ease,
    background-color 180ms ease;
}

.system-disk-name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  color: var(--foreground);
  font-weight: 500;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.system-disk-value {
  white-space: nowrap;
}

.system-disk-usage--tight .system-disk-progress {
  @apply bg-warning/75;
}

.system-disk-usage--tight {
  @apply border-warning/30;
}

.system-disk-tooltip-row {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  gap: 16px;
}

.system-disk-tooltip-row strong {
  text-align: right;
  font-variant-numeric: tabular-nums;
}

@container (max-width: 620px) {
  .system-disk-usage {
    min-width: 0;
    max-width: 100%;
  }

  .system-disk-value {
    overflow: hidden;
    text-overflow: ellipsis;
  }
}
</style>

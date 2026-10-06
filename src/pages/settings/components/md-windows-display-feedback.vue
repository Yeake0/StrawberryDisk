<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import type { ResidentDisplayStatus, ResidentPreferences } from '@/lib/models/resident';
import { ResidentService } from '@/lib/services/resident-service';
import { LoggerService } from '@/lib/services/logger-service';

const props = defineProps<{ preferences: ResidentPreferences }>();
const { t } = useI18n({ useScope: 'global' });
const status = ref<ResidentDisplayStatus>('tray');
const hint = computed(() => {
  if (
    !props.preferences.enabled ||
    props.preferences.windowsDisplayMode !== 'taskbar' ||
    !props.preferences.metrics.some(item => item.enabled)
  )
    return null;
  return (
    (
      {
        noSpace: 'systemStatus.taskbarNoSpace',
        unsupportedLayout: 'systemStatus.taskbarUnsupported',
        shellUnavailable: 'systemStatus.taskbarUnavailable',
      } as Partial<Record<ResidentDisplayStatus, string>>
    )[status.value] ?? null
  );
});
let disposed = false;
let unlisten: (() => void) | undefined;
onMounted(async () => {
  try {
    let received = false;
    const stop = await ResidentService.onDisplayStatus(value => {
      received = true;
      if (!disposed) status.value = value;
    });
    if (disposed) {
      stop();
      return;
    }
    unlisten = stop;
    const value = await ResidentService.displayStatus();
    if (!disposed && !received) status.value = value;
  } catch {
    LoggerService.warn('resident', 'display_status_load_failed');
  }
});
onBeforeUnmount(() => {
  disposed = true;
  unlisten?.();
});
</script>

<template>
  <p v-if="hint" role="status" class="text-muted-foreground text-xs">{{ t(hint) }}</p>
</template>

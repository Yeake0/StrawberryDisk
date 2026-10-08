<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { BackgroundUpdateService } from '@/lib/services/background-update-service';
import { ResidentService } from '@/lib/services/resident-service';
import { LoggerService } from '@/lib/services/logger-service';

const { t } = useI18n();
const emit = defineEmits<{ error: [] }>();
const version = ref<string | null>(null);
const opening = ref(false);
const failed = ref(false);
let disposed = false;
let stop: (() => void) | undefined;
onMounted(() => {
  void BackgroundUpdateService.watch(notice => {
    version.value = notice.version;
  })
    .then(dispose => {
      if (disposed) dispose();
      else stop = dispose;
    })
    .catch(error => LoggerService.warn('app-update', 'update_notice_subscribe_failed', { error }));
});
onBeforeUnmount(() => {
  disposed = true;
  stop?.();
});
async function openMain() {
  if (opening.value) return;
  opening.value = true;
  failed.value = false;
  try {
    await ResidentService.openMain(version.value ? 'about' : 'main');
    if (version.value)
      LoggerService.info('app-update', 'update_notice_opened', { version: version.value, source: 'resource_panel' });
  } catch (error) {
    failed.value = true;
    emit('error');
    LoggerService.warn('app-update', 'update_notice_open_failed', { error });
  } finally {
    opening.value = false;
  }
}
</script>

<template>
  <button
    type="button"
    class="open-main-shortcut"
    :disabled="opening"
    :aria-label="version ? t(failed ? 'updates.noticeRetry' : 'updates.noticeAvailable') : t('monitoring.openMain')"
    @click="openMain"
  >
    {{ t('monitoring.openMain') }}
    <span v-if="version" class="update-dot" aria-hidden="true" />
  </button>
</template>

<style scoped>
@reference "@assets/main.css";
.open-main-shortcut {
  @apply text-muted-foreground;
  position: relative;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 7px;
  min-height: 30px;
  padding: 0 8px;
  font-size: 11px;
  cursor: pointer;
}
.open-main-shortcut:hover {
  @apply bg-accent text-accent-foreground;
}
.open-main-shortcut:focus-visible {
  outline: 2px solid var(--ring);
  outline-offset: 2px;
}
.open-main-shortcut:disabled {
  opacity: 0.45;
  cursor: default;
}
.update-dot {
  @apply bg-destructive;
  position: absolute;
  top: 2px;
  right: 2px;
  width: 6px;
  height: 6px;
  border-radius: 50%;
  pointer-events: none;
}
</style>

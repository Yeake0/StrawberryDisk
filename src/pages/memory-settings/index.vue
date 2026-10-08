<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import MdMemoryReleaseSettings from '@/components/memory-release/md-memory-release-settings.vue';
import { MemoryReleaseService } from '@/lib/services/memory-release-service';
import { LoggerService } from '@/lib/services/logger-service';
import { useAppStore } from '@/stores/app-store';
import { useMemoryReleaseStore } from '@/stores/memory-release-store';

const { t } = useI18n();
const appStore = useAppStore();
const store = useMemoryReleaseStore();
const windowFailed = ref(false);
let disposed = false;
let stopFocus: (() => void) | undefined;
let stopPreferences: (() => void) | undefined;
async function close() {
  try {
    await MemoryReleaseService.closeSettings();
  } catch (error) {
    windowFailed.value = true;
    LoggerService.warn('monitoring', 'memory_settings_close_failed', { error });
  }
}
onMounted(async () => {
  try {
    await MemoryReleaseService.showSettings();
    if (disposed) return;
    stopFocus = await MemoryReleaseService.onFocus(() => {
      void appStore.loadSettings();
    });
    if (disposed) {
      stopFocus();
      return;
    }
    stopPreferences = await MemoryReleaseService.onPreferences(value => store.accept(value));
    if (disposed) {
      stopPreferences();
      return;
    }
    await store.load();
  } catch (error) {
    windowFailed.value = true;
    LoggerService.warn('monitoring', 'memory_settings_show_failed', { error });
  }
});
onBeforeUnmount(() => {
  disposed = true;
  stopFocus?.();
  stopPreferences?.();
});
</script>

<template>
  <main class="memory-settings-window">
    <p v-if="windowFailed" role="alert">{{ t('memoryRelease.failed') }}</p>
    <MdMemoryReleaseSettings
      :preferences="store.preferences"
      :saving="store.saving"
      :failed="store.failed"
      :reload-preferences="store.load"
      :save-preferences="store.save"
      @close="close"
    />
  </main>
</template>

<style scoped>
.memory-settings-window {
  height: 100%;
  display: flex;
  flex-direction: column;
}
.memory-settings-window > section {
  flex: 1;
}
</style>

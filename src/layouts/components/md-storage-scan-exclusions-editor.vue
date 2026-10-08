<script setup lang="ts">
import { computed, ref } from 'vue';

import MdStorageScanExclusionsDialog from '@/components/custom/md-storage-scan-exclusions-dialog.vue';
import type { ScanExcludedFolder, ScanExcludedName } from '@/lib/models/storage-scan';
import { useAnalysisStore } from '@/stores/analysis-store';
import { useCleanupStore } from '@/stores/cleanup-store';
import { useDuplicateFilesStore } from '@/stores/duplicate-files-store';
import { useLargeFilesStore } from '@/stores/large-files-store';
import { useStorageScanPreferencesStore } from '@/stores/storage-scan-preferences-store';

const emit = defineEmits<{ error: [error: unknown] }>();
const preferencesStore = useStorageScanPreferencesStore();
const cleanupStore = useCleanupStore();
const largeFilesStore = useLargeFilesStore();
const duplicateFilesStore = useDuplicateFilesStore();
const analysisStore = useAnalysisStore();
const dialogOpen = ref(false);
const editorMounted = ref(false);
const saving = ref(false);
const busy = computed(
  () =>
    cleanupStore.loading ||
    cleanupStore.closingApplications ||
    largeFilesStore.loading ||
    largeFilesStore.deleting ||
    duplicateFilesStore.loading ||
    duplicateFilesStore.deleting ||
    analysisStore.pending ||
    analysisStore.deleting
);

async function open() {
  if (busy.value || saving.value || dialogOpen.value) return;
  try {
    // Wait for persisted preferences before the dialog copies folders into its editable draft.
    await preferencesStore.initialize();
    if (!busy.value) {
      editorMounted.value = true;
      dialogOpen.value = true;
    }
  } catch (error) {
    emit('error', error);
  }
}

async function saveFolders(folders: ScanExcludedFolder[], names: ScanExcludedName[]) {
  if (saving.value || busy.value) return;
  saving.value = true;
  try {
    await preferencesStore.saveFolders(folders, names);
    // Published results describe the scan configuration that produced them. Keep them visible
    // until the user scans again; each store still rejects stale results before destructive work.
    dialogOpen.value = false;
  } catch (error) {
    emit('error', error);
  } finally {
    saving.value = false;
  }
}

defineExpose({ open });
</script>

<template>
  <MdStorageScanExclusionsDialog
    v-if="editorMounted"
    v-model="dialogOpen"
    :folders="preferencesStore.folders"
    :names="preferencesStore.names"
    :saving="saving || busy"
    @error="emit('error', $event)"
    @save="saveFolders"
  />
</template>

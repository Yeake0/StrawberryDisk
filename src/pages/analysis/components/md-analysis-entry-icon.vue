<script setup lang="ts">
import MdNativeFileIcon from '@/components/custom/md-native-file-icon.vue';
import MdSpinner from '@/components/custom/md-spinner.vue';
import type { DirectoryEntryInfo } from '@/lib/models/analysis';

defineProps<{ entry: DirectoryEntryInfo; deleting: boolean; compact?: boolean }>();
</script>

<template>
  <span class="analysis-entry-icon" :class="{ compact, 'is-deleting': deleting }" aria-hidden="true">
    <MdSpinner v-if="deleting" />
    <MdNativeFileIcon
      v-else
      :path="entry.path"
      :name="entry.name"
      :directory="entry.isDirectory"
      directory-mode="generic"
      :compact="compact"
    />
  </span>
</template>

<style scoped>
@reference "@assets/main.css";
.analysis-entry-icon {
  display: grid;
  width: 34px;
  height: 34px;
  flex: none;
  place-items: center;
}
.analysis-entry-icon.compact {
  width: 30px;
  height: 30px;
}
.analysis-entry-icon.is-deleting {
  @apply text-primary;
}
</style>

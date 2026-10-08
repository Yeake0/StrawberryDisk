<script setup lang="ts">
import type { HTMLAttributes } from 'vue';

import DialogHeader from '@/components/ui/dialog/DialogHeader.vue';

withDefaults(
  defineProps<{
    class?: HTMLAttributes['class'];
    variant?: 'standard' | 'alert' | 'brand';
  }>(),
  {
    class: undefined,
    variant: 'standard',
  }
);
</script>

<template>
  <DialogHeader
    data-tauri-drag-region
    :class="['md-dialog-header relative gap-0', `md-dialog-header--${variant}`, $props.class]"
  >
    <slot />
    <!--
      Tauri only starts native window dragging when the pressed element is marked as a drag
      region. This overlay makes the complete visual header reliable, including title and
      description text, while the dialog close button remains above it in MdDialogContent.
    -->
    <div data-tauri-drag-region class="absolute inset-0 z-10" aria-hidden="true" />
  </DialogHeader>
</template>

<style scoped>
.md-dialog-header--standard {
  box-sizing: border-box;
  min-height: var(--layout-dialog-header-height);
  padding: var(--layout-dialog-header-padding);
}

.md-dialog-header--standard > :deep([data-slot='dialog-description']) {
  margin-top: var(--layout-dialog-header-description-gap);
  line-height: 1.4286;
}

/* A title-only header uses balanced vertical padding so its text shares the
   close button's center. Headers with descriptions keep their existing rhythm. */
.md-dialog-header--standard:has(> [data-slot='dialog-title']):not(:has([data-slot='dialog-description'])) {
  justify-content: center;
  padding-block: 12px;
}

.md-dialog-header--alert {
  box-sizing: border-box;
  display: grid;
  grid-template-columns: 40px minmax(0, 1fr);
  min-height: var(--layout-dialog-header-height);
  gap: 12px;
  padding: var(--layout-dialog-alert-header-padding);
}

.md-dialog-header--alert > :deep(.md-dialog-header-copy) {
  display: flex;
  min-width: 0;
  flex-direction: column;
  gap: 2px;
}

.md-dialog-header--alert > :deep(.md-dialog-header-copy [data-slot='dialog-title']) {
  line-height: 1;
}

.md-dialog-header--alert > :deep(.md-dialog-header-copy [data-slot='dialog-description']) {
  line-height: 1.4286;
}
</style>

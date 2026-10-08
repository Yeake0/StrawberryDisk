<script setup lang="ts">
import type { HTMLAttributes } from 'vue';

import DialogFooter from '@/components/ui/dialog/DialogFooter.vue';

withDefaults(
  defineProps<{
    class?: HTMLAttributes['class'];
    align?: 'end' | 'between';
  }>(),
  {
    class: undefined,
    align: 'end',
  }
);
</script>

<template>
  <DialogFooter
    :class="['md-dialog-footer gap-2', align === 'between' ? 'sm:justify-between' : 'sm:justify-end', $props.class]"
  >
    <slot />
  </DialogFooter>
</template>

<style scoped>
@reference "@assets/main.css";

.md-dialog-footer {
  box-sizing: border-box;
  min-height: var(--layout-dialog-footer-height);
  flex: none;
  align-items: center;
  border-top-width: 1px;
  padding: var(--layout-dialog-footer-padding);
  @apply border-border/70 bg-muted/20;
}
</style>

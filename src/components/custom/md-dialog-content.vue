<script setup lang="ts">
import { useI18n } from 'vue-i18n';
import { reactiveOmit } from '@vueuse/core';
import type { DialogContentEmits, DialogContentProps } from 'reka-ui';
import { DialogClose, useForwardPropsEmits } from 'reka-ui';
import type { HTMLAttributes } from 'vue';

import MdIcon from '@/components/icons/md-icon.vue';
import DialogContent from '@/components/ui/dialog/DialogContent.vue';
import { ICON_NAMES } from '@/lib/models/ui';

const { t } = useI18n({ useScope: 'global' });
const props = withDefaults(
  defineProps<
    DialogContentProps & {
      class?: HTMLAttributes['class'];
      showClose?: boolean;
      size?: 'compact' | 'standard' | 'large' | 'wide';
      height?: 'auto' | 'tall';
    }
  >(),
  {
    class: undefined,
    showClose: true,
    size: 'standard',
    height: 'auto',
  }
);
const emits = defineEmits<DialogContentEmits>();
const delegatedProps = reactiveOmit(props, 'class', 'showClose', 'size', 'height');
const forwarded = useForwardPropsEmits(delegatedProps, emits);
// The generated primitive owns default max-width and gap utilities. Passing
// replacements through its class merger removes those defaults before CSS is
// generated; plain wrapper CSS can lose to Tailwind's later cascade order.
const sizeClass = {
  compact: 'max-w-[var(--layout-dialog-compact-width)]',
  standard: 'max-w-[var(--layout-dialog-standard-width)]',
  large: 'max-w-[var(--layout-dialog-large-width)]',
  wide: 'max-w-[var(--layout-dialog-wide-width)]',
} as const;
</script>

<template>
  <DialogContent
    v-bind="forwarded"
    :class="[
      'md-dialog-content gap-0',
      sizeClass[props.size],
      { 'md-dialog-content--tall': props.height === 'tall' },
      props.class,
    ]"
  >
    <slot />
    <DialogClose
      v-if="props.showClose"
      class="absolute top-4 right-4 z-20 grid size-9 place-items-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
    >
      <MdIcon :name="ICON_NAMES.close" :size="18" />
      <span class="sr-only">{{ t('common.close') }}</span>
    </DialogClose>
  </DialogContent>
</template>

<style>
/*
 * DialogContent is teleported to document.body by the generated primitive.
 * Scoped selectors stay on this wrapper's local subtree and therefore cannot
 * constrain the teleported node. These project-prefixed classes intentionally
 * remain global so every real dialog receives the viewport safety boundary.
 */
.md-dialog-content {
  width: calc(100% - var(--layout-dialog-viewport-inset) - var(--layout-dialog-viewport-inset));
  max-height: calc(100vh - var(--layout-dialog-viewport-inset) - var(--layout-dialog-viewport-inset));
  gap: 0;
  overflow: hidden;
  padding: 0;
}

/*
 * WebKit can leave an exit animation running after the dialog is closed,
 * retaining its modal overlay and blocking the page. Release both surfaces
 * immediately on close instead of making dismissal depend on animationend.
 * Opening animations remain unchanged.
 */
.md-dialog-content[data-state='closed'],
[data-slot='dialog-overlay'][data-state='closed'] {
  animation: none;
}

.md-dialog-content--tall {
  height: min(
    var(--layout-dialog-tall-height),
    calc(100vh - var(--layout-dialog-viewport-inset) - var(--layout-dialog-viewport-inset))
  );
}
</style>

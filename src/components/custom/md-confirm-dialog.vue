<script setup lang="ts">
import MdDialogContent from '@/components/custom/md-dialog-content.vue';
import MdDialogFooter from '@/components/custom/md-dialog-footer.vue';
import MdDialogHeader from '@/components/custom/md-dialog-header.vue';
import MdSpinner from '@/components/custom/md-spinner.vue';
import { Button } from '@/components/ui/button';
import { Dialog, DialogDescription, DialogTitle } from '@/components/ui/dialog';

const props = withDefaults(
  defineProps<{
    open: boolean;
    title: string;
    description: string;
    cancelLabel: string;
    confirmLabel: string;
    confirmVariant?: 'default' | 'destructive';
    size?: 'compact' | 'standard';
    busy?: boolean;
  }>(),
  {
    confirmVariant: 'default',
    size: 'compact',
    busy: false,
  }
);

const emit = defineEmits<{
  'update:open': [value: boolean];
  confirm: [];
}>();

function updateOpen(open: boolean) {
  // An accepted operation may continue after confirmation. Keep the modal
  // visible while busy so its progress cannot disappear through Escape or an
  // outside click before the owning workflow has settled.
  if (!open && props.busy) return;
  emit('update:open', open);
}
</script>

<template>
  <Dialog :open="open" @update:open="updateOpen">
    <MdDialogContent :size="size" :show-close="!busy">
      <MdDialogHeader>
        <DialogTitle>{{ title }}</DialogTitle>
        <DialogDescription>{{ description }}</DialogDescription>
      </MdDialogHeader>
      <div v-if="$slots.default" class="md-confirm-dialog-body">
        <slot />
      </div>
      <MdDialogFooter>
        <Button variant="outline" type="button" :disabled="busy" @click="updateOpen(false)">
          {{ cancelLabel }}
        </Button>
        <Button :variant="confirmVariant" type="button" :disabled="busy" @click="emit('confirm')">
          <MdSpinner v-if="busy" size="small" />
          {{ confirmLabel }}
        </Button>
      </MdDialogFooter>
    </MdDialogContent>
  </Dialog>
</template>

<style scoped>
.md-confirm-dialog-body {
  min-width: 0;
  padding: 0 var(--layout-dialog-body-inline-padding) 12px;
}
</style>

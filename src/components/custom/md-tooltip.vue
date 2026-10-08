<script setup lang="ts">
import { onDeactivated, ref, watch } from 'vue';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import { TOOLTIP_OPEN_DELAY_MS } from '@/lib/models/ui';

defineOptions({ inheritAttrs: false });
const props = defineProps<{ text?: string | null; delayDuration?: number }>();
const open = ref(false);
// Recycled rows must close a tooltip before it starts describing another file.
watch(
  () => props.text,
  () => {
    open.value = false;
  },
  { flush: 'sync' }
);
onDeactivated(() => {
  open.value = false;
});
</script>

<template>
  <!-- Standalone resident windows do not inherit the main window's provider. -->
  <TooltipProvider
    :delay-duration="delayDuration ?? TOOLTIP_OPEN_DELAY_MS"
    :disable-hoverable-content="true"
    :ignore-non-keyboard-focus="true"
  >
    <Tooltip v-model:open="open" :disabled="!text">
      <!-- Preserve the caller's element, layout, accessible name and handlers. -->
      <TooltipTrigger as-child v-bind="$attrs"><slot /></TooltipTrigger>
      <!-- Portal content does not inherit this wrapper's scoped CSS attributes. -->
      <!-- Natural wrapping avoids balanced lines leaving unused space in descriptions and file paths. -->
      <TooltipContent
        v-if="open"
        class="max-w-[min(24rem,calc(100vw-24px))] text-left whitespace-normal text-wrap [overflow-wrap:anywhere]"
      >
        <slot name="content">{{ text }}</slot>
      </TooltipContent>
    </Tooltip>
  </TooltipProvider>
</template>

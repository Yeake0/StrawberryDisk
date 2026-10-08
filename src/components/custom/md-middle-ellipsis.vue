<script setup lang="ts">
import MdTooltip from '@/components/custom/md-tooltip.vue';
import { computed } from 'vue';

const props = withDefaults(
  defineProps<{
    text: string;
    tailLength?: number;
    showTooltip?: boolean;
  }>(),
  {
    tailLength: 12,
    showTooltip: true,
  }
);

const parts = computed(() => {
  // Preserve a Unicode-safe suffix and let CSS truncate the prefix, producing
  // stable middle ellipsis without measuring the DOM.
  const characters = Array.from(props.text);
  const requestedTailLength = Math.max(0, props.tailLength);
  if (requestedTailLength === 0 || characters.length <= requestedTailLength * 2) {
    return { start: props.text, end: '' };
  }
  const tailLength = Math.min(requestedTailLength, characters.length - 1);
  return {
    start: characters.slice(0, -tailLength).join(''),
    end: characters.slice(-tailLength).join(''),
  };
});
</script>

<template>
  <!-- Keep the native root so callers retain their scoped typography and sizing. -->
  <span class="md-middle-ellipsis">
    <MdTooltip v-if="showTooltip" :text="text">
      <span class="ellipsis-content">
        <span class="ellipsis-start">{{ parts.start }}</span>
        <span v-if="parts.end" class="ellipsis-end">{{ parts.end }}</span>
      </span>
    </MdTooltip>
    <span v-else class="ellipsis-content">
      <span class="ellipsis-start">{{ parts.start }}</span>
      <span v-if="parts.end" class="ellipsis-end">{{ parts.end }}</span>
    </span>
  </span>
</template>

<style scoped>
.md-middle-ellipsis,
.ellipsis-content {
  display: flex;
  min-width: 0;
  max-width: 100%;
  align-items: baseline;
  white-space: nowrap;
}

.ellipsis-content {
  flex: 1;
}

.ellipsis-start {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
}

.ellipsis-end {
  flex: none;
}
</style>

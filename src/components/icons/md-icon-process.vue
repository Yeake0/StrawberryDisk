<script setup lang="ts">
import { Cog } from '@lucide/vue';

withDefaults(defineProps<{ size?: number }>(), { size: 24 });
</script>

<template>
  <span class="process-icon" :style="{ width: `${size}px`, height: `${size}px` }" aria-hidden="true">
    <Cog :size="Math.round(size * 0.68)" :stroke-width="1.7" />
  </span>
</template>

<style scoped>
.process-icon {
  display: grid;
  place-items: center;
  border: 1.5px solid currentColor;
  border-radius: 4px;
}
</style>

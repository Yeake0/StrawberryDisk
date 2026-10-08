<script setup lang="ts">
import { ref } from 'vue';

interface CatalogListScrollOptions {
  top?: number;
  left?: number;
  behavior?: 'auto' | 'smooth';
}

const scrollElement = ref<HTMLElement | null>(null);

function scrollTo(options: CatalogListScrollOptions) {
  scrollElement.value?.scrollTo(options);
}

defineExpose({ scrollTo });
</script>

<template>
  <!--
    System catalogs keep their toolbar outside this element. Owning the scroll
    boundary here prevents pages with similar rows from drifting into different
    overflow, gutter, and overscroll behavior.
  -->
  <div ref="scrollElement" class="md-catalog-list scrollbar-stable-end">
    <slot />
  </div>
</template>

<style scoped>
.md-catalog-list {
  display: flex;
  min-width: 0;
  min-height: 0;
  flex: 1;
  flex-direction: column;
  overscroll-behavior: contain;
}
</style>

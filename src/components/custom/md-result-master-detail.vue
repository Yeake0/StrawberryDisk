<script setup lang="ts">
withDefaults(
  defineProps<{
    empty?: boolean;
    navigationLabel?: string;
  }>(),
  { empty: false, navigationLabel: undefined }
);
</script>

<template>
  <div class="result-master-detail" :class="{ empty }">
    <aside v-if="!empty" class="result-master-detail-navigation scrollbar-stable" :aria-label="navigationLabel">
      <slot name="navigation" />
    </aside>
    <slot />
  </div>
</template>

<style scoped>
@reference "@assets/main.css";

.result-master-detail {
  @apply border-border/70 bg-workspace;
  display: grid;
  min-width: 0;
  min-height: 0;
  flex: 1;
  grid-template-columns: clamp(250px, 31%, 324px) minmax(0, 1fr);
  overflow: hidden;
  border-width: 1px;
  border-radius: var(--radius);
}

.result-master-detail.embedded {
  border: 0;
  border-radius: 0;
}

.result-master-detail.empty {
  grid-template-columns: minmax(0, 1fr);
}

.result-master-detail-navigation {
  @apply border-border/70 bg-workspace;
  min-height: 0;
  border-right-width: 1px;
  padding: 8px 6px;
}

@container (max-width: 760px) {
  .result-master-detail {
    grid-template-columns: minmax(190px, 34%) minmax(0, 1fr);
  }
}
</style>

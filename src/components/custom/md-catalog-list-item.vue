<script setup lang="ts">
import MdTooltip from '@/components/custom/md-tooltip.vue';
withDefaults(
  defineProps<{
    title: string;
    description?: string;
  }>(),
  {
    description: undefined,
  }
);
</script>

<template>
  <!--
    This row owns only cross-page geometry and typography. Status, progress,
    and controls remain in named slots because their behavior belongs to the
    catalog page rather than to this presentation primitive.
  -->
  <article class="md-catalog-list-item">
    <div class="md-catalog-list-item-copy">
      <div class="md-catalog-list-item-heading">
        <MdTooltip :text="title"
          ><strong>{{ title }}</strong></MdTooltip
        >
        <slot name="title-after" />
      </div>
      <div class="md-catalog-list-item-details">
        <slot name="description">
          <small v-if="description" class="md-catalog-list-item-description">{{ description }}</small>
        </slot>
      </div>
    </div>
    <div class="md-catalog-list-item-actions">
      <slot name="actions" />
    </div>
  </article>
</template>

<style scoped>
@reference "@assets/main.css";

.md-catalog-list-item {
  display: grid;
  min-height: 66px;
  grid-template-columns: minmax(0, 1fr) auto;
  align-items: center;
  gap: 16px;
  border-bottom: 1px solid color-mix(in oklab, var(--border) 70%, transparent);
  padding: 10px 18px;
}

.md-catalog-list-item:last-child {
  border-bottom: 0;
}

.md-catalog-list-item:hover {
  background: color-mix(in oklab, var(--muted) 28%, transparent);
}

.md-catalog-list-item-copy {
  display: flex;
  min-width: 0;
  flex-direction: column;
  gap: 3px;
}

.md-catalog-list-item-heading {
  display: flex;
  min-width: 0;
  min-height: 20px;
  align-items: center;
  gap: 7px;
  overflow: hidden;
}

.md-catalog-list-item-heading > strong {
  min-width: 0;
  overflow: hidden;
  font-size: var(--font-content-primary);
  font-weight: 600;
  line-height: 1.4;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.md-catalog-list-item-details {
  display: flex;
  min-width: 0;
  min-height: 18px;
  align-items: center;
}

.md-catalog-list-item-description {
  overflow: hidden;
  color: var(--muted-foreground);
  font-size: var(--font-content-secondary);
  line-height: 1.45;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.md-catalog-list-item-actions {
  display: flex;
  min-width: 0;
  align-items: center;
  justify-content: flex-end;
  gap: 10px;
}
</style>

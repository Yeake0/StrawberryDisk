<script setup lang="ts">
withDefaults(
  defineProps<{
    tone?: 'neutral' | 'primary' | 'success' | 'warning' | 'destructive';
    size?: 'compact' | 'default';
  }>(),
  {
    tone: 'neutral',
    size: 'default',
  }
);
</script>

<template>
  <span class="md-status-badge" :class="[`md-status-badge--${tone}`, `md-status-badge--${size}`]">
    <slot />
  </span>
</template>

<style scoped>
@reference "@assets/main.css";

.md-status-badge {
  display: inline-flex;
  flex: none;
  align-items: center;
  border-radius: 999px;
  white-space: nowrap;
  font-style: normal;
  font-weight: 500;
}

.md-status-badge--default {
  padding: 3px 8px;
  font-size: var(--font-content-secondary);
}

.md-status-badge--compact {
  padding: 2px 7px;
  font-size: var(--font-content-meta);
}

.md-status-badge--neutral {
  @apply bg-muted text-muted-foreground;
}

.md-status-badge--primary {
  @apply text-primary;
  background: var(--surface-primary-subtle);
}

.md-status-badge--success {
  @apply text-success-foreground;
  background: var(--surface-success-subtle);
}

.md-status-badge--warning {
  @apply text-warning-foreground;
  background: var(--surface-warning-subtle);
}

.md-status-badge--destructive {
  @apply text-destructive;
  background: var(--surface-destructive-subtle);
}
</style>

<script setup lang="ts">
import MdIcon from '@/components/icons/md-icon.vue';
import { ICON_NAMES } from '@/lib/models/ui';

type NoticeIconName = (typeof ICON_NAMES)[keyof typeof ICON_NAMES];

withDefaults(
  defineProps<{
    title?: string;
    iconName?: NoticeIconName;
    tone?: 'neutral' | 'info' | 'warning' | 'destructive' | 'success';
    role?: 'status' | 'alert';
  }>(),
  {
    title: undefined,
    iconName: undefined,
    tone: 'neutral',
    role: undefined,
  }
);
</script>

<template>
  <section class="md-inline-notice" :class="`md-inline-notice--${tone}`" :role="role">
    <span v-if="iconName || $slots.icon" class="md-inline-notice-icon" aria-hidden="true">
      <slot name="icon"><MdIcon v-if="iconName" :name="iconName" :size="17" /></slot>
    </span>
    <div class="md-inline-notice-copy">
      <strong v-if="title">{{ title }}</strong>
      <div class="md-inline-notice-message"><slot /></div>
    </div>
  </section>
</template>

<style scoped>
@reference "@assets/main.css";

.md-inline-notice {
  display: flex;
  flex: none;
  align-items: flex-start;
  gap: 10px;
  border-width: 1px;
  border-radius: var(--radius-lg);
  padding: 11px 14px;
  @apply border-border/70 bg-muted/45 text-foreground;
}

.md-inline-notice-icon {
  display: grid;
  flex: none;
  place-items: center;
  margin-top: 1px;
  @apply text-muted-foreground;
}

.md-inline-notice-copy {
  display: flex;
  min-width: 0;
  flex: 1;
  flex-direction: column;
  gap: 3px;
}

.md-inline-notice-copy > strong {
  font-size: var(--font-content-primary);
}

.md-inline-notice-message {
  @apply text-muted-foreground;
  font-size: var(--font-content-body);
  line-height: 1.55;
}

.md-inline-notice--info {
  background: var(--surface-primary-subtle);
}

.md-inline-notice--info .md-inline-notice-icon {
  @apply text-primary;
}

.md-inline-notice--warning {
  background: var(--surface-warning-subtle);
}

.md-inline-notice--warning .md-inline-notice-icon {
  @apply text-warning;
}

.md-inline-notice--destructive {
  background: var(--surface-destructive-subtle);
}

.md-inline-notice--destructive .md-inline-notice-icon {
  @apply text-destructive;
}

.md-inline-notice--success {
  background: var(--surface-success-subtle);
}

.md-inline-notice--success .md-inline-notice-icon {
  @apply text-success;
}
</style>

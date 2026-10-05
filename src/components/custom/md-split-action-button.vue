<script setup lang="ts">
import {
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuPortal,
  DropdownMenuRoot,
  DropdownMenuTrigger,
} from 'reka-ui';
import { computed } from 'vue';

import MdIcon from '@/components/icons/md-icon.vue';
import { Button } from '@/components/ui/button';
import type { ButtonVariants } from '@/components/ui/button/button-variants';
import type { IconName } from '@/lib/models/ui';

interface SplitActionItem {
  description: string;
  icon: IconName;
  label: string;
  value: string;
}

const props = withDefaults(
  defineProps<{
    accessibleLabel: string;
    selectedValue?: string;
    disabled?: boolean;
    items?: readonly SplitActionItem[];
    primaryIcon: IconName;
    primaryLabel: string;
    size?: ButtonVariants['size'];
    variant?: ButtonVariants['variant'];
  }>(),
  {
    selectedValue: undefined,
    disabled: false,
    items: () => [],
    size: 'default',
    variant: 'default',
  }
);

const emit = defineEmits<{
  primary: [];
  select: [value: string];
}>();

const hasMenu = computed(() => props.items.length > 0);
</script>

<template>
  <div
    class="md-split-action"
    :class="[
      `md-split-action--${variant}`,
      { 'md-split-action--joined': hasMenu, 'md-split-action--large': size === 'lg' },
    ]"
    :role="hasMenu ? 'group' : undefined"
    :aria-label="hasMenu ? accessibleLabel : undefined"
  >
    <Button
      class="md-split-action__primary"
      :class="{ 'rounded-r-none shadow-none': hasMenu }"
      :variant="variant"
      :size="size"
      type="button"
      :disabled="disabled"
      @click="emit('primary')"
    >
      <slot name="primary">
        <MdIcon :name="primaryIcon" :size="16" />
        {{ primaryLabel }}
      </slot>
    </Button>

    <DropdownMenuRoot v-if="hasMenu">
      <DropdownMenuTrigger as-child>
        <Button
          class="md-split-action__menu rounded-l-none px-0 shadow-none"
          :variant="variant"
          :size="size"
          type="button"
          :disabled="disabled"
          :aria-label="accessibleLabel"
        >
          <MdIcon name="chevronDown" :size="16" />
        </Button>
      </DropdownMenuTrigger>

      <DropdownMenuPortal>
        <DropdownMenuContent
          align="end"
          :side-offset="6"
          class="z-50 w-80 max-w-[calc(100vw-32px)] overflow-hidden rounded-lg border border-border bg-popover p-1 text-popover-foreground shadow-xl data-[state=closed]:animate-out data-[state=open]:animate-in data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95"
        >
          <DropdownMenuItem
            v-for="item in items"
            :key="item.value"
            class="md-split-action__item"
            :aria-current="item.value === selectedValue ? 'true' : undefined"
            @select="emit('select', item.value)"
          >
            <MdIcon :name="item.icon" :size="16" />
            <span class="md-split-action__copy">
              <strong>{{ item.label }}</strong>
              <small>{{ item.description }}</small>
            </span>
            <MdIcon v-if="item.value === selectedValue" name="check" :size="16" class="ml-auto" aria-hidden="true" />
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenuPortal>
    </DropdownMenuRoot>
  </div>
</template>

<style scoped>
@reference "@assets/main.css";

.md-split-action {
  display: inline-flex;
  flex: none;
  border-radius: var(--radius-lg);
}

.md-split-action--joined.md-split-action--default {
  @apply shadow-md shadow-primary/20;
}

.md-split-action--joined .md-split-action__primary,
.md-split-action--joined .md-split-action__menu {
  box-shadow: none;
}

.md-split-action--joined .md-split-action__primary {
  border-right-width: 0;
}

.md-split-action__menu {
  width: 36px;
}

.md-split-action--large .md-split-action__menu {
  width: 42px;
}

.md-split-action--default .md-split-action__menu {
  border-left: 1px solid color-mix(in srgb, var(--primary-foreground) 24%, transparent);
}

.md-split-action__primary:hover,
.md-split-action__menu:hover {
  transform: none;
}

.md-split-action__item {
  @apply focus:bg-accent focus:text-accent-foreground;
  position: relative;
  display: flex;
  cursor: default;
  user-select: none;
  align-items: flex-start;
  gap: 10px;
  border-radius: calc(var(--radius) - 2px);
  padding: 10px;
  font-size: var(--font-content-secondary);
  outline: none;
  transition:
    color 0.15s ease,
    background-color 0.15s ease;
}

.md-split-action__item[data-disabled] {
  pointer-events: none;
  opacity: 0.5;
}

.md-split-action__item > :deep(svg) {
  margin-top: 2px;
  flex: none;
}

.md-split-action__copy {
  display: flex;
  min-width: 0;
  flex-direction: column;
  gap: 2px;
}

.md-split-action__copy strong {
  color: var(--popover-foreground);
  font-size: var(--font-content-primary);
  font-weight: 600;
  line-height: 1.4;
}

.md-split-action__copy small {
  color: var(--muted-foreground);
  font-size: var(--font-content-secondary);
  line-height: 1.45;
}
</style>

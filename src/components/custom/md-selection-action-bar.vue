<script setup lang="ts">
import MdActionBarContainer from '@/components/custom/md-action-bar-container.vue';
import { Button } from '@/components/ui/button';

withDefaults(
  defineProps<{
    selectedLabel: string;
    selectedValue: string;
    spaceLabel?: string;
    spaceValue?: string;
    clearLabel?: string;
    actionLabel: string;
    emphasizeSelectedValue?: boolean;
    disabled?: boolean;
    busy?: boolean;
  }>(),
  {
    clearLabel: undefined,
    spaceLabel: undefined,
    spaceValue: undefined,
    emphasizeSelectedValue: false,
    disabled: false,
    busy: false,
  }
);

const emit = defineEmits<{
  clear: [];
  action: [];
}>();
</script>

<template>
  <MdActionBarContainer class="@container/selection-bar flex-wrap justify-start gap-x-3 gap-y-1.5 py-0.5 pr-2.5 pl-3.5">
    <div class="flex min-w-0 flex-none flex-wrap items-baseline gap-x-2 gap-y-0.5">
      <span class="flex items-baseline gap-1.5">
        <small class="text-content-meta text-muted-foreground">{{ selectedLabel }}</small>
        <strong
          class="text-content-primary whitespace-nowrap"
          :class="emphasizeSelectedValue && !disabled ? 'text-primary' : 'text-foreground'"
        >
          {{ selectedValue }}
        </strong>
      </span>
      <i v-if="spaceLabel && spaceValue" class="h-4 w-px flex-none self-center bg-border" />
      <span v-if="spaceLabel && spaceValue" class="flex items-baseline gap-1.5">
        <small class="text-content-meta text-muted-foreground">{{ spaceLabel }}</small>
        <strong
          class="text-content-section-title whitespace-nowrap"
          :class="disabled ? 'text-muted-foreground' : 'text-primary'"
        >
          {{ spaceValue }}
        </strong>
      </span>
    </div>

    <div v-if="$slots.options" class="ml-auto flex min-w-0 flex-auto items-center justify-end">
      <slot name="options" />
    </div>

    <div class="flex flex-none items-center gap-2" :class="$slots.options ? 'ml-0' : 'ml-auto'">
      <Button v-if="clearLabel" variant="ghost" type="button" :disabled="disabled || busy" @click="emit('clear')">
        {{ clearLabel }}
      </Button>
      <Button
        class="min-w-31 disabled:bg-muted disabled:text-muted-foreground disabled:shadow-none"
        type="button"
        :disabled="disabled || busy"
        @click="emit('action')"
      >
        <slot name="action-icon" />
        {{ actionLabel }}
      </Button>
    </div>
  </MdActionBarContainer>
</template>

<script setup lang="ts">
import { Checkbox } from '@/components/ui/checkbox';
import MdIcon from '@/components/icons/md-icon.vue';
import { ICON_NAMES } from '@/lib/models/ui';

type CheckboxState = boolean | 'indeterminate';

defineOptions({ inheritAttrs: false });

withDefaults(
  defineProps<{
    modelValue?: CheckboxState;
    disabled?: boolean;
  }>(),
  {
    modelValue: false,
    disabled: false,
  }
);

const emit = defineEmits<{
  'update:modelValue': [value: CheckboxState];
}>();
</script>

<template>
  <!-- One visual and keyboard contract serves forms, settings, and result selection. -->
  <Checkbox
    v-bind="$attrs"
    class="md-checkbox size-[17px] rounded-[5px] border-[1.5px] shadow-none data-[state=indeterminate]:border-primary data-[state=indeterminate]:bg-primary data-[state=indeterminate]:text-primary-foreground"
    :model-value="modelValue"
    :disabled="disabled"
    @update:model-value="emit('update:modelValue', $event)"
  >
    <MdIcon
      :name="modelValue === 'indeterminate' ? ICON_NAMES.minus : ICON_NAMES.check"
      :size="12"
      :stroke-width="2.5"
    />
  </Checkbox>
</template>

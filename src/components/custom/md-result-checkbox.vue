<script setup lang="ts">
import { computed } from 'vue';

import MdCheckbox from '@/components/custom/md-checkbox.vue';

type ResultCheckboxState = boolean | 'indeterminate';

defineOptions({ inheritAttrs: false });

const props = withDefaults(
  defineProps<{
    checked?: boolean;
    disabled?: boolean;
    indeterminate?: boolean;
  }>(),
  {
    checked: false,
    disabled: false,
    indeterminate: false,
  }
);

const emit = defineEmits<{
  'update:checked': [checked: boolean];
}>();

/*
 * Result pages need the same checked, partial, disabled, and focus behavior.
 * Keeping the state adapter here prevents individual pages from falling back
 * to browser-native checkbox rendering or implementing indeterminate state
 * differently.
 */
const state = computed<ResultCheckboxState>(() => (props.indeterminate ? 'indeterminate' : props.checked));

function updateChecked(value: ResultCheckboxState) {
  emit('update:checked', value === true);
}
</script>

<template>
  <MdCheckbox
    v-bind="$attrs"
    class="md-result-checkbox"
    :model-value="state"
    :disabled="disabled"
    @update:model-value="updateChecked"
  />
</template>

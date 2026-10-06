<!--
Carbon Design System "chart-treemap" by IBM. SPDX-License-Identifier: Apache-2.0.
Source: https://icon-sets.iconify.design/carbon/chart-treemap/
Adapted for Vue sizing; the icon geometry is unchanged.
License: public/licenses/carbon-icons.txt
-->
<script setup lang="ts">
withDefaults(defineProps<{ size?: number }>(), {
  size: 20,
});
</script>

<template>
  <svg :width="size" :height="size" viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg" aria-hidden="true">
    <path
      fill="currentColor"
      d="M28 2H4a2 2 0 0 0-2 2v24a2 2 0 0 0 2 2h24a2 2 0 0 0 2-2V4a2 2 0 0 0-2-2m0 12h-5V4h5ZM16 4h5v10h-5Zm-2 0v16H4V4ZM4 22h10v6H4Zm12 6V16h12v12Z"
    />
  </svg>
</template>

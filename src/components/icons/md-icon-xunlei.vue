<script setup lang="ts">
defineProps<{ size: number }>();
</script>

<template>
  <svg
    :width="size"
    :height="size"
    viewBox="0 0 48 48"
    fill="none"
    stroke="currentColor"
    stroke-width="3"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
  >
    <path
      d="M27.23 28.86c1.975 4.68 3.568 7.483 5.829 12.754-3.485-4.045-7.909-7.71-11.865-9.59-6.919-3.286-9.034-6.955-7.973-12.406.874-4.493-4.014-7.132-7.72-8.782l6.98 2.197c3.186-1.652 6.249.742 7.987 3.717 6.248-5.128 12.675-8.52 19.859-10.364-1.87 2.706-3.133 4.646-4.79 7.837l1.276-.283c-1.682 2.453-3.592 4.583-5.044 7.602 3.744.678 8.069 2.087 10.731 3.94-5.338.804-10.627 1.673-15.27 3.377"
    />
  </svg>
</template>

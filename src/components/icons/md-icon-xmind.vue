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
      d="M31.779 17.953c-.307-2.698-.15-5.46.494-8.163a23.7 23.7 0 0 0-9.367-4.29 17.74 17.74 0 0 0 2.267 17.6 17.74 17.74 0 0 0 16.41 6.747 23.65 23.65 0 0 0-1.75-10.24 23.66 23.66 0 0 1-18.102-9.23m-5.51 19.67c.307 2.698.15 5.46-.494 8.163a23.7 23.7 0 0 0 9.367 4.29 17.74 17.74 0 0 0-2.267-17.6 17.74 17.74 0 0 0-16.41-6.747 23.65 23.65 0 0 0 1.75 10.24 23.66 23.66 0 0 1 18.102 9.23"
    />
  </svg>
</template>

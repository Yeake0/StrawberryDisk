<script setup lang="ts">
defineProps<{ size: number }>();
</script>

<template>
  <svg :width="size" :height="size" viewBox="-64 -64 1152 1152" fill="currentColor" aria-hidden="true">
    <path
      d="M128 192v640l284.8-640H128z m483.2 0L896 832V192h-284.8zM512 428.8L387.2 704h131.104l51.2 128H691.2L512 428.8z"
    />
  </svg>
</template>

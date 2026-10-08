<script setup lang="ts">
withDefaults(
  defineProps<{
    size?: 'small' | 'medium';
  }>(),
  {
    size: 'medium',
  }
);
</script>

<template>
  <span class="md-spinner md-operational-motion" :class="`md-spinner--${size}`" aria-hidden="true" />
</template>

<style scoped>
.md-spinner {
  display: inline-block;
  flex: none;
  border: 2px solid currentColor;
  border-right-color: transparent;
  border-radius: 999px;
  animation: md-spinner-rotation 0.8s linear infinite;
}

.md-spinner--small {
  width: 13px;
  height: 13px;
}

.md-spinner--medium {
  width: 15px;
  height: 15px;
}

@keyframes md-spinner-rotation {
  to {
    transform: rotate(360deg);
  }
}
</style>

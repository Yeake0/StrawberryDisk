<script setup lang="ts">
import { useI18n } from 'vue-i18n';
import { ThumbsUp, ThumbsDown } from '@lucide/vue';
import { Button } from '@/components/ui/button';
import type { AiFeedbackState, AiFeedbackRating } from '@/lib/models/ai';
const { t } = useI18n({ useScope: 'global' });
defineProps<{ feedback: AiFeedbackState; disabled?: boolean }>();
const emit = defineEmits<{ rate: [rating: AiFeedbackRating] }>();
</script>

<template>
  <div class="mt-4 grid gap-1" :aria-busy="feedback.busy">
    <div class="flex flex-wrap items-center gap-1" role="group" :aria-label="t('ai.feedback.label')">
      <Button
        v-for="rating in ['positive', 'negative'] as const"
        :key="rating"
        variant="ghost"
        :class="feedback.rating === rating ? 'bg-accent text-accent-foreground' : ''"
        size="sm"
        class="h-8 gap-1.5 px-2 text-xs"
        :aria-pressed="feedback.rating === rating"
        :disabled="disabled || feedback.busy || feedback.error === 'expired'"
        @click="emit('rate', rating)"
      >
        <component :is="rating === 'positive' ? ThumbsUp : ThumbsDown" class="size-3.5" aria-hidden="true" />
        {{ t(rating === 'positive' ? 'ai.feedback.positive' : 'ai.feedback.negative') }}
      </Button>
    </div>
    <p v-if="feedback.error" role="alert" class="text-xs text-muted-foreground">
      {{ t(feedback.error === 'expired' ? 'ai.feedback.expired' : 'ai.feedback.failed') }}
    </p>
    <span v-else-if="feedback.rating && !feedback.busy" role="status" class="sr-only">{{
      t('ai.feedback.saved')
    }}</span>
  </div>
</template>

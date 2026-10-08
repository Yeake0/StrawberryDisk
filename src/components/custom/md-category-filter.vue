<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import MdIcon from '@/components/icons/md-icon.vue';
import { ICON_NAMES } from '@/lib/models/ui';

const props = defineProps<{
  accessibilityLabel: string;
  disabled?: boolean;
  modelValue: string;
  options: Array<{ value: string; label: string; count?: number }>;
  showScrollButtons?: boolean;
  previousLabel?: string;
  nextLabel?: string;
}>();

const emit = defineEmits<{
  'update:modelValue': [value: string];
}>();

const filter = ref<HTMLElement | null>(null);
const canScrollStart = ref(false);
const canScrollEnd = ref(false);
let resizeObserver: ResizeObserver | undefined;
let overflowFrame: number | undefined;

function updateOverflowState() {
  const element = filter.value;
  if (!element) return;
  canScrollStart.value = element.scrollLeft > 1;
  canScrollEnd.value = element.scrollLeft + element.clientWidth < element.scrollWidth - 1;
}

function scheduleOverflowUpdate() {
  if (overflowFrame !== undefined) window.cancelAnimationFrame(overflowFrame);
  overflowFrame = window.requestAnimationFrame(() => {
    overflowFrame = undefined;
    updateOverflowState();
  });
}

function revealActiveOption(behavior: 'auto' | 'smooth') {
  void nextTick(() => {
    const activeOption = filter.value?.querySelector<HTMLElement>('[data-active="true"]');
    activeOption?.scrollIntoView({ behavior, block: 'nearest', inline: 'nearest' });
    scheduleOverflowUpdate();
  });
}

function selectOption(value: string) {
  emit('update:modelValue', value);
}

function scrollCategories(direction: -1 | 1) {
  const element = filter.value;
  if (!element) return;
  element.scrollBy({ left: direction * Math.max(120, element.clientWidth * 0.7), behavior: 'smooth' });
}

watch([() => props.modelValue, () => props.options], () => revealActiveOption('smooth'), { flush: 'post' });

onMounted(() => {
  const element = filter.value;
  if (!element) return;
  // Sidebar animation changes the available toolbar width over several frames.
  // Keeping the selected option visible prevents the search field from seeming
  // to cover a category when the navigation expands.
  resizeObserver = new ResizeObserver(() => revealActiveOption('auto'));
  resizeObserver.observe(element);
  revealActiveOption('auto');
});

onBeforeUnmount(() => {
  resizeObserver?.disconnect();
  if (overflowFrame !== undefined) window.cancelAnimationFrame(overflowFrame);
});
</script>

<template>
  <div class="category-filter-container flex min-w-0 items-center gap-1">
    <button
      v-if="showScrollButtons"
      type="button"
      class="category-filter-arrow"
      :aria-label="previousLabel"
      :disabled="disabled || !canScrollStart"
      @click="scrollCategories(-1)"
    >
      <MdIcon :name="ICON_NAMES.chevronLeft" :size="16" />
    </button>
    <nav
      ref="filter"
      class="category-filter scrollbar-hidden flex min-w-0 items-center gap-1 overflow-x-auto p-0.5"
      :class="{
        'category-filter--overflow-start': canScrollStart,
        'category-filter--overflow-end': canScrollEnd,
      }"
      :aria-label="accessibilityLabel"
      @scroll.passive="scheduleOverflowUpdate"
    >
      <button
        v-for="option in options"
        :key="option.value"
        type="button"
        class="inline-flex h-7.5 flex-none cursor-pointer items-center gap-1.5 rounded-md border border-transparent px-2.5 text-content-body text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/35 disabled:cursor-default disabled:hover:bg-transparent disabled:hover:text-muted-foreground"
        :class="{
          'border-primary/20 bg-primary/10 font-semibold text-primary': modelValue === option.value,
        }"
        :data-active="modelValue === option.value"
        :disabled="disabled"
        @click="selectOption(option.value)"
      >
        <span>{{ option.label }}</span>
        <small
          v-if="option.count !== undefined"
          class="min-w-4 px-0.5 py-0.5 text-center text-content-meta text-muted-foreground"
          :class="{ 'text-primary': modelValue === option.value }"
        >
          {{ option.count }}
        </small>
      </button>
    </nav>
    <button
      v-if="showScrollButtons"
      type="button"
      class="category-filter-arrow"
      :aria-label="nextLabel"
      :disabled="disabled || !canScrollEnd"
      @click="scrollCategories(1)"
    >
      <MdIcon :name="ICON_NAMES.chevronRight" :size="16" />
    </button>
  </div>
</template>

<style scoped>
@reference "@assets/main.css";

.category-filter-arrow {
  @apply flex h-7.5 w-7.5 flex-none items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/35 disabled:cursor-default disabled:opacity-35 disabled:hover:bg-transparent;
}

.category-filter {
  flex: 1;
  scroll-padding-inline: 20px;
  overscroll-behavior-inline: contain;
}

/* Edge fades communicate hidden categories without consuming toolbar space. */
.category-filter--overflow-start:not(.category-filter--overflow-end) {
  -webkit-mask-image: linear-gradient(to right, transparent, var(--foreground) 18px, var(--foreground) 100%);
  mask-image: linear-gradient(to right, transparent, var(--foreground) 18px, var(--foreground) 100%);
}

.category-filter--overflow-end:not(.category-filter--overflow-start) {
  -webkit-mask-image: linear-gradient(to right, var(--foreground) 0, var(--foreground) calc(100% - 18px), transparent);
  mask-image: linear-gradient(to right, var(--foreground) 0, var(--foreground) calc(100% - 18px), transparent);
}

.category-filter--overflow-start.category-filter--overflow-end {
  -webkit-mask-image: linear-gradient(
    to right,
    transparent,
    var(--foreground) 18px,
    var(--foreground) calc(100% - 18px),
    transparent
  );
  mask-image: linear-gradient(
    to right,
    transparent,
    var(--foreground) 18px,
    var(--foreground) calc(100% - 18px),
    transparent
  );
}
</style>

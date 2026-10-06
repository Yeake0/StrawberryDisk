<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, useId, watch } from 'vue';

import MdResultScrollbar from './md-result-scrollbar.vue';

interface Props {
  headerVariant?: 'muted' | 'plain';
  synchronousScroll?: boolean;
}

interface ResultTableScrollOptions {
  top?: number;
  left?: number;
  behavior?: 'auto' | 'smooth';
}

const viewportId = useId();
const scrollElement = ref<HTMLElement | null>(null);
const scrollGutter = ref(0);
let resizeObserver: ResizeObserver | null = null;
let mutationObserver: MutationObserver | null = null;

const props = withDefaults(defineProps<Props>(), {
  headerVariant: 'muted',
  synchronousScroll: false,
});

function handleWheel(event: WheelEvent) {
  const element = scrollElement.value;
  if (!props.synchronousScroll || !element || !event.cancelable || event.defaultPrevented) return;
  // Preserve zoom and horizontal gestures. Wheel deltas already include the
  // platform's trackpad momentum; do not add another easing curve or throttle.
  if (event.ctrlKey || event.metaKey || event.shiftKey || Math.abs(event.deltaX) > Math.abs(event.deltaY)) return;
  const unit = event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? element.clientHeight : 1;
  const maximum = Math.max(0, element.scrollHeight - element.clientHeight);
  const next = Math.max(0, Math.min(maximum, element.scrollTop + event.deltaY * unit));
  // Cancel at the edges too: some engines decide whether the entire momentum
  // gesture is cancelable from its first event, including a direction reversal.
  event.preventDefault();
  if (next === element.scrollTop) return;
  // Async compositor scrolling can expose unrendered space before Vue receives
  // the scroll event. Advance the viewport and notify its virtualizer in the
  // same input task, so the DOM patch completes before the next painted frame.
  setScrollOffset(next);
}

function setScrollOffset(top: number) {
  const element = scrollElement.value;
  if (!element || element.scrollTop === top) return;
  element.scrollTop = top;
  element.dispatchEvent(new Event('scroll'));
}

watch(
  () => [scrollElement.value, props.synchronousScroll] as const,
  ([element, enabled], _, cleanup) => {
    if (!element || !enabled) return;
    element.addEventListener('wheel', handleWheel, { passive: false });
    cleanup(() => element.removeEventListener('wheel', handleWheel));
  },
  { flush: 'sync' }
);

function syncScrollGutter() {
  const element = scrollElement.value;
  if (!element) return;

  // WebView engines disagree on whether a CSS scrollbar gutter contributes to
  // offsetWidth - clientWidth. Measure the rendered content inset directly so
  // the fixed header follows the same left gutter as rows on every platform.
  const firstRow = element.firstElementChild;
  scrollGutter.value = firstRow
    ? Math.max(0, firstRow.getBoundingClientRect().left - element.getBoundingClientRect().left)
    : 0;
}

function scrollTo(options: ResultTableScrollOptions) {
  scrollElement.value?.scrollTo(options);
}

function getScrollElement() {
  return scrollElement.value;
}

onMounted(() => {
  syncScrollGutter();
  resizeObserver = new ResizeObserver(syncScrollGutter);
  if (scrollElement.value) resizeObserver.observe(scrollElement.value);
  mutationObserver = new MutationObserver(syncScrollGutter);
  if (scrollElement.value) mutationObserver.observe(scrollElement.value, { childList: true });
});

onBeforeUnmount(() => {
  resizeObserver?.disconnect();
  mutationObserver?.disconnect();
});

defineExpose({
  scrollTo,
  getScrollElement,
});
</script>

<template>
  <div class="result-table" :style="{ '--result-table-scroll-gutter': `${scrollGutter}px` }">
    <header
      v-if="$slots.header"
      class="result-table-header md-result-header"
      :class="{ 'result-table-header-plain': headerVariant === 'plain' }"
    >
      <slot name="header" />
    </header>
    <div class="result-table-body">
      <div
        :id="viewportId"
        ref="scrollElement"
        class="result-table-scroll"
        :class="synchronousScroll ? 'scrollbar-synchronous' : 'scrollbar-stable'"
      >
        <slot />
      </div>
      <MdResultScrollbar
        v-if="synchronousScroll"
        :viewport="scrollElement"
        :viewport-id="viewportId"
        @scroll="setScrollOffset"
        @wheel="handleWheel"
      />
    </div>
  </div>
</template>

<style scoped>
@reference "@assets/main.css";

.result-table {
  --result-table-content-inline-padding: 12px;
  --result-table-hierarchy-indent: 32px;

  display: flex;
  min-width: 0;
  min-height: 0;
  flex: 1;
  flex-direction: column;
  overflow: hidden;
}

.result-table-header {
  min-width: 0;
  flex: none;
  border-bottom-width: 1px;
  padding-inline: calc(var(--result-table-scroll-gutter) + var(--result-table-content-inline-padding));
}

.result-table-header-plain {
  background: transparent;
}

.result-table-body {
  position: relative;
  display: flex;
  min-width: 0;
  min-height: 0;
  flex: 1;
}

.result-table-scroll {
  min-width: 0;
  min-height: 0;
  flex: 1;
  overflow-x: hidden;
  overscroll-behavior: contain;
}
</style>

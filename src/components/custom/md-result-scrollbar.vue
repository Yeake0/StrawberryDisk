<script setup lang="ts">
import { computed, onDeactivated, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';

const props = defineProps<{ viewport: HTMLElement | null; viewportId: string }>();
const emit = defineEmits<{ scroll: [offset: number] }>();
const { t } = useI18n();
const track = ref<HTMLElement | null>(null);
const viewportHeight = ref(0);
const contentHeight = ref(0);
const offset = ref(0);
const maximum = computed(() => Math.max(0, contentHeight.value - viewportHeight.value));
const thumbHeight = computed(() =>
  Math.min(viewportHeight.value, Math.max(24, viewportHeight.value ** 2 / (contentHeight.value || 1)))
);
const travel = computed(() => Math.max(0, viewportHeight.value - thumbHeight.value));
const thumbTop = computed(() => (maximum.value ? (offset.value / maximum.value) * travel.value : 0));
let drag: { pointerId: number; grabOffset: number } | null = null;

function stopDrag() {
  if (drag && track.value?.hasPointerCapture(drag.pointerId)) track.value.releasePointerCapture(drag.pointerId);
  drag = null;
}
function syncGeometry() {
  const viewport = props.viewport;
  // KeepAlive temporarily detaches the viewport. Retain the visible geometry.
  if (!viewport || viewport.clientHeight === 0) return;
  viewportHeight.value = viewport.clientHeight;
  contentHeight.value = viewport.scrollHeight;
  offset.value = Math.max(0, Math.min(maximum.value, viewport.scrollTop));
  if (!maximum.value) stopDrag();
}
watch(
  () => props.viewport,
  (viewport, _, cleanup) => {
    if (!viewport) return;
    const resize = new ResizeObserver(syncGeometry);
    const observeContent = () => {
      resize.disconnect();
      resize.observe(viewport);
      // Virtual content changes height as groups expand or more rows load.
      for (const child of viewport.children) resize.observe(child);
      syncGeometry();
    };
    const mutation = new MutationObserver(observeContent);
    mutation.observe(viewport, { childList: true });
    viewport.addEventListener('scroll', syncGeometry, { passive: true });
    observeContent();
    cleanup(() => {
      stopDrag();
      resize.disconnect();
      mutation.disconnect();
      viewport.removeEventListener('scroll', syncGeometry);
    });
  },
  { flush: 'post', immediate: true }
);
onDeactivated(stopDrag);

function scrollTo(next: number) {
  // The parent notifies virtual rows synchronously in this pointer/key task,
  // before the browser can paint the new scroll position without its rows.
  emit('scroll', Math.max(0, Math.min(maximum.value, next)));
  syncGeometry();
}
function dragTo(clientY: number) {
  if (!drag || !track.value || travel.value === 0) return;
  const top = clientY - track.value.getBoundingClientRect().top - drag.grabOffset;
  scrollTo((top / travel.value) * maximum.value);
}
function pointerDown(event: PointerEvent) {
  if (event.button !== 0 || !track.value || !maximum.value) return;
  event.preventDefault();
  syncGeometry();
  const position = event.clientY - track.value.getBoundingClientRect().top;
  const onThumb = position >= thumbTop.value && position <= thumbTop.value + thumbHeight.value;
  drag = { pointerId: event.pointerId, grabOffset: onThumb ? position - thumbTop.value : thumbHeight.value / 2 };
  track.value.focus({ preventScroll: true });
  track.value.setPointerCapture(event.pointerId);
  // Clicking the track jumps to that position and can continue as a drag.
  if (!onThumb) dragTo(event.clientY);
}
function pointerMove(event: PointerEvent) {
  if (event.pointerId === drag?.pointerId) dragTo(event.clientY);
}
function pointerEnd(event: PointerEvent) {
  if (event.pointerId === drag?.pointerId) stopDrag();
}
function keyDown(event: KeyboardEvent) {
  const targets: Record<string, number> = {
    ArrowUp: offset.value - 40,
    ArrowDown: offset.value + 40,
    PageUp: offset.value - viewportHeight.value,
    PageDown: offset.value + viewportHeight.value,
    Home: 0,
    End: maximum.value,
  };
  const next = targets[event.key];
  if (next === undefined || event.ctrlKey || event.metaKey || event.altKey) return;
  event.preventDefault();
  scrollTo(next);
}
</script>

<template>
  <div
    v-show="maximum > 0"
    ref="track"
    class="result-scrollbar"
    role="scrollbar"
    tabindex="0"
    :aria-label="t('common.scrollbar')"
    aria-orientation="vertical"
    :aria-controls="viewportId"
    :aria-valuemin="0"
    :aria-valuemax="maximum"
    :aria-valuenow="Math.round(offset)"
    @pointerdown="pointerDown"
    @pointermove="pointerMove"
    @pointerup="pointerEnd"
    @pointercancel="pointerEnd"
    @lostpointercapture="pointerEnd"
    @keydown="keyDown"
  >
    <div class="result-scrollbar-thumb" :style="{ height: `${thumbHeight}px`, top: `${thumbTop}px` }" />
  </div>
</template>

<style scoped>
@reference "@assets/main.css";

.result-scrollbar {
  position: absolute;
  inset-block: 0;
  inset-inline-end: 0;
  width: var(--layout-scrollbar-width);
  touch-action: none;
  user-select: none;
}

.result-scrollbar-thumb {
  position: absolute;
  width: 100%;
  border: 3px solid transparent;
  border-radius: 999px;
  background-clip: padding-box;
  @apply bg-muted-foreground/45;
}

.result-scrollbar:hover .result-scrollbar-thumb,
.result-scrollbar:focus-visible .result-scrollbar-thumb {
  @apply bg-muted-foreground/65;
}
</style>

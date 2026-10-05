<script setup lang="ts">
import { computed, ref, watch } from 'vue';

import MdIcon from '@/components/icons/md-icon.vue';
import type { ApplicationUninstallPlatform } from '@/lib/models/application';
import { ICON_NAMES } from '@/lib/models/ui';
import { ApplicationIconService } from '@/lib/services/application-icon-service';
import { OperatingSystemService } from '@/lib/services/operating-system-service';

const props = withDefaults(
  defineProps<{
    src?: string;
    platform?: ApplicationUninstallPlatform;
    size?: number;
    artworkSize?: number;
  }>(),
  {
    src: '',
    platform: undefined,
    size: 36,
    artworkSize: 0,
  }
);
const emit = defineEmits<{
  error: [];
}>();

const resolvedPlatform = computed<ApplicationUninstallPlatform>(() => {
  if (props.platform) return props.platform;
  if (OperatingSystemService.isLinux()) return 'linuxPackage';
  return OperatingSystemService.isWindows() ? 'windowsRegistry' : 'macosBundle';
});

const failedSource = ref('');
const fallbackSource = ref<string | null>(null);
const primarySource = computed(() => (props.src !== failedSource.value ? props.src : ''));
const imageSource = computed(() => primarySource.value || fallbackSource.value);

watch(
  () => props.src,
  () => {
    failedSource.value = '';
  }
);

watch(
  () => [resolvedPlatform.value, Boolean(primarySource.value)] as const,
  async ([platform, hasPrimary], _, onCleanup) => {
    if (hasPrimary) return;
    let active = true;
    onCleanup(() => {
      active = false;
    });
    fallbackSource.value = ApplicationIconService.peekFallback(platform);
    const resolved = await ApplicationIconService.resolveFallback(platform);
    if (active) fallbackSource.value = resolved ?? fallbackSource.value;
  },
  { immediate: true }
);

function handleImageError() {
  if (primarySource.value) {
    // Keep the fallback local: callers may retain a failed URL until their next scan.
    failedSource.value = primarySource.value;
    emit('error');
  } else {
    // A failed native image must not trigger an endless load/error cycle.
    fallbackSource.value = null;
  }
}

const resolvedArtworkSize = computed(() => {
  if (props.artworkSize > 0) return props.artworkSize;
  // Windows icon resources usually fill their canvas while macOS ICNS artwork includes optical
  // padding. Normalizing only the artwork keeps alignment slots identical across platforms.
  return resolvedPlatform.value === 'windowsRegistry' ? Math.round(props.size * 0.85) : props.size;
});
</script>

<template>
  <span
    class="md-application-icon"
    :class="{
      resolved: Boolean(imageSource),
      'fallback-container': !imageSource,
      'macos-icon': resolvedPlatform === 'macosBundle',
      'windows-icon': resolvedPlatform === 'windowsRegistry',
      'linux-icon': resolvedPlatform === 'linuxPackage',
    }"
    :style="{ width: `${size}px`, height: `${size}px` }"
  >
    <img
      v-if="imageSource"
      :src="imageSource"
      alt=""
      :style="{ width: `${resolvedArtworkSize}px`, height: `${resolvedArtworkSize}px` }"
      @error="handleImageError"
    />
    <span v-else class="fallback-icon" aria-hidden="true">
      <MdIcon
        :name="resolvedPlatform === 'linuxPackage' ? ICON_NAMES.process : ICON_NAMES.application"
        :size="Math.round(size * (resolvedPlatform === 'linuxPackage' ? 0.8 : 0.6))"
      />
    </span>
  </span>
</template>

<style scoped>
@reference "@assets/main.css";

.md-application-icon {
  @apply text-primary;
  background: var(--surface-primary-subtle);
  display: grid;
  flex: none;
  overflow: hidden;
  place-items: center;
  border-radius: calc(var(--radius) - 2px);
}

.md-application-icon.resolved {
  background: transparent;
}

.md-application-icon.fallback-container {
  background: transparent;
}

.md-application-icon.macos-icon {
  background: transparent;
  border-radius: 0;
  overflow: visible;
}

img {
  object-fit: contain;
}

/* Transparent Windows glyphs can match the list surface. A theme-aware contour keeps
 * their silhouette visible before hover without replacing the original artwork colors. */
.windows-icon img {
  filter: drop-shadow(0 0 0 var(--foreground));
}

.fallback-icon {
  display: grid;
  width: 100%;
  height: 100%;
  place-items: center;
}

.linux-icon .fallback-icon {
  color: var(--muted-foreground);
}
</style>

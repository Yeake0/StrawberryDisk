<script setup lang="ts">
import MdTooltip from '@/components/custom/md-tooltip.vue';
import { computed, ref, watch, onMounted, onBeforeUnmount, useId } from 'vue';
import { useI18n } from 'vue-i18n';
import { toast } from 'vue-sonner';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Dialog, DialogTitle, DialogDescription } from '@/components/ui/dialog';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import MdDialogContent from '@/components/custom/md-dialog-content.vue';
import MdDialogHeader from '@/components/custom/md-dialog-header.vue';
import MdDialogFooter from '@/components/custom/md-dialog-footer.vue';
import MdSpinner from '@/components/custom/md-spinner.vue';
import MdNumberField from '@/components/custom/md-number-field.vue';
import MdAiQuotaStatus from '@/components/custom/md-ai-quota-status.vue';
import { isAiServiceUnavailable } from '@/lib/utils/ai-quota';
import MdIcon from '@/components/icons/md-icon.vue';
import { ICON_NAMES } from '@/lib/models/ui';
import { AiService, AiSession } from '@/lib/services/ai-service';
import { LoggerService } from '@/lib/services/logger-service';
import { LinkService } from '@/lib/services/link-service';
import { projectWebsiteUrl } from '@/lib/utils/project-website';
import { aiErrorCode } from '@/lib/utils/ai-error';
import type { AiCustomHeader, AiErrorCode, AiReasoningMode, AiServiceMode, AiQuota } from '@/lib/models/ai';
import { AI_ERROR_LABELS } from '@/lib/models/ai';

const props = defineProps<{ open: boolean; quota?: AiQuota | null }>();
const emit = defineEmits<{
  'update:open': [value: boolean];
  configured: [testError: AiErrorCode | null];
  refreshQuota: [language: string, force: boolean];
}>();
const { t, locale } = useI18n({ useScope: 'global' });
// Each editor owns one notification so disabling AI can remove already-visible
// feedback as well as suppressing callbacks that arrive after unmount.
const notificationId = useId();
let disposed = false;
const guideUrl = computed(() => projectWebsiteUrl(locale.value, '/docs/ai#custom-service'));

async function openGuide() {
  try {
    await LinkService.open(guideUrl.value);
  } catch {
    // Do not include the editor draft or native error: it may contain private configuration.
    LoggerService.warn('ai', 'configuration_guide_open_failed');
    if (!disposed && props.open) toast.error(t('ai.guideOpenFailed'), { id: notificationId });
  }
}
const configured = ref(false);
const mode = ref<AiServiceMode>('free');
const freeAvailable = ref(false);
const freeConsent = ref(false);
const endpoint = ref('');
const model = ref('');
const apiKey = ref('');
const showKey = ref(false);
const customHeaders = ref<AiCustomHeader[]>([]);
// Provider-specific reasoning flags are opt-in; new endpoints must work with
// the standard request shape without silently changing existing preferences.
const reasoning = ref<AiReasoningMode>('default');
const temperature = ref<number | null>(null);
const maxTokens = ref<number | null>(null);
const advanced = ref(false);
const busy = ref(false);
const loading = ref(false);
const loaded = ref(false);
const showFreeQuota = computed(() => props.open && loaded.value && mode.value === 'free' && freeAvailable.value);
// Quota reads are independent of editor initialization: neither a slow network
// nor a failed refresh may delay the entrance animation or resize the form.
watch(showFreeQuota, active => {
  if (active) emit('refreshQuota', locale.value, false);
});
function refreshQuotaAfterFocus() {
  if (showFreeQuota.value) emit('refreshQuota', locale.value, true);
}
onMounted(() => window.addEventListener('focus', refreshQuotaAfterFocus));
const testing = ref(false);
const error = ref<AiErrorCode | null>(null);
let testSession: AiSession | null = null;
let loadRevision = 0;
const canSave = computed(
  () =>
    loaded.value &&
    !loading.value &&
    (mode.value === 'free' ||
      ((temperature.value === null ||
        (Number.isFinite(Number(temperature.value)) &&
          Number(temperature.value) >= 0 &&
          Number(temperature.value) <= 2)) &&
        (maxTokens.value === null ||
          (Number.isSafeInteger(Number(maxTokens.value)) &&
            Number(maxTokens.value) > 0 &&
            Number(maxTokens.value) <= 4294967295)) &&
        customHeaders.value.length <= 8 &&
        customHeaders.value.every(header => header.name.trim().length > 0 && header.value.length > 0) &&
        Boolean(endpoint.value.trim()) &&
        Boolean(model.value.trim())))
);

function addHeader() {
  if (customHeaders.value.length >= 8) return;
  customHeaders.value.push({ name: '', value: '' });
}

function removeHeader(index: number) {
  customHeaders.value.splice(index, 1);
}

watch(
  () => props.open,
  async open => {
    const revision = ++loadRevision;
    showKey.value = false;
    if (!open) {
      apiKey.value = '';
      customHeaders.value = [];
      return;
    }
    loading.value = true;
    advanced.value = false;
    loaded.value = false;
    error.value = null;
    const started = performance.now();
    try {
      const { configuration: settings, freeAvailable: available } = await AiService.editorState();
      // Closing during IPC must not repopulate a hidden editor with a secret.
      if (revision !== loadRevision || !props.open) return;
      configured.value = settings !== null;
      mode.value = settings?.mode ?? 'free';
      freeConsent.value = settings?.freeConsent ?? false;
      freeAvailable.value = available;
      endpoint.value = settings?.endpoint ?? '';
      model.value = settings?.model ?? '';
      apiKey.value = settings?.apiKey ?? '';
      customHeaders.value = settings?.customHeaders?.map(header => ({ ...header })) ?? [];
      reasoning.value = settings?.reasoning ?? 'default';
      temperature.value = settings?.temperature ?? null;
      maxTokens.value = settings?.maxTokens ?? null;
      loaded.value = true;
      LoggerService.info('ai', 'settings_editor_ready', { durationMs: Math.round(performance.now() - started) });
    } catch (cause) {
      if (revision === loadRevision) error.value = aiErrorCode(cause);
    } finally {
      if (revision === loadRevision) loading.value = false;
    }
  },
  { immediate: true }
);

async function save(test: boolean) {
  if (busy.value || !canSave.value) return;
  busy.value = true;
  error.value = null;
  let saved = false;
  try {
    // Free mode changes only the mode in Core. Keep the unsaved custom draft
    // in this editor, and never send it as a replacement for persisted settings.
    await AiService.save(
      mode.value === 'free'
        ? {
            mode: 'free',
            freeConsent: freeConsent.value,
            endpoint: '',
            model: '',
            apiKey: null,
            reasoning: 'default',
          }
        : {
            mode: 'custom',
            freeConsent: freeConsent.value,
            endpoint: endpoint.value,
            model: model.value,
            apiKey: apiKey.value,
            reasoning: reasoning.value,
            temperature: temperature.value,
            maxTokens: maxTokens.value,
            customHeaders: customHeaders.value.map(header => ({ ...header })),
          }
    );
    if (disposed) return;
    configured.value = true;
    showKey.value = false;
    saved = true;
    if (test && mode.value === 'custom') {
      testing.value = true;
      testSession = new AiSession();
      await testSession.run(null, locale.value, () => undefined);
    }
    // Success feedback must not add a form row and move the dialog or its
    // actions. Report test success only after streaming finishes successfully.
    if (!disposed && props.open) toast.success(t(test ? 'ai.connected' : 'ai.saved'), { id: notificationId });
  } catch (cause) {
    error.value = aiErrorCode(cause);
  } finally {
    busy.value = false;
    testSession = null;
    testing.value = false;
    // Publish after the connection-test session releases its reservation;
    // an open explanation panel can then start automatically without Busy.
    if (saved && !disposed) emit('configured', error.value);
  }
}

async function remove() {
  busy.value = true;
  error.value = null;
  try {
    await AiService.delete();
    if (disposed) return;
    configured.value = false;
    loaded.value = true;
    mode.value = 'free';
    freeConsent.value = false;
    emit('configured', null);
    apiKey.value = '';
    customHeaders.value = [];
    endpoint.value = '';
    model.value = '';
    reasoning.value = 'default';
    temperature.value = null;
    maxTokens.value = null;
    advanced.value = false;
    showKey.value = false;
    toast.success(t('ai.deleted'), { id: notificationId });
  } catch (cause) {
    error.value = aiErrorCode(cause);
  } finally {
    busy.value = false;
  }
}

function close(open: boolean) {
  if (busy.value && !testing.value) return;
  if (!open) void cancelTest();
  emit('update:open', open);
}
async function cancelTest() {
  try {
    await testSession?.cancel();
  } catch {
    LoggerService.warn('ai', 'connection_test_cancel_failed');
  }
}
onBeforeUnmount(() => {
  disposed = true;
  toast.dismiss(notificationId);
  window.removeEventListener('focus', refreshQuotaAfterFocus);
  ++loadRevision;
  void cancelTest();
  apiKey.value = '';
  customHeaders.value = [];
});
</script>

<template>
  <!-- Start the entrance only with the final form, not a shorter loading shell.
       Resizing a centered dialog during its animation changes its origin. -->
  <Dialog :open="open && !loading" @update:open="close">
    <!-- close() guards busy actions; keep the header control mounted to avoid
         a visible flash during quick local saves. -->
    <MdDialogContent
      class="flex min-h-0 flex-col"
      @interact-outside.prevent
      @escape-key-down="busy && !testing && $event.preventDefault()"
    >
      <MdDialogHeader class="flex-none border-b border-border/70">
        <DialogTitle>{{ t('ai.settingsTitle') }}</DialogTitle>
        <DialogDescription>{{ t('ai.settingsDescription') }}</DialogDescription>
      </MdDialogHeader>
      <div class="grid min-h-0 gap-4 overflow-y-auto p-5">
        <!-- Native radios communicate a persistent choice and provide arrow-key
             navigation. Selection edits the draft; only Save changes the service. -->
        <fieldset v-if="loaded" class="grid min-w-0 gap-3" :disabled="busy">
          <legend class="sr-only">{{ t('ai.serviceMode') }}</legend>
          <div
            class="min-w-0 rounded-xl border transition-colors"
            :class="mode === 'free' ? 'border-primary/30' : 'border-border/70'"
          >
            <label
              class="flex cursor-pointer items-start gap-3 rounded-t-xl p-4 transition-colors"
              :class="{ 'cursor-default': busy, 'bg-accent/15': mode === 'free' }"
            >
              <input
                v-model="mode"
                type="radio"
                name="ai-service-mode"
                value="free"
                class="peer sr-only"
                aria-labelledby="ai-mode-free-title"
              />
              <!-- WebKit can clip the native radio artwork inside a fixed-size
                   input. Keep native semantics and draw only the visible circle. -->
              <span
                aria-hidden="true"
                class="mt-0.5 grid size-4 shrink-0 place-items-center rounded-full border border-muted-foreground/60 bg-background peer-checked:border-primary peer-checked:bg-primary peer-focus-visible:ring-2 peer-focus-visible:ring-ring/35 peer-focus-visible:ring-offset-2 peer-focus-visible:ring-offset-background peer-disabled:opacity-50"
              >
                <span
                  class="size-1.5 rounded-full bg-primary-foreground"
                  :class="mode === 'free' ? 'opacity-100' : 'opacity-0'"
                />
              </span>
              <span class="grid min-w-0 gap-1">
                <span id="ai-mode-free-title" class="text-sm font-medium">{{ t('ai.freeService') }}</span>
                <span class="text-sm leading-relaxed text-muted-foreground">{{ t('ai.freeNoKey') }}</span>
              </span>
            </label>
            <div
              v-if="mode === 'free'"
              class="grid gap-2 border-t border-border/70 p-4 text-sm leading-relaxed text-muted-foreground"
            >
              <MdAiQuotaStatus :quota="freeAvailable ? quota : null" />
              <p v-if="freeAvailable && isAiServiceUnavailable(quota)">{{ t('ai.freeUseCustom') }}</p>
              <p v-if="!freeAvailable" class="text-xs">{{ t('ai.freeBuildUnavailable') }}</p>
            </div>
          </div>
          <div
            class="min-w-0 rounded-xl border transition-colors"
            :class="mode === 'custom' ? 'border-primary/30' : 'border-border/70'"
          >
            <!-- Keep the help link outside the radio label so opening help never changes the draft mode. -->
            <div class="relative rounded-t-xl transition-colors" :class="{ 'bg-accent/15': mode === 'custom' }">
              <label class="flex cursor-pointer items-start gap-3 p-4" :class="{ 'cursor-default': busy }">
                <input
                  v-model="mode"
                  type="radio"
                  name="ai-service-mode"
                  value="custom"
                  class="peer sr-only"
                  aria-labelledby="ai-mode-custom-title"
                />
                <span
                  aria-hidden="true"
                  class="mt-0.5 grid size-4 shrink-0 place-items-center rounded-full border border-muted-foreground/60 bg-background peer-checked:border-primary peer-checked:bg-primary peer-focus-visible:ring-2 peer-focus-visible:ring-ring/35 peer-focus-visible:ring-offset-2 peer-focus-visible:ring-offset-background peer-disabled:opacity-50"
                >
                  <span
                    class="size-1.5 rounded-full bg-primary-foreground"
                    :class="mode === 'custom' ? 'opacity-100' : 'opacity-0'"
                  />
                </span>
                <span class="grid min-w-0 flex-1 gap-1">
                  <span id="ai-mode-custom-title" class="pr-32 text-sm font-medium">{{ t('ai.customService') }}</span>
                  <span class="text-sm leading-relaxed text-muted-foreground">{{ t('ai.customDescription') }}</span>
                </span>
              </label>
              <MdTooltip :text="t('ai.guideOpenInBrowser')"
                ><a
                  :href="guideUrl"
                  target="_blank"
                  rel="noopener noreferrer"
                  class="absolute top-4 right-4 inline-flex items-center gap-1 rounded-sm text-xs leading-5 text-muted-foreground underline-offset-4 transition-colors hover:text-foreground hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/35"
                  @click.prevent="openGuide"
                >
                  {{ t('ai.configurationGuide') }}
                  <MdIcon :name="ICON_NAMES.external" :size="13" aria-hidden="true" /> </a
              ></MdTooltip>
            </div>
            <div v-if="mode === 'custom'" class="grid min-w-0 gap-4 border-t border-border/70 p-4">
              <div class="grid gap-2">
                <label for="ai-endpoint" class="text-sm font-medium">{{ t('ai.endpoint') }}</label>
                <Input
                  id="ai-endpoint"
                  v-model="endpoint"
                  :disabled="busy"
                  placeholder="https://api.example.com/v1"
                  autocomplete="off"
                  spellcheck="false"
                />
              </div>
              <div class="grid gap-2">
                <label for="ai-model" class="text-sm font-medium">{{ t('ai.model') }}</label>
                <Input
                  id="ai-model"
                  v-model="model"
                  :disabled="busy"
                  placeholder="deepseek/deepseek-v4-flash"
                  autocomplete="off"
                  spellcheck="false"
                />
              </div>
              <div class="grid gap-2">
                <label for="ai-key" class="text-sm font-medium">{{ t('ai.apiKey') }}</label>
                <div class="relative">
                  <Input
                    id="ai-key"
                    v-model="apiKey"
                    class="pr-11"
                    :disabled="busy"
                    :type="showKey ? 'text' : 'password'"
                    :placeholder="t('ai.keyPlaceholder')"
                    autocomplete="new-password"
                    spellcheck="false"
                  />
                  <button
                    type="button"
                    class="absolute inset-y-1 right-1 grid w-8 cursor-pointer place-items-center rounded-md text-muted-foreground transition-colors hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/35 disabled:cursor-not-allowed disabled:opacity-50"
                    :disabled="busy"
                    :aria-label="t(showKey ? 'ai.hideKey' : 'ai.showKey')"
                    :aria-pressed="showKey"
                    @click="showKey = !showKey"
                  >
                    <MdIcon :name="showKey ? ICON_NAMES.eyeOff : ICON_NAMES.eye" :size="17" />
                  </button>
                </div>
              </div>
              <button
                type="button"
                class="flex items-center gap-2 rounded-md py-1 text-sm text-muted-foreground transition-colors hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/35"
                :aria-expanded="advanced"
                aria-controls="ai-advanced-settings"
                :disabled="busy"
                @click="advanced = !advanced"
              >
                <MdIcon :name="advanced ? ICON_NAMES.chevronUp : ICON_NAMES.chevronDown" :size="16" />
                {{ t('ai.advancedSettings') }}
              </button>
              <div v-if="advanced" id="ai-advanced-settings" class="grid gap-4">
                <div class="grid gap-2">
                  <label for="ai-reasoning" class="text-sm font-medium">{{ t('ai.reasoning') }}</label>
                  <Select v-model="reasoning" :disabled="busy">
                    <SelectTrigger id="ai-reasoning" class="w-full"
                      ><SelectValue>{{
                        t(reasoning === 'default' ? 'ai.reasoningDefault' : 'ai.reasoningDisabled')
                      }}</SelectValue></SelectTrigger
                    >
                    <SelectContent>
                      <SelectItem value="default">{{ t('ai.reasoningDefault') }}</SelectItem>
                      <SelectItem value="disabled">{{ t('ai.reasoningDisabled') }}</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
                <div class="grid grid-cols-2 gap-3">
                  <div class="grid gap-2">
                    <div class="flex items-center gap-1.5">
                      <label for="ai-temperature" class="text-sm font-medium">{{ t('ai.temperature') }}</label>
                      <MdTooltip :text="t('ai.temperatureHint')">
                        <button type="button" class="md-help-action" :aria-label="t('ai.temperatureHint')">
                          <MdIcon :name="ICON_NAMES.help" :size="14" />
                        </button>
                      </MdTooltip>
                    </div>
                    <MdNumberField
                      id="ai-temperature"
                      v-model="temperature"
                      :label="t('ai.temperature')"
                      :min="0"
                      :max="2"
                      :step="0.1"
                      :step-snapping="false"
                      :placeholder="t('ai.providerDefault')"
                      :disabled="busy"
                    />
                  </div>
                  <div class="grid gap-2">
                    <label for="ai-max-tokens" class="text-sm font-medium">{{ t('ai.maxTokens') }}</label>
                    <MdNumberField
                      id="ai-max-tokens"
                      v-model="maxTokens"
                      :label="t('ai.maxTokens')"
                      :min="1"
                      :max="4294967295"
                      :step="1"
                      :placeholder="t('ai.providerDefault')"
                      :disabled="busy"
                    />
                  </div>
                </div>
                <div class="grid gap-2">
                  <div class="flex items-center justify-between gap-2">
                    <div class="flex items-center gap-1.5">
                      <span class="text-sm font-medium">{{ t('ai.customHeaders') }}</span>
                      <MdTooltip :text="t('ai.customHeadersHint')">
                        <button type="button" class="md-help-action" :aria-label="t('ai.customHeadersHint')">
                          <MdIcon :name="ICON_NAMES.help" :size="14" />
                        </button>
                        <template #content>
                          <div class="grid gap-2 leading-relaxed">
                            <p>{{ t('ai.customHeadersHint') }}</p>
                            <dl class="grid grid-cols-[auto_minmax(0,1fr)] gap-x-2 gap-y-1">
                              <dt><code v-text="'{{uuid}}'" />:</dt>
                              <dd>{{ t('ai.headerUuidHint') }}</dd>
                              <dt><code v-text="'{{timestamp}}'" />:</dt>
                              <dd>{{ t('ai.headerTimestampHint') }}</dd>
                            </dl>
                          </div>
                        </template>
                      </MdTooltip>
                    </div>
                    <Button variant="ghost" size="sm" :disabled="busy || customHeaders.length >= 8" @click="addHeader">
                      {{ t('ai.addHeader') }}
                    </Button>
                  </div>
                  <div v-for="(header, index) in customHeaders" :key="index" class="flex min-w-0 items-center gap-2">
                    <label :for="`ai-header-name-${index}`" class="sr-only"
                      >{{ t('ai.headerName') }} {{ index + 1 }}</label
                    >
                    <Input
                      :id="`ai-header-name-${index}`"
                      v-model="header.name"
                      class="min-w-0 flex-1"
                      :placeholder="t('ai.headerName')"
                      :disabled="busy"
                      autocomplete="off"
                      spellcheck="false"
                    />
                    <label :for="`ai-header-value-${index}`" class="sr-only"
                      >{{ t('ai.headerValue') }} {{ index + 1 }}</label
                    >
                    <Input
                      :id="`ai-header-value-${index}`"
                      v-model="header.value"
                      class="min-w-0 flex-1"
                      type="text"
                      :placeholder="t('ai.headerValue')"
                      :disabled="busy"
                      autocomplete="new-password"
                      spellcheck="false"
                    />
                    <Button
                      variant="ghost"
                      size="sm"
                      :disabled="busy"
                      :aria-label="t('ai.removeHeader')"
                      @click="removeHeader(index)"
                    >
                      {{ t('ai.removeHeader') }}
                    </Button>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </fieldset>
        <p v-if="error" role="alert" class="text-sm text-destructive">{{ t(AI_ERROR_LABELS[error]) }}</p>
      </div>
      <MdDialogFooter class="flex-wrap" align="between">
        <Button v-if="testing" variant="ghost" @click="cancelTest">{{ t('ai.stop') }}</Button>
        <Button v-else variant="ghost" :disabled="loading || busy || (!configured && !error)" @click="remove">{{
          t('ai.deleteConfiguration')
        }}</Button>
        <div class="flex flex-wrap items-center gap-2">
          <!-- Local saves need no flashing spinner; reserve the same space for
               the slower connection test so its indicator never shifts actions. -->
          <span class="grid size-4 place-items-center"><MdSpinner v-if="testing" /></span>
          <Button v-if="mode === 'custom'" variant="outline" :disabled="busy || !canSave" @click="save(true)">{{
            t('ai.saveAndTest')
          }}</Button>
          <Button :disabled="busy || !canSave" @click="save(false)">{{ t('ai.save') }}</Button>
        </div>
      </MdDialogFooter>
    </MdDialogContent>
  </Dialog>
</template>

<script setup lang="ts">
import MdTooltip from '@/components/custom/md-tooltip.vue';
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';

import MdDialogContent from '@/components/custom/md-dialog-content.vue';
import MdDialogFooter from '@/components/custom/md-dialog-footer.vue';
import MdDialogHeader from '@/components/custom/md-dialog-header.vue';
import MdIconAction from '@/components/custom/md-icon-action.vue';
import MdSpinner from '@/components/custom/md-spinner.vue';
import MdIcon from '@/components/icons/md-icon.vue';
import { Button } from '@/components/ui/button';
import MdCheckbox from '@/components/custom/md-checkbox.vue';
import { Dialog, DialogDescription, DialogTitle } from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { PROJECT_LINKS } from '@/lib/models/application-shell';
import {
  FEEDBACK_CATEGORY_IDS,
  FEEDBACK_LIMITS,
  type FeedbackCategory,
  type StagedFeedbackAttachment,
} from '@/lib/models/feedback';
import { LOG_DOMAINS, LOG_EVENTS } from '@/lib/models/telemetry';
import { ICON_NAMES } from '@/lib/models/ui';
import { ClipboardService } from '@/lib/services/clipboard-service';
import { DroppedAttachmentError, FeedbackService } from '@/lib/services/feedback-service';
import { FileManagerService } from '@/lib/services/file-manager-service';
import { LinkService } from '@/lib/services/link-service';
import { LoggerService } from '@/lib/services/logger-service';
import { NativeDragDropService, type NativeDragDropEvent } from '@/lib/services/native-drag-drop-service';
import {
  extractPastedFeedbackImages,
  feedbackContentLength,
  isPreviewableFeedbackImage,
  resolveFeedbackFileType,
  validateFeedback,
} from '@/lib/utils/feedback';

const CATEGORY_LABEL_KEYS: Readonly<Record<FeedbackCategory, string>> = {
  issue: 'settings.feedbackDialog.categories.issue',
  suggestion: 'settings.feedbackDialog.categories.suggestion',
  other: 'settings.feedbackDialog.categories.other',
};
const VALIDATION_ERROR_KEYS = {
  contentTooShort: 'settings.feedbackDialog.errors.contentTooShort',
  contentTooLong: 'settings.feedbackDialog.errors.contentTooLong',
  invalidEmail: 'settings.feedbackDialog.errors.invalidEmail',
} as const;
const COPY_CONFIRMATION_DURATION_MS = 1000;

interface VisibleAttachment extends StagedFeedbackAttachment {
  previewUrl: string | null;
}

const props = defineProps<{ open: boolean }>();
const emit = defineEmits<{
  'update:open': [value: boolean];
  error: [error: unknown];
}>();
const { locale, t } = useI18n({ useScope: 'global' });

const category = ref<FeedbackCategory>(FEEDBACK_CATEGORY_IDS.issue);
const content = ref('');
const email = ref('');
const includeLogs = ref(true);
const attachments = ref<VisibleAttachment[]>([]);
const fileInput = ref<HTMLInputElement | null>(null);
const addingAttachments = ref(false);
const nativeDropActive = ref(false);
const submitting = ref(false);
const contentErrorKey = ref<string | null>(null);
const emailErrorKey = ref<string | null>(null);
const attachmentErrorKey = ref<string | null>(null);
const submissionErrorKey = ref<string | null>(null);
const submittedId = ref<string | null>(null);
const referenceCopied = ref(false);
const requestId = ref<string | null>(null);
let stopNativeDropListener: (() => void) | null = null;
let nativeDropListenerMounted = false;
let referenceCopyTimer: ReturnType<typeof setTimeout> | null = null;

const remainingAttachmentCount = computed(() => FEEDBACK_LIMITS.attachmentCount - attachments.value.length);
const categoryLabel = computed(() => t(CATEGORY_LABEL_KEYS[category.value]));
const contentLength = computed(() => feedbackContentLength(content.value));
const acceptTypes = 'image/png,image/jpeg,image/webp,application/pdf,application/zip,text/plain,.log,.txt,.zip';

function setCategory(value: unknown) {
  if (typeof value !== 'string' || !Object.values(FEEDBACK_CATEGORY_IDS).includes(value as FeedbackCategory)) return;
  category.value = value as FeedbackCategory;
}

function updateIncludeLogs(value: boolean | 'indeterminate') {
  includeLogs.value = value === true;
}

async function stageFile(originalFile: File) {
  const file = normalizedClipboardFile(originalFile);
  const mimeType = resolveFeedbackFileType(file);
  if (!mimeType) {
    attachmentErrorKey.value = 'settings.feedbackDialog.errors.unsupportedAttachment';
    return;
  }
  if (file.size > FEEDBACK_LIMITS.attachmentBytes) {
    attachmentErrorKey.value = 'settings.feedbackDialog.errors.attachmentTooLarge';
    return;
  }
  try {
    const staged = await FeedbackService.stageAttachment(file, mimeType);
    attachments.value.push({
      ...staged,
      previewUrl: isPreviewableFeedbackImage(staged.mimeType) ? URL.createObjectURL(file) : null,
    });
  } catch (error) {
    handleAttachmentFailure(error);
  }
}

async function addFiles(files: Iterable<File>) {
  const selected = Array.from(files);
  if (!beginAddingAttachments(selected.length)) return;
  try {
    for (const file of selected) await stageFile(file);
  } finally {
    finishAddingAttachments();
  }
}

async function addDroppedPaths(paths: string[]) {
  if (!beginAddingAttachments(paths.length)) return;
  try {
    for (const path of paths) {
      const mimeType = resolveFeedbackFileType({ name: path, type: '' });
      if (!mimeType) {
        attachmentErrorKey.value = 'settings.feedbackDialog.errors.unsupportedAttachment';
        continue;
      }
      try {
        await stageFile(await FeedbackService.readDroppedAttachment(path, mimeType));
      } catch (error) {
        handleAttachmentFailure(error);
      }
    }
  } finally {
    finishAddingAttachments();
  }
}

function beginAddingAttachments(count: number): boolean {
  if (addingAttachments.value || submitting.value || count === 0) return false;
  attachmentErrorKey.value = null;
  if (count > remainingAttachmentCount.value) {
    attachmentErrorKey.value = 'settings.feedbackDialog.errors.tooManyAttachments';
    return false;
  }
  addingAttachments.value = true;
  return true;
}

function finishAddingAttachments() {
  addingAttachments.value = false;
  if (fileInput.value) fileInput.value.value = '';
}

function handleAttachmentFailure(error: unknown) {
  attachmentErrorKey.value =
    error instanceof DroppedAttachmentError && error.issue === 'tooLarge'
      ? 'settings.feedbackDialog.errors.attachmentTooLarge'
      : 'settings.feedbackDialog.errors.attachmentFailed';
  // A directory or oversized file is a normal user-correctable drop result.
  // Keep it local to the dialog instead of also raising a global application
  // error that would make one action appear to have failed twice.
  if (error instanceof DroppedAttachmentError) return;
  LoggerService.error(LOG_DOMAINS.feedback, LOG_EVENTS.feedbackAttachmentStageFailed);
  emit('error', error);
}

function normalizedClipboardFile(file: File): File {
  if (file.name) return file;
  const extension = file.type === 'image/jpeg' ? 'jpg' : file.type.split('/')[1] || 'png';
  return new File([file], `clipboard-image.${extension}`, { type: file.type });
}

function onFileInput(event: Event) {
  const input = event.target as HTMLInputElement;
  if (input.files) void addFiles(input.files);
}

function onPaste(event: ClipboardEvent) {
  const files = extractPastedFeedbackImages(event.clipboardData);
  if (files.length === 0) return;
  event.preventDefault();
  event.stopPropagation();
  void addFiles(files);
}

function onDrop(event: DragEvent) {
  event.preventDefault();
  event.stopPropagation();
  if (event.dataTransfer?.files) void addFiles(event.dataTransfer.files);
}

function handleNativeDrop(event: NativeDragDropEvent) {
  if (!props.open || submittedId.value) {
    nativeDropActive.value = false;
    return;
  }
  if (event.type === 'leave') {
    nativeDropActive.value = false;
    return;
  }
  nativeDropActive.value = event.type !== 'drop';
  if (event.type === 'drop') void addDroppedPaths(event.paths);
}

async function removeAttachment(item: VisibleAttachment) {
  attachments.value = attachments.value.filter(candidate => candidate.token !== item.token);
  revokePreview(item);
  try {
    await FeedbackService.discardAttachments([item.token]);
  } catch (error) {
    LoggerService.warn(LOG_DOMAINS.feedback, LOG_EVENTS.feedbackAttachmentDiscardFailed);
    emit('error', error);
  }
}

async function submit() {
  contentErrorKey.value = null;
  emailErrorKey.value = null;
  submissionErrorKey.value = null;
  const validationIssue = validateFeedback(content.value, email.value);
  if (validationIssue) {
    if (validationIssue === 'invalidEmail') emailErrorKey.value = VALIDATION_ERROR_KEYS[validationIssue];
    else contentErrorKey.value = VALIDATION_ERROR_KEYS[validationIssue];
    return;
  }

  submitting.value = true;
  requestId.value ??= crypto.randomUUID();
  try {
    const result = await FeedbackService.submit({
      requestId: requestId.value,
      category: category.value,
      content: content.value.trim(),
      email: email.value.trim() || null,
      locale: locale.value,
      includeLogs: includeLogs.value,
      attachmentTokens: attachments.value.map(item => item.token),
    });
    submittedId.value = result.id;
    attachments.value.forEach(revokePreview);
    attachments.value = [];
    LoggerService.info(LOG_DOMAINS.feedback, LOG_EVENTS.feedbackSubmissionCompleted);
  } catch {
    submissionErrorKey.value = 'settings.feedbackDialog.errors.submitFailed';
    LoggerService.error(LOG_DOMAINS.feedback, LOG_EVENTS.feedbackSubmissionFailed);
  } finally {
    submitting.value = false;
  }
}

async function openGitHub() {
  try {
    await LinkService.open(PROJECT_LINKS.issues);
  } catch (error) {
    emit('error', error);
  }
}

async function openApplicationLogs() {
  try {
    await FileManagerService.openApplicationLogs();
  } catch (error) {
    emit('error', error);
  }
}

async function copyReference() {
  if (!submittedId.value) return;
  try {
    await ClipboardService.writeText(submittedId.value);
    referenceCopied.value = true;
    if (referenceCopyTimer) clearTimeout(referenceCopyTimer);
    referenceCopyTimer = setTimeout(() => {
      referenceCopied.value = false;
      referenceCopyTimer = null;
    }, COPY_CONFIRMATION_DURATION_MS);
  } catch (error) {
    emit('error', error);
  }
}

function updateOpen(value: boolean) {
  if (value) {
    emit('update:open', true);
    return;
  }
  // The request may already have reached the server. Closing at this point
  // would hide its terminal result and invite an accidental duplicate action.
  // Staging writes the selected bytes before returning a draft token. Closing
  // during that short window would reset the form before the token can be
  // discarded, leaving a hidden attachment in the native draft store.
  if (submitting.value || addingAttachments.value) return;
  const tokens = attachments.value.map(item => item.token);
  attachments.value.forEach(revokePreview);
  resetForm();
  emit('update:open', false);
  void FeedbackService.discardAttachments(tokens).catch(error => {
    LoggerService.warn(LOG_DOMAINS.feedback, LOG_EVENTS.feedbackAttachmentDiscardFailed);
    emit('error', error);
  });
}

function preventOutsideDismiss(event: Event) {
  event.preventDefault();
}

function resetForm() {
  category.value = FEEDBACK_CATEGORY_IDS.issue;
  content.value = '';
  email.value = '';
  includeLogs.value = true;
  attachments.value = [];
  contentErrorKey.value = null;
  emailErrorKey.value = null;
  attachmentErrorKey.value = null;
  submissionErrorKey.value = null;
  submittedId.value = null;
  referenceCopied.value = false;
  requestId.value = null;
  if (referenceCopyTimer) clearTimeout(referenceCopyTimer);
  referenceCopyTimer = null;
}

function revokePreview(item: VisibleAttachment) {
  if (item.previewUrl) URL.revokeObjectURL(item.previewUrl);
}

onBeforeUnmount(() => {
  nativeDropListenerMounted = false;
  if (referenceCopyTimer) clearTimeout(referenceCopyTimer);
  stopNativeDropListener?.();
  stopNativeDropListener = null;
  const tokens = attachments.value.map(item => item.token);
  attachments.value.forEach(revokePreview);
  void FeedbackService.discardAttachments(tokens).catch(() => {
    LoggerService.warn(LOG_DOMAINS.feedback, LOG_EVENTS.feedbackAttachmentDiscardFailed);
  });
});

watch(
  () => props.open,
  open => {
    if (!open) nativeDropActive.value = false;
  }
);

watch(content, () => {
  contentErrorKey.value = null;
});

watch(email, () => {
  emailErrorKey.value = null;
});

onMounted(() => {
  nativeDropListenerMounted = true;
  void NativeDragDropService.listen(handleNativeDrop)
    .then(stop => {
      if (nativeDropListenerMounted) stopNativeDropListener = stop;
      else stop();
    })
    .catch(error => {
      LoggerService.error(LOG_DOMAINS.feedback, LOG_EVENTS.feedbackNativeDropUnavailable);
      emit('error', error);
    });
});
</script>

<template>
  <Dialog :open="props.open" @update:open="updateOpen">
    <MdDialogContent
      class="feedback-dialog flex min-h-0 flex-col"
      height="auto"
      :show-close="!submitting && !addingAttachments"
      :size="submittedId ? 'standard' : 'wide'"
      @interact-outside="preventOutsideDismiss"
    >
      <template v-if="submittedId">
        <MdDialogHeader class="feedback-success-heading">
          <DialogTitle>{{ t('settings.feedbackDialog.successTitle') }}</DialogTitle>
        </MdDialogHeader>
        <div class="feedback-success">
          <span class="feedback-success-icon"><MdIcon :name="ICON_NAMES.check" :size="30" /></span>
          <DialogDescription class="feedback-success-description">
            {{ t('settings.feedbackDialog.successDescription') }}
          </DialogDescription>
          <div class="feedback-reference">
            <span>{{ t('settings.feedbackDialog.referenceId', { id: submittedId }) }}</span>
            <MdIconAction
              class="feedback-reference-copy"
              :class="{ copied: referenceCopied }"
              appearance="unstyled"
              :label="
                t(referenceCopied ? 'settings.feedbackDialog.referenceCopied' : 'settings.feedbackDialog.copyReference')
              "
              @click="copyReference"
            >
              <MdIcon :name="referenceCopied ? ICON_NAMES.check : ICON_NAMES.copy" :size="14" />
            </MdIconAction>
            <span class="sr-only" aria-live="polite">
              {{ referenceCopied ? t('settings.feedbackDialog.referenceCopied') : '' }}
            </span>
          </div>
        </div>
        <MdDialogFooter class="feedback-footer">
          <Button type="button" @click="updateOpen(false)">{{ t('common.close') }}</Button>
        </MdDialogFooter>
      </template>

      <template v-else>
        <MdDialogHeader class="feedback-header">
          <DialogTitle>{{ t('settings.feedbackDialog.title') }}</DialogTitle>
          <DialogDescription
            :class="{ 'feedback-submit-error': submissionErrorKey }"
            :role="submissionErrorKey ? 'alert' : undefined"
            aria-live="polite"
          >
            {{ t(submissionErrorKey ?? 'settings.feedbackDialog.description') }}
          </DialogDescription>
        </MdDialogHeader>

        <div class="feedback-body scrollbar-stable">
          <div class="feedback-meta-grid">
            <label class="field-label">
              <span>{{ t('settings.feedbackDialog.categoryLabel') }}</span>
              <Select :model-value="category" :disabled="submitting" @update:model-value="setCategory">
                <SelectTrigger class="w-full font-normal"
                  ><SelectValue>{{ categoryLabel }}</SelectValue></SelectTrigger
                >
                <SelectContent>
                  <SelectItem v-for="value in FEEDBACK_CATEGORY_IDS" :key="value" :value="value">
                    {{ t(CATEGORY_LABEL_KEYS[value]) }}
                  </SelectItem>
                </SelectContent>
              </Select>
            </label>

            <label class="field-label">
              <span class="field-heading">
                <span>{{ t('settings.feedbackDialog.emailLabel') }}</span>
                <MdTooltip v-if="emailErrorKey" :text="t(emailErrorKey)"
                  ><small class="field-error" role="alert">
                    {{ t(emailErrorKey) }}
                  </small></MdTooltip
                >
              </span>
              <Input
                v-model="email"
                class="feedback-email font-normal"
                :class="{ invalid: emailErrorKey }"
                type="email"
                autocomplete="email"
                :aria-invalid="Boolean(emailErrorKey)"
                :disabled="submitting"
                :maxlength="FEEDBACK_LIMITS.emailMaxLength"
                :placeholder="t('settings.feedbackDialog.emailPlaceholder')"
              />
            </label>
          </div>

          <label class="field-label">
            <span class="field-heading">
              <span>{{ t('settings.feedbackDialog.contentLabel') }}</span>
              <MdTooltip v-if="contentErrorKey" :text="t(contentErrorKey)"
                ><small class="field-error" role="alert">
                  {{ t(contentErrorKey) }}
                </small></MdTooltip
              >
              <small class="field-count">{{ contentLength }} / {{ FEEDBACK_LIMITS.contentMaxLength }}</small>
            </span>
            <textarea
              v-model="content"
              class="feedback-textarea"
              :class="{ invalid: contentErrorKey }"
              :aria-invalid="Boolean(contentErrorKey)"
              :disabled="submitting"
              :maxlength="FEEDBACK_LIMITS.contentMaxLength"
              :placeholder="t('settings.feedbackDialog.contentPlaceholder')"
              @paste="onPaste"
            />
          </label>

          <section class="attachment-section">
            <div class="attachment-heading">
              <span class="attachment-copy">
                <strong>{{ t('settings.feedbackDialog.attachmentsLabel') }}</strong>
                <MdTooltip :text="attachmentErrorKey ? t(attachmentErrorKey) : undefined"
                  ><small
                    :class="{ 'field-error': attachmentErrorKey }"
                    :role="attachmentErrorKey ? 'alert' : undefined"
                  >
                    {{
                      attachmentErrorKey
                        ? t(attachmentErrorKey)
                        : t('settings.feedbackDialog.attachmentHint', { count: FEEDBACK_LIMITS.attachmentCount })
                    }}
                  </small></MdTooltip
                >
              </span>
              <Button
                class="attachment-picker-button"
                variant="ghost"
                size="sm"
                type="button"
                :disabled="submitting || addingAttachments || remainingAttachmentCount === 0"
                @click="fileInput?.click()"
              >
                <MdIcon :name="ICON_NAMES.paperclip" :size="15" />
                {{ t('settings.feedbackDialog.chooseAttachments') }}
              </Button>
            </div>
            <input ref="fileInput" class="sr-only" type="file" multiple :accept="acceptTypes" @change="onFileInput" />
            <div
              class="attachment-dropzone"
              :class="{
                empty: attachments.length === 0,
                'attachment-dropzone-active': nativeDropActive,
                invalid: attachmentErrorKey,
              }"
              :aria-invalid="Boolean(attachmentErrorKey)"
              @dragover.prevent
              @dragenter.prevent
              @drop="onDrop"
            >
              <button
                v-if="attachments.length === 0"
                class="attachment-dropzone-action"
                type="button"
                :disabled="submitting || addingAttachments || remainingAttachmentCount === 0"
                @click="fileInput?.click()"
              >
                <MdIcon :name="ICON_NAMES.fileImage" :size="22" />
                <strong>{{ t('settings.feedbackDialog.addAttachments') }}</strong>
              </button>
              <ul v-else class="attachment-list" :class="{ 'scrollbar-stable-end': attachments.length > 3 }">
                <li v-for="item in attachments" :key="item.token">
                  <img v-if="item.previewUrl" :src="item.previewUrl" alt="" />
                  <span v-else class="attachment-file-icon"><MdIcon :name="ICON_NAMES.file" :size="20" /></span>
                  <span class="attachment-name">{{ item.displayName }}</span>
                  <button
                    type="button"
                    :aria-label="t('settings.feedbackDialog.removeAttachment', { name: item.displayName })"
                    :disabled="submitting"
                    @click="removeAttachment(item)"
                  >
                    <MdIcon :name="ICON_NAMES.close" :size="16" />
                  </button>
                </li>
              </ul>
            </div>
          </section>

          <div class="log-option-row">
            <label class="log-option">
              <MdCheckbox :model-value="includeLogs" :disabled="submitting" @update:model-value="updateIncludeLogs" />
              <span class="log-option-copy">
                <strong>{{ t('settings.feedbackDialog.includeLogs') }}</strong>
                <small>{{ t('settings.feedbackDialog.logPrivacyHint') }}</small>
              </span>
            </label>
            <Button class="log-folder-button" variant="ghost" size="sm" type="button" @click="openApplicationLogs">
              <MdIcon :name="ICON_NAMES.folderOpen" :size="15" />
              {{ t('settings.feedbackDialog.openLogFolder') }}
            </Button>
          </div>
        </div>

        <MdDialogFooter class="feedback-footer" align="between">
          <Button variant="ghost" type="button" :disabled="submitting" @click="openGitHub">
            <MdIcon :name="ICON_NAMES.github" :size="17" />
            {{ t('settings.feedbackDialog.githubAction') }}
            <MdIcon :name="ICON_NAMES.external" :size="14" />
          </Button>
          <div class="feedback-primary-actions">
            <Button
              variant="outline"
              type="button"
              :disabled="submitting || addingAttachments"
              @click="updateOpen(false)"
            >
              {{ t('common.cancel') }}
            </Button>
            <Button type="button" :disabled="submitting || addingAttachments" @click="submit">
              <MdSpinner v-if="submitting" size="small" />
              {{ submitting ? t('settings.feedbackDialog.submitting') : t('settings.feedbackDialog.submit') }}
            </Button>
          </div>
        </MdDialogFooter>
      </template>
    </MdDialogContent>
  </Dialog>
</template>

<style scoped>
@reference "@assets/main.css";

.feedback-dialog {
  container-type: inline-size;
}

.feedback-header,
.feedback-success-heading {
  flex: none;
  border-bottom-width: 1px;
  @apply border-border/70;
}

.feedback-body {
  display: grid;
  flex: 1 1 auto;
  min-height: 0;
  align-content: start;
  gap: 15px;
  overflow-y: auto;
  padding: 16px var(--layout-dialog-body-inline-padding);
}

.feedback-meta-grid {
  display: grid;
  grid-template-columns: minmax(0, 0.85fr) minmax(0, 1.15fr);
  gap: 12px;
}

.field-label {
  display: grid;
  gap: 7px;
  font-size: var(--font-content-body);
  font-weight: var(--font-weight-label);
}

.field-heading {
  display: flex;
  min-width: 0;
  align-items: center;
  gap: 7px;
}

.field-heading .field-count {
  flex: none;
  margin-left: auto;
  font-weight: 400;
  font-variant-numeric: tabular-nums;
  @apply text-muted-foreground;
}

.field-error {
  min-width: 0;
  overflow: hidden;
  color: var(--destructive);
  font-size: 11px;
  font-weight: 400;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.feedback-submit-error {
  color: var(--destructive);
}

.feedback-textarea {
  min-height: 108px;
  resize: vertical;
  border-width: 1px;
  border-radius: 7px;
  padding: 10px 12px;
  background: transparent;
  font-size: 14px;
  font-weight: 400;
  line-height: 1.55;
  outline: none;
  @apply border-input shadow-xs transition-[color,box-shadow,border-color] placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/30 disabled:opacity-50;
}

:deep(.feedback-email.invalid),
.feedback-textarea.invalid {
  border-color: var(--destructive);
  box-shadow: none;
}

.attachment-section {
  display: grid;
  gap: 9px;
}

.attachment-heading {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 14px;
}

.attachment-copy {
  display: grid;
  min-width: 0;
  gap: 3px;
}

.attachment-copy strong {
  font-size: var(--font-content-body);
}

.attachment-copy small {
  font-size: 11px;
  font-weight: 400;
  line-height: 1.45;
  @apply text-muted-foreground;
}

.attachment-copy .field-error {
  color: var(--destructive);
}

.attachment-heading :deep(.attachment-picker-button) {
  flex: none;
  height: 32px;
  gap: 6px;
  padding: 0 9px;
  border: 1px solid transparent;
  border-radius: 6px;
  box-shadow: none;
  color: var(--muted-foreground);
  font-size: 12px;
  font-weight: 500;
  transition:
    color 150ms ease,
    background-color 150ms ease,
    border-color 150ms ease;
}

@media (hover: hover) {
  .attachment-heading :deep(.attachment-picker-button:hover) {
    border-color: var(--border-subtle);
    background: var(--surface-muted-subtle);
    color: var(--foreground);
  }
}

.attachment-dropzone {
  min-height: 84px;
  border-width: 1px;
  border-style: dashed;
  border-radius: 9px;
  padding: 10px;
  background: transparent;
  @apply border-border/80 transition-colors;
}

.attachment-dropzone.empty {
  padding: 0;
}

.attachment-dropzone-active {
  border-color: var(--primary);
  background: var(--surface-primary-subtle);
  box-shadow: 0 0 0 2px var(--border-primary-subtle);
}

.attachment-dropzone.invalid {
  border-color: var(--destructive);
  box-shadow: none;
}

.attachment-dropzone-action {
  display: flex;
  width: 100%;
  min-height: 82px;
  align-items: center;
  justify-content: center;
  gap: 8px;
  cursor: pointer;
  border-radius: 8px;
  color: var(--primary);
  transition: background-color 150ms ease;
}

/*
 * Tailwind's opacity modifiers fall back to the full source color when
 * color-mix() is unavailable. Safari 15.6 would therefore paint this entire
 * action solid orange. The semantic surface keeps the same subtle feedback on
 * both legacy and modern WebKit.
 */
@media (hover: hover) {
  .attachment-dropzone-action:hover {
    background: var(--surface-primary-subtle);
  }
}

.attachment-dropzone-action:focus-visible {
  outline: 2px solid var(--focus-ring-subtle);
  outline-offset: -2px;
}

.attachment-dropzone-action:disabled {
  cursor: default;
  opacity: 0.5;
}

.attachment-dropzone-action strong {
  font-size: 13px;
}

.attachment-list {
  display: grid;
  max-height: 138px;
  gap: 6px;
  margin: 0;
  padding: 0;
  overflow-y: auto;
  overscroll-behavior: contain;
  list-style: none;
}

.attachment-list li {
  display: grid;
  grid-template-columns: 34px minmax(0, 1fr) 32px;
  min-height: 42px;
  align-items: center;
  gap: 8px;
  border-width: 1px;
  border-radius: 8px;
  padding: 4px 5px;
  border-color: var(--border-subtle);
  background: var(--background);
}

.attachment-list img,
.attachment-file-icon {
  width: 32px;
  height: 32px;
  border-radius: 6px;
}

.attachment-list img {
  object-fit: cover;
}

.attachment-file-icon {
  display: grid;
  place-items: center;
  @apply text-muted-foreground;
}

.attachment-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 12px;
  font-weight: 500;
}

.attachment-list button {
  display: grid;
  width: 30px;
  height: 30px;
  place-items: center;
  border-radius: 6px;
  color: var(--muted-foreground);
  transition:
    color 150ms ease,
    background-color 150ms ease;
}

@media (hover: hover) {
  .attachment-list button:hover {
    background: var(--surface-destructive-subtle);
    color: var(--destructive);
  }
}

.attachment-list button:focus-visible {
  outline: 2px solid var(--focus-ring-subtle);
  outline-offset: -1px;
}

.log-option-row {
  display: flex;
  min-width: 0;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.log-option {
  display: flex;
  min-width: 0;
  align-items: center;
  gap: 9px;
  cursor: pointer;
}

.log-option-copy {
  display: flex;
  min-width: 0;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 3px;
}

.log-option strong {
  font-size: 12px;
}

.log-option small {
  font-size: 11px;
  font-weight: 400;
  line-height: 1.45;
  @apply text-muted-foreground;
}

.log-option-row :deep(.log-folder-button) {
  flex: none;
  height: 32px;
  gap: 6px;
  padding: 0 9px;
  font-size: 12px;
  font-weight: 500;
  @apply text-muted-foreground hover:bg-muted/60 hover:text-foreground;
}

@container (max-width: 540px) {
  .feedback-meta-grid {
    grid-template-columns: 1fr;
  }

  .attachment-heading {
    align-items: flex-end;
  }

  .log-option-row {
    align-items: flex-start;
  }
}

.feedback-footer {
  flex: none;
  align-items: center;
}

.feedback-primary-actions {
  display: flex;
  gap: 8px;
}

.feedback-primary-actions :deep(button) {
  min-width: 96px;
}

.feedback-success {
  display: grid;
  flex: 1 1 auto;
  min-height: 0;
  place-items: center;
  align-content: center;
  gap: 16px;
  overflow-y: auto;
  padding: 28px var(--layout-dialog-body-inline-padding);
  text-align: center;
}

.feedback-success-icon {
  display: grid;
  width: 64px;
  height: 64px;
  place-items: center;
  border-radius: 50%;
  background: var(--surface-success-subtle);
  @apply text-success-foreground;
}

.feedback-success-description {
  line-height: 1.6;
}

.feedback-reference {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  border-radius: 999px;
  padding: 4px 5px 4px 10px;
  font-family: ui-monospace, monospace;
  font-size: 11px;
  @apply bg-muted text-muted-foreground;
}

:deep(.feedback-reference-copy) {
  display: grid;
  width: 24px;
  height: 24px;
  place-items: center;
  border-radius: 999px;
  @apply text-muted-foreground transition-colors hover:bg-background/70 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50;
}

:deep(.feedback-reference-copy.copied) {
  @apply text-success-foreground;
}
</style>

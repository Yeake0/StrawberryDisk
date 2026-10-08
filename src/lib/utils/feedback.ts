import { FEEDBACK_ACCEPTED_FILE_TYPES, FEEDBACK_LIMITS, type FeedbackAcceptedFileType } from '@/lib/models/feedback';

export type FeedbackValidationIssue = 'contentTooShort' | 'contentTooLong' | 'invalidEmail' | null;

/**
 * Uses UTF-16 code units to match HTML maxlength and the website API's
 * JavaScript validation. A shared definition prevents emoji from displaying a
 * smaller count than the input and server actually enforce.
 */
export function feedbackContentLength(content: string): number {
  return content.trim().length;
}

export function validateFeedback(content: string, email: string): FeedbackValidationIssue {
  const contentLength = feedbackContentLength(content);
  if (contentLength < FEEDBACK_LIMITS.contentMinLength) return 'contentTooShort';
  if (contentLength > FEEDBACK_LIMITS.contentMaxLength) return 'contentTooLong';
  const normalizedEmail = email.trim();
  if (normalizedEmail && !isValidEmail(normalizedEmail)) return 'invalidEmail';
  return null;
}

export function resolveFeedbackFileType(file: Pick<File, 'name' | 'type'>): FeedbackAcceptedFileType | null {
  const declaredType = file.type.toLowerCase();
  if ((FEEDBACK_ACCEPTED_FILE_TYPES as readonly string[]).includes(declaredType)) {
    return declaredType as FeedbackAcceptedFileType;
  }
  const extension = file.name.split('.').pop()?.toLowerCase();
  if (extension === 'png') return 'image/png';
  if (extension === 'jpg' || extension === 'jpeg') return 'image/jpeg';
  if (extension === 'webp') return 'image/webp';
  if (extension === 'pdf') return 'application/pdf';
  if (extension === 'log' || extension === 'txt') return 'text/plain';
  if (extension === 'zip') return 'application/zip';
  return null;
}

export function feedbackFileNameFromPath(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).at(-1) || 'attachment';
}

export function isPreviewableFeedbackImage(mimeType: string): boolean {
  return mimeType.startsWith('image/');
}

/**
 * Extracts pasted images across Chromium and WebView clipboard implementations.
 * Some Windows screenshot tools expose images only through `items`, while other
 * browsers also mirror them into `files`; the stable signature prevents the
 * same clipboard image from being staged twice when both representations exist.
 * The key deliberately ignores byte size and timestamp because WebKit may
 * expose differently encoded representations of the same copied image.
 */
export function extractPastedFeedbackImages(clipboardData: Pick<DataTransfer, 'files' | 'items'> | null): File[] {
  if (!clipboardData) return [];
  const images: File[] = [];
  const signatures = new Set<string>();
  const append = (file: File | null) => {
    if (!file?.type.startsWith('image/')) return;
    const signature = file.name.toLocaleLowerCase() + '\0' + file.type.toLowerCase();
    if (signatures.has(signature)) return;
    signatures.add(signature);
    images.push(file);
  };

  Array.from(clipboardData.files).forEach(append);
  Array.from(clipboardData.items)
    .filter(item => item.kind === 'file' && item.type.startsWith('image/'))
    .forEach(item => append(item.getAsFile()));
  return images;
}

function isValidEmail(value: string): boolean {
  if (value.length > FEEDBACK_LIMITS.emailMaxLength) return false;
  return /^[^\s@]+@[^\s@.]+(?:\.[^\s@.]+)+$/u.test(value);
}

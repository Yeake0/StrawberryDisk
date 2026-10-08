import { describe, expect, it } from 'vitest';

import {
  extractPastedFeedbackImages,
  feedbackContentLength,
  feedbackFileNameFromPath,
  resolveFeedbackFileType,
  validateFeedback,
} from '@/lib/utils/feedback';

describe('feedback validation', () => {
  it('validates trimmed content and optional email', () => {
    expect(validateFeedback(' short ', '')).toBe('contentTooShort');
    expect(validateFeedback('A useful feedback report', '')).toBeNull();
    expect(validateFeedback('A useful feedback report', 'invalid@example')).toBe('invalidEmail');
    expect(validateFeedback('A useful feedback report', 'person@example.com')).toBeNull();
    expect(validateFeedback('A useful feedback report', 'person@team@example.com')).toBe('invalidEmail');
    expect(validateFeedback('A useful feedback report', 'person@example..com')).toBe('invalidEmail');
  });

  it('counts content with the same UTF-16 semantics as maxlength and the website API', () => {
    expect(feedbackContentLength('  report  ')).toBe(6);
    expect(feedbackContentLength('disk 😀')).toBe(7);
  });

  it('infers safe diagnostic file types when the browser omits MIME metadata', () => {
    expect(resolveFeedbackFileType({ name: 'screenshot.PNG', type: '' })).toBe('image/png');
    expect(resolveFeedbackFileType({ name: 'photo.jpeg', type: '' })).toBe('image/jpeg');
    expect(resolveFeedbackFileType({ name: 'capture.webp', type: '' })).toBe('image/webp');
    expect(resolveFeedbackFileType({ name: 'steps.pdf', type: '' })).toBe('application/pdf');
    expect(resolveFeedbackFileType({ name: 'StrawberryDisk.log', type: '' })).toBe('text/plain');
    expect(resolveFeedbackFileType({ name: 'report.zip', type: '' })).toBe('application/zip');
    expect(resolveFeedbackFileType({ name: 'tool.exe', type: '' })).toBeNull();
  });

  it('extracts display names from macOS and Windows native drop paths', () => {
    expect(feedbackFileNameFromPath('/Users/test/Desktop/screenshot.png')).toBe('screenshot.png');
    expect(feedbackFileNameFromPath('C:\\Users\\test\\Desktop\\report.zip')).toBe('report.zip');
    expect(feedbackFileNameFromPath('')).toBe('attachment');
  });

  it('extracts WebView clipboard images from items and deduplicates file mirrors', () => {
    const image = new File(['image'], 'screenshot.png', { type: 'image/png', lastModified: 42 });
    const alternateRepresentation = new File(['different bytes'], 'screenshot.png', {
      type: 'image/png',
      lastModified: 44,
    });
    const text = new File(['text'], 'note.txt', { type: 'text/plain', lastModified: 43 });
    const clipboardData = {
      files: [image, text],
      items: [
        { kind: 'file', type: 'image/png', getAsFile: () => alternateRepresentation },
        { kind: 'string', type: 'text/plain', getAsFile: () => null },
      ],
    } as unknown as Pick<DataTransfer, 'files' | 'items'>;

    expect(extractPastedFeedbackImages(clipboardData)).toEqual([image]);
    expect(extractPastedFeedbackImages(null)).toEqual([]);
  });
});

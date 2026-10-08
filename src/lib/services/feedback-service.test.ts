import { beforeEach, describe, expect, it, vi } from 'vitest';

import { FeedbackService } from '@/lib/services/feedback-service';

const { invokeMock, readFileMock, statMock } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
  readFileMock: vi.fn(),
  statMock: vi.fn(),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }));
vi.mock('@tauri-apps/plugin-fs', () => ({ readFile: readFileMock, stat: statMock }));

describe('FeedbackService', () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockResolvedValue(undefined);
    readFileMock.mockReset();
    statMock.mockReset();
  });

  it('reads a native dropped file only after checking its size', async () => {
    statMock.mockResolvedValue({ isFile: true, size: 4 });
    readFileMock.mockResolvedValue(new Uint8Array([0x89, 0x50, 0x4e, 0x47]));

    const file = await FeedbackService.readDroppedAttachment('C:\\Temp\\screen.png', 'image/png');

    expect(statMock).toHaveBeenCalledWith('C:\\Temp\\screen.png');
    expect(readFileMock).toHaveBeenCalledWith('C:\\Temp\\screen.png');
    expect(file.name).toBe('screen.png');
    expect(file.type).toBe('image/png');
    expect(file.size).toBe(4);
  });

  it('does not read oversized native dropped files', async () => {
    statMock.mockResolvedValue({ isFile: true, size: 10 * 1024 * 1024 + 1 });

    await expect(FeedbackService.readDroppedAttachment('/tmp/large.zip', 'application/zip')).rejects.toMatchObject({
      issue: 'tooLarge',
    });
    expect(readFileMock).not.toHaveBeenCalled();
  });

  it('rejects a dropped directory without reading it', async () => {
    statMock.mockResolvedValue({ isFile: false, size: 0 });

    await expect(FeedbackService.readDroppedAttachment('/tmp/folder', 'text/plain')).rejects.toMatchObject({
      issue: 'unavailable',
    });
    expect(readFileMock).not.toHaveBeenCalled();
  });

  it('stages raw attachment bytes with transport-safe metadata headers', async () => {
    const bytes = new Uint8Array([0x89, 0x50, 0x4e, 0x47]);
    const file = {
      name: '截图.png',
      arrayBuffer: vi.fn(async () => bytes.buffer),
    } as unknown as File;

    await FeedbackService.stageAttachment(file, 'image/png');

    expect(invokeMock).toHaveBeenCalledWith('stage_feedback_attachment', bytes.buffer, {
      headers: {
        'x-strawberrydisk-file-name': '5oiq5Zu-LnBuZw',
        'x-strawberrydisk-mime-type': 'image/png',
      },
    });
  });

  it('skips native cleanup when there are no draft tokens', async () => {
    await FeedbackService.discardAttachments([]);

    expect(invokeMock).not.toHaveBeenCalled();
  });

  it('discards staged drafts and submits the typed request', async () => {
    await FeedbackService.discardAttachments(['draft-1']);
    expect(invokeMock).toHaveBeenCalledWith('discard_feedback_attachments', { tokens: ['draft-1'] });

    const request = {
      requestId: '982c87c2-946a-4c19-ab90-920acc51f52b',
      category: 'issue' as const,
      content: 'A reproducible feedback report',
      email: null,
      locale: 'en-US',
      includeLogs: true,
      attachmentTokens: ['draft-1'],
    };
    await FeedbackService.submit(request);

    expect(invokeMock).toHaveBeenLastCalledWith('submit_feedback', { request });
  });
});

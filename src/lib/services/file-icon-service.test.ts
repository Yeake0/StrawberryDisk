import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock('@tauri-apps/api/core', () => ({
  isTauri: () => false,
  invoke: invokeMock,
}));

import { FileIconService } from '@/lib/services/file-icon-service';

describe('FileIconService', () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('coalesces files from one render pass into a native batch', async () => {
    invokeMock.mockImplementation(async (_command: string, args: { requests: Array<{ path: string }> }) => ({
      assignments: args.requests.map(request => ({ ...request, iconKey: 'ext:batch-test' })),
      assets: [{ iconKey: 'ext:batch-test', dataUrl: 'data:image/png;base64,batch' }],
    }));

    const icons = await Promise.all([
      FileIconService.resolve({ path: '/tmp/batch-test.one', kind: 'file', mode: 'automatic' }),
      FileIconService.resolve({ path: '/tmp/batch-test.two', kind: 'file', mode: 'automatic' }),
    ]);

    expect(invokeMock).toHaveBeenCalledTimes(1);
    expect(invokeMock.mock.calls[0]?.[1].requests).toHaveLength(2);
    expect(icons).toEqual(['data:image/png;base64,batch', 'data:image/png;base64,batch']);
  });

  it('keeps extensionless executable icons distinct in path mode', async () => {
    invokeMock.mockImplementation(async (_command: string, args: { requests: Array<{ path: string }> }) => ({
      assignments: args.requests.map(request => ({ ...request, iconKey: `path:${request.path}` })),
      assets: args.requests.map(request => ({ iconKey: `path:${request.path}`, dataUrl: `icon:${request.path}` })),
    }));
    const first = { path: '/usr/bin/first-icon-test', kind: 'file' as const, mode: 'path' as const };
    const second = { ...first, path: '/usr/bin/second-icon-test' };
    expect(await Promise.all([FileIconService.resolve(first), FileIconService.resolve(second)])).toEqual([
      `icon:${first.path}`,
      `icon:${second.path}`,
    ]);
    expect(invokeMock.mock.calls[0]?.[1].requests).toHaveLength(2);
    expect(await FileIconService.resolve(first)).toBe(`icon:${first.path}`);
    expect(invokeMock).toHaveBeenCalledTimes(1);
  });

  it('reuses a resolved file type without another native call', async () => {
    invokeMock.mockImplementation(async (_command: string, args: { requests: Array<{ path: string }> }) => ({
      assignments: args.requests.map(request => ({ ...request, iconKey: 'ext:sessionpdf' })),
      assets: [{ iconKey: 'ext:sessionpdf', dataUrl: 'data:image/png;base64,pdf' }],
    }));

    const first = await FileIconService.resolve({
      path: '/tmp/first.sessionpdf',
      kind: 'file',
      mode: 'automatic',
    });
    const second = await FileIconService.resolve({
      path: '/tmp/second.sessionpdf',
      kind: 'file',
      mode: 'automatic',
    });

    expect(invokeMock).toHaveBeenCalledTimes(1);
    expect(first).toBe('data:image/png;base64,pdf');
    expect(second).toBe(first);
  });

  it('keeps file and directory requests for the same path distinct', async () => {
    invokeMock.mockImplementation(
      async (_command: string, args: { requests: Array<{ path: string; kind: 'file' | 'directory' }> }) => ({
        assignments: args.requests.map(request => ({
          ...request,
          iconKey: request.kind === 'directory' ? 'kind:folder' : 'kind:file',
        })),
        assets: [
          { iconKey: 'kind:folder', dataUrl: 'data:image/png;base64,folder' },
          { iconKey: 'kind:file', dataUrl: 'data:image/png;base64,file' },
        ],
      })
    );

    const [fileIcon, directoryIcon] = await Promise.all([
      FileIconService.resolve({ path: '/tmp/replaced-item', kind: 'file', mode: 'automatic' }),
      FileIconService.resolve({ path: '/tmp/replaced-item', kind: 'directory', mode: 'automatic' }),
    ]);

    expect(invokeMock).toHaveBeenCalledTimes(1);
    expect(invokeMock.mock.calls[0]?.[1].requests).toHaveLength(2);
    expect(fileIcon).toBe('data:image/png;base64,file');
    expect(directoryIcon).toBe('data:image/png;base64,folder');
  });

  it('reuses successful icons for 24 hours before revalidating', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-07-27T00:00:00Z'));
    invokeMock.mockImplementation(async (_command: string, args: { requests: Array<{ path: string }> }) => ({
      assignments: args.requests.map(request => ({ ...request, iconKey: 'ext:ttl-refresh' })),
      assets: [{ iconKey: 'ext:ttl-refresh', dataUrl: 'data:image/png;base64,ttl' }],
    }));

    const request = { path: '/tmp/example.ttl-refresh', kind: 'file' as const, mode: 'automatic' as const };
    await FileIconService.resolve(request);
    await FileIconService.resolve(request);
    expect(invokeMock).toHaveBeenCalledTimes(1);

    vi.setSystemTime(new Date('2026-07-27T00:05:01Z'));
    await FileIconService.resolve(request);
    expect(invokeMock).toHaveBeenCalledTimes(1);

    vi.setSystemTime(new Date('2026-07-27T23:59:59Z'));
    await FileIconService.resolve(request);
    expect(invokeMock).toHaveBeenCalledTimes(1);

    vi.setSystemTime(new Date('2026-07-28T00:00:00Z'));
    await FileIconService.resolve(request);
    expect(invokeMock).toHaveBeenCalledTimes(2);
  });

  it('keeps expired icons available while deduplicating native revalidation', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-09-16T00:00:00Z'));
    const request = { path: '/tmp/stale-revalidate.exe', kind: 'file' as const, mode: 'automatic' as const };
    const response = (dataUrl: string) => ({
      assignments: [{ ...request, iconKey: 'path:stale-revalidate' }],
      assets: [{ iconKey: 'path:stale-revalidate', dataUrl }],
    });
    invokeMock.mockResolvedValue(response('data:image/png;base64,old'));
    await FileIconService.resolve(request);

    vi.advanceTimersByTime(24 * 60 * 60 * 1000 + 1);
    expect(FileIconService.peek(request)).toBeUndefined();
    expect(FileIconService.peek(request, true)).toBe('data:image/png;base64,old');
    invokeMock.mockResolvedValue(response('data:image/png;base64,new'));
    const first = FileIconService.resolve(request);
    const second = FileIconService.resolve(request);
    expect(first).toBe(second);
    expect(FileIconService.peek(request, true)).toBe('data:image/png;base64,old');
    expect(await first).toBe('data:image/png;base64,new');
    expect(FileIconService.peek(request)).toBe('data:image/png;base64,new');
    expect(invokeMock).toHaveBeenCalledTimes(2);
  });

  it('retains the previous image after a failed refresh and retries after the short ttl', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-09-16T01:00:00Z'));
    const request = { path: '/tmp/stale-failure.exe', kind: 'file' as const, mode: 'automatic' as const };
    invokeMock.mockResolvedValue({
      assignments: [{ ...request, iconKey: 'path:stale-failure' }],
      assets: [{ iconKey: 'path:stale-failure', dataUrl: 'data:image/png;base64,previous' }],
    });
    await FileIconService.resolve(request);
    vi.advanceTimersByTime(24 * 60 * 60 * 1000 + 1);
    invokeMock.mockResolvedValue({ assignments: [], assets: [] });
    await FileIconService.resolve(request);
    expect(FileIconService.peek(request)).toBe('data:image/png;base64,previous');
    await FileIconService.resolve(request);
    expect(invokeMock).toHaveBeenCalledTimes(2);

    vi.advanceTimersByTime(15 * 1000 + 1);
    expect(FileIconService.peek(request)).toBeUndefined();
    expect(FileIconService.peek(request, true)).toBe('data:image/png;base64,previous');
    await FileIconService.resolve(request);
    expect(invokeMock).toHaveBeenCalledTimes(3);
  });

  it('retries transient failures after a short ttl', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-07-27T01:00:00Z'));
    invokeMock.mockResolvedValue({ assignments: [], assets: [] });

    const request = {
      path: '/tmp/example.transient-failure',
      kind: 'file' as const,
      mode: 'automatic' as const,
    };
    expect(await FileIconService.resolve(request)).toBeNull();
    expect(await FileIconService.resolve(request)).toBeNull();
    expect(invokeMock).toHaveBeenCalledTimes(1);

    vi.setSystemTime(new Date('2026-07-27T01:00:16Z'));
    expect(await FileIconService.resolve(request)).toBeNull();
    expect(invokeMock).toHaveBeenCalledTimes(2);
  });

  it('evicts old path-specific icons when the session cache reaches its limit', async () => {
    invokeMock.mockImplementation(
      async (_command: string, args: { requests: Array<{ path: string; kind: 'file' | 'directory' }> }) => ({
        assignments: args.requests.map(request => ({
          ...request,
          iconKey: `path:${request.path}`,
        })),
        assets: args.requests.map(request => ({
          iconKey: `path:${request.path}`,
          dataUrl: `data:image/png;base64,${request.path}`,
        })),
      })
    );

    const requests = Array.from({ length: 2049 }, (_, index) => ({
      path: `/tmp/cache-limit-${index}.exe`,
      kind: 'file' as const,
      mode: 'automatic' as const,
    }));
    await Promise.all(requests.map(request => FileIconService.resolve(request)));
    const callsAfterInitialLoad = invokeMock.mock.calls.length;

    await FileIconService.resolve(requests[0]!);

    expect(callsAfterInitialLoad).toBe(Math.ceil(requests.length / 96));
    expect(invokeMock).toHaveBeenCalledTimes(callsAfterInitialLoad + 1);
  });

  it('reuses one generic system folder icon across directory paths', async () => {
    invokeMock.mockImplementation(async (_command: string, args: { requests: Array<{ path: string }> }) => ({
      assignments: args.requests.map(request => ({ ...request, iconKey: 'kind:folder' })),
      assets: [{ iconKey: 'kind:folder', dataUrl: 'data:image/png;base64,folder-shared' }],
    }));

    const callsBefore = invokeMock.mock.calls.length;
    const first = await FileIconService.resolve({
      path: '/tmp/generic-folder-one',
      kind: 'directory',
      mode: 'generic',
    });
    const secondRequest = {
      path: '/tmp/generic-folder-two',
      kind: 'directory' as const,
      mode: 'generic' as const,
    };

    expect(FileIconService.peek(secondRequest)).toBe(first);
    expect(await FileIconService.resolve(secondRequest)).toBe(first);
    expect(invokeMock.mock.calls.length - callsBefore).toBeLessThanOrEqual(1);
  });
});

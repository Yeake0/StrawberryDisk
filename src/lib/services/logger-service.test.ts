import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { LoggerService } from '@/lib/services/logger-service';

const browserConsole = globalThis.console;
const { isTauriMock, nativeErrorMock, nativeInfoMock, nativeWarnMock } = vi.hoisted(() => ({
  isTauriMock: vi.fn(),
  nativeErrorMock: vi.fn(),
  nativeInfoMock: vi.fn(),
  nativeWarnMock: vi.fn(),
}));

vi.mock('@tauri-apps/api/core', () => ({ isTauri: isTauriMock }));
vi.mock('@tauri-apps/plugin-log', () => ({
  error: nativeErrorMock,
  info: nativeInfoMock,
  warn: nativeWarnMock,
}));

describe('LoggerService', () => {
  beforeEach(() => {
    isTauriMock.mockReset().mockReturnValue(false);
    nativeErrorMock.mockReset().mockResolvedValue(undefined);
    nativeInfoMock.mockReset().mockResolvedValue(undefined);
    nativeWarnMock.mockReset().mockResolvedValue(undefined);
    vi.spyOn(browserConsole, 'info').mockImplementation(() => undefined);
    vi.spyOn(browserConsole, 'warn').mockImplementation(() => undefined);
    vi.spyOn(browserConsole, 'error').mockImplementation(() => undefined);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it('keeps browser previews on the console-only fallback', () => {
    LoggerService.info('application-shell', 'started', { path: 'private-path' });

    expect(browserConsole.info).toHaveBeenCalledWith('[application-shell] started', { path: 'private-path' });
    expect(nativeInfoMock).not.toHaveBeenCalled();
  });

  it('persists useful paths and context in Tauri', () => {
    isTauriMock.mockReturnValue(true);

    LoggerService.error('application-shell', 'operation_failed', { path: 'private-path' });

    expect(nativeErrorMock).toHaveBeenCalledWith(
      '[application-shell] operation_failed context={"path":"private-path"}'
    );
    expect(browserConsole.error).toHaveBeenCalledWith('[application-shell] operation_failed', { path: 'private-path' });
  });
  it('preserves native failure details and excludes explicit secret fields', () => {
    isTauriMock.mockReturnValue(true);
    LoggerService.error('startup', 'change_failed', {
      serviceName: 'WslInstaller',
      error: new Error('access denied (os error 5)'),
      authorization: 'Bearer fixture-secret',
      settings: { apiKey: 'fixture-api-key', password: 'fixture-password' },
    });
    const message = nativeErrorMock.mock.calls[0][0] as string;
    expect(message).toContain('WslInstaller');
    expect(message).toContain('access denied (os error 5)');
    expect(message).not.toContain('fixture-secret');
    expect(message).not.toContain('fixture-api-key');
    expect(message).not.toContain('fixture-password');
  });

  it('handles cyclic errors, big integers and multiline paths without losing the event', () => {
    isTauriMock.mockReturnValue(true);
    const error = new Error('native failure');
    error.cause = error;
    LoggerService.warn('filesystem', 'read_failed', { error, bytes: 42n, path: '/fixture/line\nbreak' });
    const message = nativeWarnMock.mock.calls[0][0] as string;
    expect(message).toContain('[circular]');
    expect(message).toContain('native failure');
    expect(message).toContain('42');
    expect(message).not.toContain('\n');
  });

  it.each([8179, 8180, 8181])('keeps truncated Unicode valid at prefix length %i', prefixLength => {
    isTauriMock.mockReturnValue(true);
    LoggerService.error('filesystem', 'native_failure', { detail: 'x'.repeat(prefixLength) + '🦀'.repeat(30) });
    const message = nativeErrorMock.mock.calls[0][0] as string;
    expect(message).toContain('[filesystem] native_failure');
    expect(message).toContain('[truncated]');
    // Unlike JavaScript JSON.parse, URI encoding rejects unpaired surrogates just as Rust does.
    expect(() => encodeURIComponent(message)).not.toThrow();
    expect(message.length).toBeLessThan(8300);
    if (prefixLength === 8179) expect(message).toContain('🦀');
  });

  it('bounds oversized context and tolerates non-serializable getters', () => {
    isTauriMock.mockReturnValue(true);
    LoggerService.info('scan', 'failed', { detail: 'x'.repeat(20000) });
    expect(nativeInfoMock.mock.calls[0][0]).toContain('[truncated]');
    expect((nativeInfoMock.mock.calls[0][0] as string).length).toBeLessThan(8300);
    LoggerService.info('scan', 'failed', {
      get detail() {
        throw new Error('getter failed');
      },
    });
    expect(nativeInfoMock.mock.calls[1][0]).toContain('[context serialization failed]');
  });
});

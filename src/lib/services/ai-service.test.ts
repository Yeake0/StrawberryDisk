import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { AiClientMetadata, AiDelta } from '@/lib/models/ai';
import { AiService, AiSession } from './ai-service';

const ipc = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({
  invoke: ipc.invoke,
  isTauri: () => false,
  Channel: class {
    onmessage: ((delta: AiDelta) => void) | null = null;
  },
}));

vi.mock('@tauri-apps/api/app', () => ({ getVersion: vi.fn().mockResolvedValue('1.0.9') }));

beforeEach(() => {
  vi.restoreAllMocks();
  ipc.invoke.mockReset();
});

describe('AI IPC sessions', () => {
  it('sends local explanation requests without installation metadata', async () => {
    ipc.invoke.mockImplementation(async command => (command === 'ai_begin' ? 'operation' : {}));
    await new AiSession().run(null, 'pt-BR', vi.fn(), 'local');
    expect(ipc.invoke).toHaveBeenCalledWith(
      'ai_explain',
      expect.objectContaining({
        request: { context: null, language: 'pt-BR' },
        expectedMode: 'local',
      })
    );
    expect(ipc.invoke.mock.calls.find(call => call[0] === 'ai_explain')?.[1]).not.toHaveProperty('metadata');
  });
  it('reserves an ID before streaming and delivers deltas', async () => {
    ipc.invoke.mockImplementation(async (command, args) => {
      if (command === 'ai_begin') return 'operation';
      if (command === 'ai_explain') {
        args.onDelta.onmessage({ kind: 'reasoning', text: 'first' });
        args.onDelta.onmessage({ kind: 'text', text: 'second' });
        return { promptTokens: 3, completionTokens: 4 };
      }
    });
    const onDelta = vi.fn();
    await expect(new AiSession().run(null, 'en-US', onDelta)).resolves.toEqual({
      promptTokens: 3,
      completionTokens: 4,
    });
    expect(onDelta.mock.calls).toEqual([[{ kind: 'reasoning', text: 'first' }], [{ kind: 'text', text: 'second' }]]);
    expect(ipc.invoke).toHaveBeenLastCalledWith('ai_cancel', { id: 'operation' });
  });

  it('cancels before reservation completes without dispatching a paid request', async () => {
    let resolve!: (id: string) => void;
    ipc.invoke.mockImplementation(command =>
      command === 'ai_begin'
        ? new Promise<string>(r => {
            resolve = r;
          })
        : Promise.resolve()
    );
    const session = new AiSession();
    const pending = session.run(null, 'en-US', vi.fn());
    const assertion = expect(pending).rejects.toBe('cancelled');
    await session.cancel();
    resolve('reserved');
    await assertion;
    expect(ipc.invoke.mock.calls.some(call => call[0] === 'ai_explain')).toBe(false);
  });

  it('separates public settings from the secret-bearing editor configuration', async () => {
    ipc.invoke.mockResolvedValue(null);
    await AiService.settings();
    expect(ipc.invoke).toHaveBeenCalledWith('ai_get_settings');
    const state = { configuration: null, freeAvailable: true };
    ipc.invoke.mockResolvedValue(state);
    expect(await AiService.editorState()).toEqual(state);
    expect(ipc.invoke).toHaveBeenLastCalledWith('ai_get_configuration');
    await AiService.configuration();
    expect(ipc.invoke).toHaveBeenLastCalledWith('ai_get_configuration');
  });

  it('lists locally installed AI models through the dedicated IPC command', async () => {
    const fixtures = [{ provider: 'ollama', name: 'llama3', tag: 'latest', installedBytes: 4096 }];
    ipc.invoke.mockResolvedValue(fixtures);
    await expect(AiService.listLocalModels()).resolves.toEqual(fixtures);
    expect(ipc.invoke).toHaveBeenLastCalledWith('ai_list_local_models');
  });
});

it('uses a separate versioned preference contract without touching provider configuration', async () => {
  ipc.invoke.mockResolvedValue({ schemaVersion: 1, enabled: false });
  expect(await AiService.preferences()).toEqual({ schemaVersion: 1, enabled: false });
  expect(ipc.invoke).toHaveBeenLastCalledWith('ai_get_preferences');
  expect(await AiService.setEnabled(false)).toEqual({ schemaVersion: 1, enabled: false });
  expect(ipc.invoke).toHaveBeenLastCalledWith('ai_set_enabled', { enabled: false });
});

it.each([true, false])('dispatches a prepared quota request only when its lifecycle is current: %s', async current => {
  const metadata: AiClientMetadata = {
    installId: 'fixture',
    appVersion: '1.0.9',
    locale: 'en-US',
    distribution: 'installed',
    osVersion: 'unknown',
    timezone: 'UTC',
  };
  let finish!: (value: AiClientMetadata) => void;
  vi.spyOn(AiService, 'metadata').mockImplementationOnce(
    () =>
      new Promise(resolve => {
        finish = resolve;
      })
  );
  const isCurrent = vi.fn(() => true);
  ipc.invoke.mockResolvedValue({ remaining: 10 });
  const pending = AiService.quota('en-US', isCurrent);
  const assertion = current
    ? expect(pending).resolves.toEqual({ remaining: 10 })
    : expect(pending).rejects.toBe('cancelled');
  expect(isCurrent).not.toHaveBeenCalled();
  expect(ipc.invoke).not.toHaveBeenCalled();
  isCurrent.mockReturnValue(current);
  finish(metadata);
  await assertion;
  expect(isCurrent).toHaveBeenCalledTimes(1);
  if (current) expect(ipc.invoke).toHaveBeenCalledWith('ai_get_quota', { metadata });
  else expect(ipc.invoke).not.toHaveBeenCalled();
});

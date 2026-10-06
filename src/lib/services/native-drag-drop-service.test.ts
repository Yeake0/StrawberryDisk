import { beforeEach, describe, expect, it, vi } from 'vitest';

const onDragDropEvent = vi.fn();

vi.mock('@tauri-apps/api/webview', () => ({
  getCurrentWebview: () => ({ onDragDropEvent }),
}));

import { NativeDragDropService, type NativeDragDropEvent } from './native-drag-drop-service';

describe('NativeDragDropService', () => {
  beforeEach(() => {
    onDragDropEvent.mockReset();
  });

  it('forwards the native payload without exposing the Tauri event envelope', async () => {
    let nativeListener: ((event: { payload: NativeDragDropEvent }) => void) | undefined;
    const stop = vi.fn();
    onDragDropEvent.mockImplementation(async listener => {
      nativeListener = listener;
      return stop;
    });
    const listener = vi.fn();
    const unlisten = await NativeDragDropService.listen(listener);
    const payload: NativeDragDropEvent = {
      type: 'drop',
      paths: ['/tmp/example'],
      position: { x: 20, y: 40 } as Extract<NativeDragDropEvent, { type: 'drop' }>['position'],
    };

    nativeListener?.({ payload });

    expect(listener).toHaveBeenCalledWith(payload);
    expect(unlisten).toBe(stop);
  });
});

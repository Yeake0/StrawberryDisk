import type { UnlistenFn } from '@tauri-apps/api/event';
import { getCurrentWebview, type DragDropEvent } from '@tauri-apps/api/webview';

export type NativeDragDropEvent = DragDropEvent;

/**
 * Owns Tauri's native file and folder drag-drop subscription. Tauri intercepts
 * desktop drops before the WebView can create ordinary DOM File objects, so
 * page components must consume this adapter instead of relying only on `drop`.
 */
export class NativeDragDropService {
  static listen(listener: (event: DragDropEvent) => void): Promise<UnlistenFn> {
    return getCurrentWebview().onDragDropEvent(event => listener(event.payload));
  }
}

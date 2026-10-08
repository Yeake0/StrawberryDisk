import { getCurrentWindow } from '@tauri-apps/api/window';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { ExcludedApplication, MemoryReleasePreferences } from '@/lib/models/memory-release';

export class MemoryReleaseService {
  static onFocus(handler: () => void): Promise<UnlistenFn> {
    return getCurrentWindow().onFocusChanged(event => {
      if (event.payload) handler();
    });
  }
  static async showSettings(): Promise<void> {
    await getCurrentWindow().show();
    await getCurrentWindow().setFocus();
  }
  static closeSettings(): Promise<void> {
    return getCurrentWindow().close();
  }
  static preferences(): Promise<MemoryReleasePreferences> {
    return invoke('memory_release_preferences');
  }
  static save(preferences: MemoryReleasePreferences): Promise<MemoryReleasePreferences> {
    return invoke('memory_release_save_preferences', { preferences });
  }
  static applications(): Promise<ExcludedApplication[]> {
    return invoke('memory_release_applications');
  }
  static openSettings(): Promise<void> {
    return invoke('memory_release_open_settings');
  }
  static onPreferences(handler: (preferences: MemoryReleasePreferences) => void): Promise<UnlistenFn> {
    return listen<MemoryReleasePreferences>('memory-release-preferences', event => handler(event.payload));
  }
}

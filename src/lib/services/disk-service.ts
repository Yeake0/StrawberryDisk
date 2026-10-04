import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';

import type { DiskInfo } from '@/lib/models/disk';
import { LOG_DOMAINS, LOG_EVENTS } from '@/lib/models/telemetry';
import { LoggerService } from '@/lib/services/logger-service';

/** Exposes disk enumeration without aggregating unrelated native commands. */
export class DiskService {
  static getSystemDisk(refresh = false): Promise<DiskInfo> {
    return invoke<DiskInfo>('get_system_disk', { refresh });
  }

  static listDisks(): Promise<DiskInfo[]> {
    return invoke<DiskInfo[]>('list_disks');
  }

  /** Refresh active windows cheaply; returning to the app always requests a snapshot. */
  static async watch(refresh: () => Promise<unknown>): Promise<() => void> {
    const current = getCurrentWindow();
    let disposed = false;
    let focused = false;
    let focusObserved = false;
    let pending = false;
    const read = async () => {
      if (disposed || pending) return;
      pending = true;
      try {
        await refresh();
      } catch (error) {
        LoggerService.warn(LOG_DOMAINS.applicationShell, LOG_EVENTS.diskRefreshFailed, { error });
      } finally {
        pending = false;
      }
    };
    const stopFocus = await current.onFocusChanged(event => {
      focusObserved = true;
      focused = event.payload;
      if (focused) void read();
    });
    try {
      const initialFocus = await current.isFocused();
      if (!focusObserved) focused = initialFocus;
    } catch (error) {
      stopFocus();
      throw error;
    }
    const timer = setInterval(() => {
      if (focused) void read();
    }, 15_000);
    return () => {
      disposed = true;
      clearInterval(timer);
      stopFocus();
    };
  }
}

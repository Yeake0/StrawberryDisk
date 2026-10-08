import type { MetricId } from '@/lib/models/system-resources';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';

import type {
  ApplicationQuitStatus,
  MemoryReleaseResult,
  ResidentDestination,
  ResidentDisplayStatus,
  ResidentPreferences,
  ResidentReading,
} from '@/lib/models/resident';

export class ResidentService {
  static displayStatus(): Promise<ResidentDisplayStatus> {
    return invoke('resident_get_display_status');
  }
  static onDisplayStatus(callback: (status: ResidentDisplayStatus) => void): Promise<UnlistenFn> {
    return listen<ResidentDisplayStatus>('resident-display-status', event => callback(event.payload));
  }
  static reading(): Promise<ResidentReading> {
    return invoke('monitoring_get_reading');
  }
  static refresh(): Promise<void> {
    return invoke('monitoring_refresh');
  }
  static releaseMemory(): Promise<MemoryReleaseResult> {
    return invoke('monitoring_release_memory');
  }
  static quitApplication(applicationId: string): Promise<ApplicationQuitStatus> {
    return invoke('monitoring_quit_application', { applicationId });
  }
  static preferences(): Promise<ResidentPreferences> {
    return invoke('resident_get_preferences');
  }
  static savePreferences(preferences: ResidentPreferences): Promise<ResidentPreferences> {
    return invoke('resident_save_preferences', { preferences });
  }
  static catalogue(): Promise<ResidentReading> {
    return invoke('resident_get_catalogue');
  }
  static panelMetric(): Promise<MetricId> {
    return invoke('resident_get_panel_metric');
  }
  static selectMetric(metric: MetricId): Promise<void> {
    return invoke('resident_select_metric', { metric });
  }
  static onPanelMetric(handler: (metric: MetricId) => void): Promise<UnlistenFn> {
    return listen<MetricId>('resident-panel-metric', event => handler(event.payload));
  }
  static autostartEnabled(): Promise<boolean> {
    return invoke('resident_get_autostart');
  }
  static setAutostart(enabled: boolean): Promise<void> {
    return invoke('resident_set_autostart', { enabled });
  }
  static openPanel(): Promise<void> {
    return invoke('resident_open_panel');
  }
  static panelReady(): Promise<void> {
    return invoke('resident_panel_ready');
  }
  static hidePanel(): Promise<void> {
    return invoke('resident_hide_panel');
  }
  static openMain(destination: ResidentDestination): Promise<void> {
    return invoke('resident_open_main', { destination });
  }
  static quit(): Promise<void> {
    return invoke('resident_quit');
  }
  static onReading(handler: (reading: ResidentReading) => void): Promise<UnlistenFn> {
    // Scope high-frequency samples to this window; global listeners also receive
    // events targeted at another window and keep hidden pages updating.
    return getCurrentWindow().listen<ResidentReading>('resident-reading', event => handler(event.payload));
  }
  static onNavigate(handler: (destination: ResidentDestination) => void): Promise<UnlistenFn> {
    return listen<ResidentDestination>('resident-open-page', event => handler(event.payload));
  }
  static async onPanelVisibility(handler: (visible: boolean) => void): Promise<UnlistenFn> {
    const current = getCurrentWindow();
    let receivedEvent = false;
    const dispose = await current.listen<boolean>('resident-panel-visibility', event => {
      receivedEvent = true;
      handler(event.payload);
    });
    try {
      // Showing can precede focus or listener registration. Seed actual visibility,
      // but never let an older query overwrite a newer native show/hide event.
      const visible = await current.isVisible();
      if (!receivedEvent) handler(visible);
      return dispose;
    } catch (error) {
      dispose();
      throw error;
    }
  }
}

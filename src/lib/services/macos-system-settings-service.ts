import { invoke } from '@tauri-apps/api/core';

import type { StartupArtifact } from '@/lib/models/startup';
import { LOG_DOMAINS, LOG_EVENTS } from '@/lib/models/telemetry';
import { LoggerService } from '@/lib/services/logger-service';

/** Opens fixed macOS settings destinations exposed by the native adapter. */
export class MacOsSystemSettingsService {
  static async openLoginItems(artifacts: readonly StartupArtifact[] = []): Promise<void> {
    const context = {
      requestId: crypto.randomUUID(),
      destination: 'macos.loginItems',
      items: artifacts.map(artifact => ({
        itemId: artifact.itemId,
        displayName: artifact.displayName,
        sourceId: artifact.sourceId,
        sourceKind: artifact.sourceKind,
        controlCapability: artifact.controlCapability,
        diagnostics: artifact.diagnostics,
        targetPath: artifact.target.path,
      })),
    };
    LoggerService.info(LOG_DOMAINS.startup, LOG_EVENTS.startupSystemSettingsOpenRequested, context);
    try {
      await invoke<void>('open_macos_login_items_settings');
      // The native command dispatches the settings request; it does not verify record removal.
      LoggerService.info(LOG_DOMAINS.startup, LOG_EVENTS.startupSystemSettingsOpenDispatched, context);
    } catch (error) {
      LoggerService.error(LOG_DOMAINS.startup, LOG_EVENTS.startupSystemSettingsOpenFailed, { ...context, error });
      throw error;
    }
  }
}

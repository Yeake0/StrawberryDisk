import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import { EVENT_NAMES } from '@/lib/models/telemetry';

import type {
  PrivacyBrowserCloseRequest,
  PrivacyBrowserStatusRequest,
  PrivacyBrowserStatusResult,
  PrivacyDetailsPage,
  PrivacyDetailsRequest,
  PrivacyExecutionPlan,
  PrivacyExecutionProgress,
  PrivacyExecutionRequest,
  PrivacyExecutionResult,
  PrivacyExecutionRunRequest,
  PrivacyScanRequest,
  PrivacyScanResult,
  PrivacyScanProgress,
} from '@/lib/models/privacy';
import type { ApplicationCloseBatchResult } from '@/lib/models/application-close';

export class PrivacyService {
  static scan(request: PrivacyScanRequest): Promise<PrivacyScanResult> {
    return invoke<PrivacyScanResult>('scan_privacy', { request });
  }

  static async scanWithProgress(
    request: PrivacyScanRequest,
    handler: (progress: PrivacyScanProgress) => void
  ): Promise<PrivacyScanResult> {
    let unlisten: UnlistenFn | undefined;
    try {
      unlisten = await listen<PrivacyScanProgress>(EVENT_NAMES.privacyScanProgress, event => handler(event.payload));
      return await PrivacyService.scan(request);
    } finally {
      unlisten?.();
    }
  }

  static cancelScan(): Promise<void> {
    return invoke<void>('cancel_privacy_scan');
  }

  static details(request: PrivacyDetailsRequest): Promise<PrivacyDetailsPage> {
    return invoke<PrivacyDetailsPage>('get_privacy_details', { request });
  }

  static prepare(request: PrivacyExecutionRequest): Promise<PrivacyExecutionPlan> {
    return invoke<PrivacyExecutionPlan>('prepare_privacy_execution', { request });
  }

  static closeBrowsers(request: PrivacyBrowserCloseRequest): Promise<ApplicationCloseBatchResult> {
    return invoke<ApplicationCloseBatchResult>('close_privacy_browsers', { request });
  }

  static refreshBrowserStatus(request: PrivacyBrowserStatusRequest): Promise<PrivacyBrowserStatusResult> {
    return invoke<PrivacyBrowserStatusResult>('refresh_privacy_browser_status', { request });
  }

  static execute(request: PrivacyExecutionRunRequest): Promise<PrivacyExecutionResult> {
    return invoke<PrivacyExecutionResult>('execute_privacy', { request });
  }

  static async executeWithProgress(
    request: PrivacyExecutionRunRequest,
    handler: (progress: PrivacyExecutionProgress) => void
  ): Promise<PrivacyExecutionResult> {
    let unlisten: UnlistenFn | undefined;
    try {
      unlisten = await listen<PrivacyExecutionProgress>(EVENT_NAMES.privacyExecutionProgress, event =>
        handler(event.payload)
      );
      return await PrivacyService.execute(request);
    } finally {
      unlisten?.();
    }
  }

  static cancelExecution(): Promise<void> {
    return invoke<void>('cancel_privacy_execution');
  }
}

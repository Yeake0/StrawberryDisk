import type { AppDistribution } from '@/lib/models/app-update';

/** Collected facts, independent of HTTP or IPC encoding. Each caller owns its failure policy. */
export interface ClientRequestMetadata {
  locale: string;
  distribution: AppDistribution;
  installId?: string;
  osVersion?: string;
  timezone?: string;
}

/** Stable desktop metadata shared by update telemetry and official AI signatures. */
export const CLIENT_REQUEST_HEADERS = {
  distribution: 'x-strawberrydisk-distribution',
  installId: 'x-strawberrydisk-install-id',
  locale: 'x-strawberrydisk-locale',
  osVersion: 'x-strawberrydisk-os-version',
  timezone: 'x-strawberrydisk-timezone',
  appVersion: 'x-strawberrydisk-app-version',
  requestId: 'x-request-id',
} as const;

import type { PrivacyItem, PrivacyTimeRange } from './privacy';
import type { ScanRuleResult } from './cleanup';
import type { StartupArtifact } from './startup';
import type { SystemSettingItem, SystemSettingTargetState } from './system-settings';
import type { SystemMaintenanceItem } from './system-maintenance';
import type { LargeFileEntry } from './large-file';
import type { DuplicateEntryDeletePolicy, DuplicateGroupKind } from './duplicate-file';
import type { ApplicationUninstallCandidate, ApplicationUninstallComponentSummary } from './application';
import type { ApplicationIdentityMetadata, ApplicationUninstallInventorySource } from './application';

export type AiReasoningMode = 'default' | 'disabled';
export type AiServiceMode = 'free' | 'local' | 'custom';

export interface LocalAiStatus {
  supported: boolean;
  installed: boolean;
  modelBytes: number;
  downloadBytes: number;
}

export interface LocalAiProgress {
  stage: 'runtime' | 'model' | 'ready';
  downloadedBytes: number;
  totalBytes: number | null;
}

export interface AiCustomHeader {
  name: string;
  value: string;
}

export interface AiPreferences {
  schemaVersion: 1;
  enabled: boolean;
}

export interface AiClientMetadata {
  installId: string;
  appVersion: string;
  locale: string;
  distribution: string;
  osVersion: string;
  timezone: string;
}

export interface AiQuota {
  available: boolean;
  unavailableReason: string | null;
  dailyLimit: number;
  remaining: number;
  cooldownSeconds: number;
  nextAllowedAt: string;
  resetAt: string;
  serverTime: string;
  activeRequests: number;
  maxConcurrentRequests: number;
  policyVersion: string;
  promptVersion: string;
}

export type LocalModelProvider = 'ollama' | 'openAiCompatible';

export interface InstalledLocalModel {
  provider: LocalModelProvider;
  name: string;
  tag: string | null;
  installedBytes: number;
}

export const AI_ERROR_LABELS = {
  disabled: 'ai.errors.disabled',
  freeUnavailable: 'ai.errors.freeUnavailable',
  freeDailyLimit: 'ai.errors.freeDailyLimit',
  freeRateLimited: 'ai.errors.freeRateLimited',
  freeConcurrent: 'ai.errors.freeConcurrent',
  freeClockSkew: 'ai.errors.freeClockSkew',
  freeSignatureInvalid: 'ai.errors.freeSignatureInvalid',
  freeRequestExists: 'ai.errors.freeRequestExists',
  freeArchiveUnavailable: 'ai.errors.freeArchiveUnavailable',
  feedbackExpired: 'ai.feedback.expired',
  localUnsupported: 'ai.errors.localUnsupported',
  localDownloadFailed: 'ai.errors.localDownloadFailed',
  localIntegrityFailed: 'ai.errors.localIntegrityFailed',
  localLaunchFailed: 'ai.errors.localLaunchFailed',
  invalidConfiguration: 'ai.errors.invalidConfiguration',
  invalidContext: 'ai.errors.invalidContext',
  notConfigured: 'ai.errors.notConfigured',
  configurationUnavailable: 'ai.errors.configurationUnavailable',
  busy: 'ai.errors.busy',
  cancelled: 'ai.errors.cancelled',
  unauthorized: 'ai.errors.unauthorized',
  quotaExceeded: 'ai.errors.quotaExceeded',
  modelUnavailable: 'ai.errors.modelUnavailable',
  providerRejected: 'ai.errors.providerRejected',
  connectionFailed: 'ai.errors.connectionFailed',
  timeout: 'ai.errors.timeout',
  invalidStream: 'ai.errors.invalidStream',
  incompleteStream: 'ai.errors.incompleteStream',
  outputLimit: 'ai.errors.outputLimit',
  emptyResponse: 'ai.errors.emptyResponse',
  responseTooLarge: 'ai.errors.responseTooLarge',
} as const;

export interface AiSettings {
  schemaVersion: 2;
  mode: AiServiceMode;
  /** Legacy schema 2 field; free requests no longer require explicit consent. */
  freeConsent: boolean;
  freeAvailable: boolean;
  endpoint: string;
  model: string;
  hasKey: boolean;
  reasoning: AiReasoningMode;
  temperature?: number | null;
  maxTokens?: number | null;
}

export interface AiConfigurationUpdate {
  mode: AiServiceMode;
  freeConsent: boolean;
  endpoint: string;
  model: string;
  apiKey: string | null;
  reasoning: AiReasoningMode;
  temperature?: number | null;
  maxTokens?: number | null;
  customHeaders?: AiCustomHeader[] | null;
}

/** Secret-bearing data is scoped to the settings editor, never the AI store. */
export interface AiConfiguration extends AiConfigurationUpdate {
  schemaVersion: 2;
  apiKey: string;
  customHeaders: AiCustomHeader[];
}

/** One fresh editor read; this secret-bearing snapshot must not be cached. */
export interface AiEditorState {
  configuration: AiConfiguration | null;
  freeAvailable: boolean;
}

/** Descriptive metadata, including original startup locations; never executable actions. */
export interface AiContext {
  schemaVersion: 2;
  platform: 'macos' | 'windows' | 'linux' | 'unknown';
  title: string;
  description: string;
  subject: AiSubject;
}

export type AiStartupEntry = Pick<
  StartupArtifact,
  | 'sourceKind'
  | 'triggers'
  | 'configuredState'
  | 'runtimeState'
  | 'controlCapability'
  | 'diagnostics'
  | 'removalSupported'
> & {
  name: string;
  identity: {
    applicationName: string;
    publisher: string;
    executableName: string;
    executablePath: string;
    configurationPath: string;
    description: string;
    version: string;
    trust: StartupArtifact['trust'];
  };
};

/** Only descriptive file metadata enters an explanation request. */
export type AiFileMetadata = Pick<LargeFileEntry, 'name' | 'path' | 'bytes' | 'modifiedAtMs'>;

export interface AiDuplicateEntry {
  file: AiFileMetadata;
  deletePolicy: DuplicateEntryDeletePolicy;
}

/** Include the requested entry and a bounded, explicitly partial set of other copies. */
export const AI_DUPLICATE_COPY_LIMIT = 31;

/** Each module exposes only facts relevant to its explanation, not its full domain object. */
export type AiSubject =
  | ({
      module: 'applicationUninstall';
      identity?: ApplicationIdentityMetadata;
      installationSources?: ApplicationUninstallInventorySource[];
      executionSupported: boolean;
      catalogActionable: boolean;
      recordRemovalAvailable: boolean;
      selectionKind: 'default' | 'current';
      components: (Pick<ApplicationUninstallComponentSummary, 'kind' | 'risk' | 'bytes'> & { selected: boolean })[];
    } & Pick<
      ApplicationUninstallCandidate,
      | 'platform'
      | 'publisher'
      | 'version'
      | 'applicationPath'
      | 'capability'
      | 'recordState'
      | 'systemKind'
      | 'installerKind'
      | 'executionMode'
      | 'associatedDataComplete'
    >)
  | { module: 'largeFiles'; file: AiFileMetadata }
  | {
      module: 'duplicateFiles';
      kind: DuplicateGroupKind;
      target: AiDuplicateEntry;
      otherCopies: AiDuplicateEntry[];
      omittedCount: number;
    }
  | {
      module: 'cleanup';
      impact: string;
      bytes: number;
      itemCount: number;
      requiresAppClose: boolean;
      scan: Pick<
        ScanRuleResult,
        | 'ruleId'
        | 'risk'
        | 'status'
        | 'available'
        | 'selectable'
        | 'runningProcesses'
        | 'sources'
        | 'sourceCount'
        | 'sourcesTruncated'
      >;
    }
  | ({ module: 'privacy'; timeRange: PrivacyTimeRange } & Pick<
      PrivacyItem,
      | 'kind'
      | 'impact'
      | 'capability'
      | 'recommendation'
      | 'itemCount'
      | 'estimatedBytes'
      | 'requiresBrowserClose'
      | 'synchronizationMayPropagate'
    >)
  | { module: 'startup'; entries: AiStartupEntry[]; omittedCount: number }
  | ({
      module: 'systemOptimization';
      pendingTarget: SystemSettingTargetState | null;
      hasRecordedOriginalValue: boolean;
    } & Pick<
      SystemSettingItem,
      'status' | 'riskLevel' | 'requiresRestart' | 'requiresElevation' | 'selectionKind' | 'diagnostic'
    >)
  | ({ module: 'systemMaintenance' } & Pick<
      SystemMaintenanceItem,
      | 'taskId'
      | 'status'
      | 'riskLevel'
      | 'requiresRestart'
      | 'requiresElevation'
      | 'estimatedDurationSeconds'
      | 'diagnostic'
    >);

/** Provider output channels remain distinct through streaming and caching. */
export type AiDelta = { kind: 'text' | 'reasoning'; text: string };

export type AiFeedbackRating = 'positive' | 'negative';
export interface AiFeedbackTarget {
  schemaVersion: 1;
  requestId: string;
}
export interface AiFeedback extends AiFeedbackTarget {
  rating: AiFeedbackRating | null;
  updatedAt: number | null;
}
export interface AiFeedbackState extends AiFeedbackTarget {
  rating: AiFeedbackRating | null;
  busy: boolean;
  error: 'failed' | 'expired' | null;
}

export interface AiUsage {
  feedback?: AiFeedbackTarget;
  promptTokens: number | null;
  completionTokens: number | null;
}

export const AI_ERROR_CODES = [
  'disabled',
  'freeUnavailable',
  'freeDailyLimit',
  'freeRateLimited',
  'freeConcurrent',
  'freeClockSkew',
  'freeSignatureInvalid',
  'freeRequestExists',
  'freeArchiveUnavailable',
  'feedbackExpired',
  'localUnsupported',
  'localDownloadFailed',
  'localIntegrityFailed',
  'localLaunchFailed',
  'invalidConfiguration',
  'invalidContext',
  'notConfigured',
  'configurationUnavailable',
  'busy',
  'cancelled',
  'unauthorized',
  'quotaExceeded',
  'modelUnavailable',
  'providerRejected',
  'connectionFailed',
  'timeout',
  'invalidStream',
  'incompleteStream',
  'outputLimit',
  'emptyResponse',
  'responseTooLarge',
] as const;
export type AiErrorCode = (typeof AI_ERROR_CODES)[number];

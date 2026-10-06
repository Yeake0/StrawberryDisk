export type PrivacyTimeRange = 'lastHour' | 'today' | 'lastSevenDays' | 'allTime';
export type PrivacyCategory = 'browserActivity' | 'browserAccountState' | 'applicationActivity' | 'systemActivity';
export type PrivacySensitivity = 'activity' | 'contentDerived' | 'accountState' | 'personalContent';
export type PrivacyImpact = 'low' | 'workflow' | 'signOut' | 'crossDevice' | 'dataLoss' | 'unknown';
export type PrivacyRecommendation = 'recommended' | 'manual' | 'reviewOnly' | 'unsupported';
export type PrivacyCapabilityState =
  | 'ready'
  | 'empty'
  | 'browserRunning'
  | 'applicationRunning'
  | 'permissionRequired'
  | 'unsupported'
  | 'schemaUnsupported'
  | 'unavailable';
export type PrivacyDataKind =
  | 'browsingHistory'
  | 'downloadHistory'
  | 'cookies'
  | 'siteStorage'
  | 'sitePermissions'
  | 'sessions'
  | 'browserCache'
  | 'searchHistory'
  | 'websiteIcons'
  | 'frequentlyVisitedSites'
  | 'addressBarShortcuts'
  | 'savedPasswords'
  | 'autofillData'
  | 'currentClipboard'
  | 'clipboardHistory'
  | 'recentItems'
  | 'recentApplications'
  | 'applicationUsageHistory'
  | 'networkConnectionHistory'
  | 'folderViewHistory'
  | 'printerHistory'
  | 'shellHistory'
  | 'jumpLists'
  | 'runDialogHistory'
  | 'fileDialogHistory'
  | 'systemSearchHistory'
  | 'explorerPathHistory'
  | 'applicationCache'
  | 'applicationLogs'
  | 'applicationSessions'
  | 'editorLocalHistory'
  | 'recentDocuments'
  | 'recentProjects'
  | 'recentConnections'
  | 'playbackHistory'
  | 'recentPaths'
  | 'recentSearches';

export interface PrivacyScanRequest {
  timeRange: PrivacyTimeRange;
}

export interface PrivacyScanProgress {
  stage: 'discovering' | 'browser' | 'application' | 'system' | 'finalizing';
  sourceName: string | null;
  completedSources: number;
  totalSources: number;
}

export interface PrivacyItem {
  token: string;
  sourceId: string;
  sourceName: string;
  profileId: string | null;
  profileName: string | null;
  category: PrivacyCategory;
  kind: PrivacyDataKind;
  sensitivity: PrivacySensitivity;
  impact: PrivacyImpact;
  recommendation: PrivacyRecommendation;
  capability: PrivacyCapabilityState;
  itemCount: number;
  estimatedBytes: number;
  selectedByDefault: boolean;
  requiresBrowserClose: boolean;
  synchronizationMayPropagate: boolean;
}

export interface PrivacySourceCoverage {
  sourceId: string;
  sourceName: string;
  iconPath: string | null;
  capability: PrivacyCapabilityState;
  itemCount: number;
}

export interface PrivacyScanResult {
  schemaVersion: number;
  scanId: string;
  revision: string;
  timeRange: PrivacyTimeRange;
  scannedAtMs: number;
  elapsedMs: number;
  items: PrivacyItem[];
  coverage: PrivacySourceCoverage[];
}

export interface PrivacyDetailsRequest {
  scanId: string;
  token: string;
  offset: number;
  limit: number;
}

export interface PrivacyDetailEntry {
  label: string;
  itemCount: number;
}

export interface PrivacyDetailsPage {
  schemaVersion: number;
  scanId: string;
  token: string;
  totalItemCount: number;
  presentation: 'list' | 'aggregateOnly';
  entries: PrivacyDetailEntry[];
  nextOffset: number | null;
}

export interface PrivacyExecutionRequest {
  scanId: string;
  tokens: string[];
}

export interface PrivacyExecutionPlanItem {
  token: string;
  sourceId: string;
  sourceName: string;
  profileName: string | null;
  kind: PrivacyDataKind;
  impact: PrivacyImpact;
  itemCount: number;
  estimatedBytes: number;
  requiresBrowserClose: boolean;
  synchronizationMayPropagate: boolean;
}

export interface PrivacyBrowserCloseRequirement {
  sourceId: string;
  sourceName: string;
  processes: string[];
}

export interface PrivacyExecutionPlan {
  schemaVersion: number;
  planId: string;
  scanId: string;
  createdAtMs: number;
  expiresAtMs: number;
  items: PrivacyExecutionPlanItem[];
  browserCloseRequirements: PrivacyBrowserCloseRequirement[];
  requiresConfirmation: boolean;
  requiresBrowserClose: boolean;
}

export interface PrivacyBrowserCloseRequest {
  planId: string;
  sourceIds: string[];
  mode: ApplicationCloseMode;
}

export interface PrivacyBrowserStatusRequest {
  planId: string;
  sourceIds: string[];
}

export interface PrivacyBrowserStatusTarget {
  sourceId: string;
  runningProcesses: string[];
}

export interface PrivacyBrowserStatusResult {
  runningProcessCount: number;
  targets: PrivacyBrowserStatusTarget[];
  elapsedMs: number;
}

export interface PrivacyExecutionRunRequest {
  planId: string;
  excludedSourceIds: string[];
}

export interface PrivacyExecutionProgress {
  stage: 'validating' | 'cleaning' | 'finalizing';
  currentToken: string | null;
  currentSourceName: string | null;
  currentKind: PrivacyDataKind | null;
  completedItemCount: number;
  totalItemCount: number;
  affectedItemCount: number;
  elapsedMs: number;
  completedItems: PrivacyExecutionProgressItem[];
}

export interface PrivacyExecutionProgressItem {
  token: string;
  status: PrivacyExecutionItemResult['status'];
}

export interface PrivacyExecutionItemResult {
  token: string;
  status: 'cleared' | 'unchanged' | 'failed' | 'cancelled';
  affectedItemCount: number;
  verified: boolean;
  failureReason: string | null;
}

export interface PrivacyExecutionResult {
  planId: string;
  affectedItemCount: number;
  failedItemCount: number;
  items: PrivacyExecutionItemResult[];
  scan: PrivacyScanResult | null;
}
import type { ApplicationCloseMode } from '@/lib/models/application-close';

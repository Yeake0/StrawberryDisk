import { defineStore } from 'pinia';

import type {
  PrivacyExecutionPlan,
  PrivacyExecutionPlanItem,
  PrivacyExecutionProgress,
  PrivacyExecutionResult,
  PrivacyBrowserStatusResult,
  PrivacyScanProgress,
  PrivacyScanResult,
  PrivacyTimeRange,
} from '@/lib/models/privacy';
import type { ApplicationCloseBatchResult, ApplicationCloseMode } from '@/lib/models/application-close';
import { LOG_DOMAINS, LOG_EVENTS } from '@/lib/models/telemetry';
import { LoggerService } from '@/lib/services/logger-service';
import { PrivacyService } from '@/lib/services/privacy-service';
import { parseCommandError } from '@/lib/utils/error';
import { applyPrivacyExecutionResult } from '@/lib/utils/privacy-execution-result';
import { initialPrivacySelection, isPrivacyItemActionable } from '@/lib/utils/privacy-selection';

import { useAppStore } from './app-store';

interface PrivacyState {
  scanResult: PrivacyScanResult | null;
  plan: PrivacyExecutionPlan | null;
  completedPlan: PrivacyExecutionPlan | null;
  result: PrivacyExecutionResult | null;
  closeResult: ApplicationCloseBatchResult | null;
  browserStatusResult: PrivacyBrowserStatusResult | null;
  selectedTokens: string[];
  timeRange: PrivacyTimeRange;
  scanning: boolean;
  cancellingScan: boolean;
  preparing: boolean;
  closingBrowsers: boolean;
  refreshingBrowserStatus: boolean;
  browserStatusRefreshFailed: boolean;
  executing: boolean;
  cancellingExecution: boolean;
  scanProgress: PrivacyScanProgress | null;
  executionProgress: PrivacyExecutionProgress | null;
  executionItems: PrivacyExecutionPlanItem[];
  executionStartedAtMs: number | null;
}

export const usePrivacyStore = defineStore('privacy', {
  state: (): PrivacyState => ({
    scanResult: null,
    plan: null,
    completedPlan: null,
    result: null,
    closeResult: null,
    browserStatusResult: null,
    selectedTokens: [],
    timeRange: 'allTime',
    scanning: false,
    cancellingScan: false,
    preparing: false,
    closingBrowsers: false,
    refreshingBrowserStatus: false,
    browserStatusRefreshFailed: false,
    executing: false,
    cancellingExecution: false,
    scanProgress: null,
    executionProgress: null,
    executionItems: [],
    executionStartedAtMs: null,
  }),
  actions: {
    async scan() {
      if (this.scanning || this.preparing || this.closingBrowsers || this.refreshingBrowserStatus || this.executing)
        return;
      this.scanning = true;
      this.cancellingScan = false;
      this.scanResult = null;
      this.selectedTokens = [];
      this.plan = null;
      this.completedPlan = null;
      this.result = null;
      this.closeResult = null;
      this.browserStatusResult = null;
      this.browserStatusRefreshFailed = false;
      this.scanProgress = null;
      this.executionProgress = null;
      this.executionItems = [];
      this.executionStartedAtMs = null;
      useAppStore().clearError();
      const startedAt = performance.now();
      LoggerService.info(LOG_DOMAINS.privacy, LOG_EVENTS.privacyScanRequested, { timeRange: this.timeRange });
      try {
        this.scanResult = await PrivacyService.scanWithProgress({ timeRange: this.timeRange }, progress => {
          this.scanProgress = progress;
        });
        this.selectedTokens = initialPrivacySelection(this.scanResult.items);
        LoggerService.info(LOG_DOMAINS.privacy, LOG_EVENTS.privacyScanCompleted, {
          sourceCount: this.scanResult.coverage.length,
          candidateCount: this.scanResult.items.length,
          elapsedMs: Math.round(performance.now() - startedAt),
        });
      } catch (error) {
        const code = parseCommandError(error)?.code;
        if (code !== 'operationCancelled') {
          LoggerService.warn(LOG_DOMAINS.privacy, LOG_EVENTS.privacyScanFailed, { code: code ?? 'unknown' });
          useAppStore().reportError(error);
        }
      } finally {
        this.scanning = false;
        this.cancellingScan = false;
        this.scanProgress = null;
      }
    },
    async cancelScan() {
      if (!this.scanning || this.cancellingScan) return;
      this.cancellingScan = true;
      try {
        await PrivacyService.cancelScan();
      } catch (error) {
        useAppStore().reportError(error);
        this.cancellingScan = false;
      }
    },
    setTimeRange(value: PrivacyTimeRange) {
      if (
        this.scanning ||
        this.preparing ||
        this.closingBrowsers ||
        this.refreshingBrowserStatus ||
        this.executing ||
        value === this.timeRange
      )
        return;
      this.timeRange = value;
      this.scanResult = null;
      this.selectedTokens = [];
      this.plan = null;
      this.completedPlan = null;
      this.result = null;
      this.closeResult = null;
      this.browserStatusResult = null;
      this.browserStatusRefreshFailed = false;
      this.scanProgress = null;
      this.executionProgress = null;
      this.executionItems = [];
      this.executionStartedAtMs = null;
    },
    toggle(token: string) {
      if (this.preparing || this.closingBrowsers || this.refreshingBrowserStatus || this.executing) return;
      const selected = new Set(this.selectedTokens);
      if (selected.has(token)) selected.delete(token);
      else selected.add(token);
      this.selectedTokens = [...selected];
      this.plan = null;
    },
    setSelection(tokens: string[]) {
      if (!this.scanResult || this.preparing || this.closingBrowsers || this.refreshingBrowserStatus || this.executing)
        return;
      const actionable = new Set(
        this.scanResult.items.filter(item => isPrivacyItemActionable(item)).map(item => item.token)
      );
      this.selectedTokens = [...new Set(tokens)].filter(token => actionable.has(token));
      this.plan = null;
    },
    async prepare() {
      if (
        !this.scanResult ||
        !this.selectedTokens.length ||
        this.preparing ||
        this.closingBrowsers ||
        this.refreshingBrowserStatus ||
        this.executing
      )
        return null;
      this.preparing = true;
      this.plan = null;
      this.closeResult = null;
      this.browserStatusResult = null;
      this.browserStatusRefreshFailed = false;
      useAppStore().clearError();
      try {
        this.plan = await PrivacyService.prepare({ scanId: this.scanResult.scanId, tokens: this.selectedTokens });
        return this.plan;
      } catch (error) {
        useAppStore().reportError(error);
        return null;
      } finally {
        this.preparing = false;
      }
    },
    clearPlan() {
      if (!this.executing) {
        this.plan = null;
        this.closeResult = null;
        this.browserStatusResult = null;
        this.browserStatusRefreshFailed = false;
      }
    },
    async closeBrowsers(sourceIds: string[], mode: ApplicationCloseMode) {
      if (
        !this.plan ||
        !sourceIds.length ||
        this.closingBrowsers ||
        this.refreshingBrowserStatus ||
        this.scanning ||
        this.preparing ||
        this.executing
      ) {
        return null;
      }
      this.closingBrowsers = true;
      this.closeResult = null;
      this.browserStatusResult = null;
      this.browserStatusRefreshFailed = false;
      useAppStore().clearError();
      try {
        this.closeResult = await PrivacyService.closeBrowsers({ planId: this.plan.planId, sourceIds, mode });
        return this.closeResult;
      } catch (error) {
        useAppStore().reportError(error);
        return null;
      } finally {
        this.closingBrowsers = false;
      }
    },
    async refreshBrowserStatus(sourceIds: string[]) {
      if (
        !this.plan ||
        !sourceIds.length ||
        this.refreshingBrowserStatus ||
        this.closingBrowsers ||
        this.scanning ||
        this.preparing ||
        this.executing
      ) {
        return null;
      }
      this.refreshingBrowserStatus = true;
      try {
        this.browserStatusResult = await PrivacyService.refreshBrowserStatus({
          planId: this.plan.planId,
          sourceIds,
        });
        if (this.browserStatusRefreshFailed) {
          LoggerService.info(LOG_DOMAINS.privacy, LOG_EVENTS.privacyBrowserStatusRefreshRecovered, {
            sourceCount: sourceIds.length,
            runningProcessCount: this.browserStatusResult.runningProcessCount,
          });
          this.browserStatusRefreshFailed = false;
          useAppStore().clearError();
        }
        return this.browserStatusResult;
      } catch (error) {
        // Process snapshots can fail transiently while applications are exiting. Keep retrying on
        // the existing one-second timer, but surface and log only the first failure in each outage.
        if (!this.browserStatusRefreshFailed) {
          const code = parseCommandError(error)?.code;
          this.browserStatusRefreshFailed = true;
          LoggerService.warn(LOG_DOMAINS.privacy, LOG_EVENTS.privacyBrowserStatusRefreshFailed, {
            code: code ?? 'unknown',
            sourceCount: sourceIds.length,
          });
          useAppStore().reportError(error);
        }
        return null;
      } finally {
        this.refreshingBrowserStatus = false;
      }
    },
    async execute(excludedSourceIds: string[] = []) {
      if (!this.plan || this.refreshingBrowserStatus || this.executing) return;
      const appStore = useAppStore();
      const completedPlan = this.plan;
      const planId = this.plan.planId;
      const previousScan = this.scanResult;
      this.executing = true;
      this.executionProgress = null;
      const excludedSources = new Set(excludedSourceIds);
      this.executionItems = completedPlan.items.filter(item => !excludedSources.has(item.sourceId));
      this.executionStartedAtMs = Date.now();
      this.result = null;
      appStore.clearError();
      const startedAt = performance.now();
      LoggerService.info(LOG_DOMAINS.privacy, LOG_EVENTS.privacyExecutionRequested, {
        selectedItemCount: completedPlan.items.length,
        excludedSourceCount: excludedSourceIds.length,
      });
      try {
        const result = await PrivacyService.executeWithProgress({ planId, excludedSourceIds }, progress => {
          this.executionProgress = progress;
        });
        this.result = result;
        this.completedPlan = completedPlan;
        this.scanResult = previousScan ? applyPrivacyExecutionResult(previousScan, result) : result.scan;
        const actionableTokens = new Set(
          this.scanResult?.items.filter(item => isPrivacyItemActionable(item)).map(item => item.token) ?? []
        );
        this.selectedTokens = this.selectedTokens.filter(token => actionableTokens.has(token));
        LoggerService.info(LOG_DOMAINS.privacy, LOG_EVENTS.privacyExecutionCompleted, {
          resultItemCount: result.items.length,
          affectedItemCount: result.affectedItemCount,
          failedItemCount: result.failedItemCount,
          elapsedMs: Math.round(performance.now() - startedAt),
        });
        await appStore.refreshSystemDisk();
      } catch (error) {
        const code = parseCommandError(error)?.code;
        if (code !== 'operationCancelled') {
          LoggerService.warn(LOG_DOMAINS.privacy, LOG_EVENTS.privacyExecutionFailed, { code: code ?? 'unknown' });
          appStore.reportError(error);
        }
      } finally {
        this.plan = null;
        this.executing = false;
        this.cancellingExecution = false;
        this.executionStartedAtMs = null;
      }
    },
    async cancelExecution() {
      if (!this.executing || this.cancellingExecution) return;
      this.cancellingExecution = true;
      try {
        await PrivacyService.cancelExecution();
      } catch (error) {
        useAppStore().reportError(error);
        this.cancellingExecution = false;
      }
    },
  },
});

import type { CleanupActionResult, PresentedCleanupActionResult } from '@/lib/models/cleanup-action';
import type {
  CleanupCategory,
  CleanupResult,
  PresentedCleanupResult,
  PresentedScanRuleResult,
  PresentedCleanupScanResult,
  ScanRuleResult,
  CleanupScanResult,
} from '@/lib/models/cleanup';
import type {
  DeepCleanupOperationRecord,
  OperationRecord,
  PresentedDeepCleanupOperationRecord,
  PresentedOperationRecord,
} from '@/lib/models/history';
type RuleGroup = 'ai' | 'app' | 'browser' | 'container' | 'custom' | 'dev' | 'system';
type MessageParameters = Readonly<Record<string, string>>;
type CustomRuleNames = Readonly<Record<string, string>>;
export type CleanupRuleMessageResolver = (key: string, parameters?: MessageParameters) => string | undefined;
/**
 * Builds the UI-owned presentation for backend rule facts. Locale resources
 * are optional: a new declarative rule remains readable before translators
 * add a curated entry.
 */
export function snapshot(
  snapshot: CleanupScanResult,
  resolveMessage: CleanupRuleMessageResolver,
  customRuleNames: CustomRuleNames = {}
): PresentedCleanupScanResult {
  return {
    ...snapshot,
    rules: snapshot.rules.map(rule => presentRule(rule, resolveMessage, customRuleNames)),
  };
}
export function cleanupResult(
  result: CleanupResult,
  resolveMessage: CleanupRuleMessageResolver,
  customRuleNames: CustomRuleNames = {}
): PresentedCleanupResult {
  return {
    ...result,
    actions: result.actions.map(action => presentAction(action, resolveMessage, customRuleNames)),
    record: presentRecord(result.record, resolveMessage),
  };
}
export function records(
  records: OperationRecord[],
  resolveMessage: CleanupRuleMessageResolver
): PresentedOperationRecord[] {
  return records.map<PresentedOperationRecord>(record =>
    isDeepCleanupRecord(record) ? presentRecord(record, resolveMessage) : record
  );
}

function isDeepCleanupRecord(record: OperationRecord): record is DeepCleanupOperationRecord {
  return record.details.type === 'deepCleanup';
}
function presentRule(
  rule: ScanRuleResult,
  resolveMessage: CleanupRuleMessageResolver,
  customRuleNames: CustomRuleNames
): PresentedScanRuleResult {
  const group = ruleGroup(rule.category);
  const keyPrefix = `cleanupRules.entries.${rule.ruleId}`;
  const resolvedFallbackName = fallbackName(rule.ruleId);
  const categoryLabel = resolveMessage(`cleanupRules.categories.${group}`) ?? rule.category;
  return {
    ...rule,
    name: customRuleNames[rule.ruleId] ?? resolveMessage(`${keyPrefix}.name`) ?? resolvedFallbackName,
    categoryLabel,
    description: resolveMessage(`${keyPrefix}.description`) ?? '',
    impact: resolveMessage(`${keyPrefix}.impact`) ?? '',
  };
}
function presentRecord(
  record: DeepCleanupOperationRecord,
  resolveMessage: CleanupRuleMessageResolver
): PresentedDeepCleanupOperationRecord {
  const cleanup = record.details.payload.cleanup;
  return {
    ...record,
    details: {
      ...record.details,
      payload: {
        ...record.details.payload,
        cleanup: cleanup
          ? {
              ...cleanup,
              actions: cleanup.actions.map(action => presentAction(action, resolveMessage)),
            }
          : null,
      },
    },
  };
}
function presentAction(
  action: CleanupActionResult,
  resolveMessage: CleanupRuleMessageResolver,
  customRuleNames: CustomRuleNames = {}
): PresentedCleanupActionResult {
  const reasonMessage = action.reasonCode
    ? resolveMessage(`cleanupRules.actionReasons.${action.reasonCode}`, {
        processes: action.runningProcesses.join(', '),
      })
    : undefined;
  const message = reasonMessage ?? resolveMessage(`cleanupRules.actionMessages.${action.status}`) ?? action.status;
  return {
    ...action,
    name:
      customRuleNames[action.ruleId] ??
      resolveMessage(`cleanupRules.entries.${action.ruleId}.name`) ??
      fallbackName(action.ruleId),
    message,
  };
}
function ruleGroup(category: CleanupCategory): RuleGroup {
  if (category === 'custom') return 'custom';
  if (category === 'application') return 'app';
  if (category === 'development') return 'dev';
  if (category === 'ai' || category === 'browser' || category === 'container' || category === 'system') {
    return category;
  }
  return 'system';
}
export function fallbackName(ruleId: string): string {
  const segment = ruleId.split('.').at(-1) ?? ruleId;
  return segment
    .split(/[-_]+/u)
    .filter(Boolean)
    .map(word => word.charAt(0).toLocaleUpperCase() + word.slice(1))
    .join(' ');
}

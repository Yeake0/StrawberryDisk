import type { TraversalProgress } from '@/lib/models/progress';

export type DuplicateProgressBytesLabelKey =
  'duplicateFiles.verifiedContentData' | 'duplicateFiles.discoveredLogicalData';

/**
 * Maps backend progress semantics to the byte counter shown by the duplicate page.
 * Discovery reports logical file sizes, while hashing reports bytes actually read for
 * verification. Keeping this distinction explicit prevents sparse files from making the UI
 * imply that the displayed number is physical disk usage.
 */
export function duplicateProgressBytesLabelKey(
  stage: TraversalProgress['currentStage'] | null | undefined
): DuplicateProgressBytesLabelKey {
  return stage === 'hashingFiles' ? 'duplicateFiles.verifiedContentData' : 'duplicateFiles.discoveredLogicalData';
}

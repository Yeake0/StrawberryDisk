import { describe, expect, it } from 'vitest';

import { TRAVERSAL_STAGE_IDS } from '@/lib/models/progress';

import { duplicateProgressBytesLabelKey } from './duplicate-file-progress-presentation';

describe('duplicateProgressBytesLabelKey', () => {
  it('labels content verification bytes only during hashing', () => {
    expect(duplicateProgressBytesLabelKey('hashingFiles')).toBe('duplicateFiles.verifiedContentData');
  });

  it('labels every non-hashing stage as discovered logical data', () => {
    for (const stage of Object.values(TRAVERSAL_STAGE_IDS).filter(stage => stage !== 'hashingFiles')) {
      expect(duplicateProgressBytesLabelKey(stage)).toBe('duplicateFiles.discoveredLogicalData');
    }
    expect(duplicateProgressBytesLabelKey(null)).toBe('duplicateFiles.discoveredLogicalData');
    expect(duplicateProgressBytesLabelKey(undefined)).toBe('duplicateFiles.discoveredLogicalData');
  });
});

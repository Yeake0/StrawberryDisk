import { describe, expect, it } from 'vitest';

import type { PrivacyItem } from '@/lib/models/privacy';

import {
  actionablePrivacyTokens,
  capturePrivacySelection,
  initialPrivacySelection,
  isPrivacyItemActionable,
  privacySelectionMode,
  recommendedPrivacyTokens,
  restorePrivacySelection,
  summarizePrivacySelection,
} from './privacy-selection';

function item(overrides: Partial<PrivacyItem> = {}): PrivacyItem {
  return {
    token: 'token',
    sourceId: 'browser',
    sourceName: 'Browser',
    profileId: 'browser:Default',
    profileName: 'Default',
    category: 'browserActivity',
    kind: 'downloadHistory',
    sensitivity: 'activity',
    impact: 'low',
    recommendation: 'recommended',
    capability: 'ready',
    itemCount: 3,
    estimatedBytes: 24,
    selectedByDefault: true,
    requiresBrowserClose: true,
    synchronizationMayPropagate: false,
    ...overrides,
  };
}

describe('privacy selection', () => {
  it('never selects unavailable or empty items by default', () => {
    const items = [
      item({ token: 'ready' }),
      item({ token: 'empty', capability: 'empty', itemCount: 0 }),
      item({ token: 'unknown', capability: 'schemaUnsupported' }),
    ];
    expect(initialPrivacySelection(items)).toEqual(['ready']);
  });

  it('supports smart, all, none, and manual quick-selection modes', () => {
    const items = [
      item({ token: 'recommended' }),
      item({ token: 'manual', selectedByDefault: false, recommendation: 'manual' }),
      item({ token: 'unavailable', selectedByDefault: false, capability: 'unavailable' }),
    ];

    expect(recommendedPrivacyTokens(items)).toEqual(['recommended']);
    expect(actionablePrivacyTokens(items)).toEqual(['recommended', 'manual']);
    expect(privacySelectionMode(items, ['recommended'])).toBe('smart');
    expect(privacySelectionMode(items, ['recommended', 'manual'])).toBe('all');
    expect(privacySelectionMode(items, [])).toBe('none');
    expect(privacySelectionMode(items, ['manual'])).toBe('manual');
  });

  it('allows a running-browser item to be reviewed but rejects unsupported items', () => {
    expect(isPrivacyItemActionable(item({ capability: 'browserRunning' }))).toBe(true);
    expect(isPrivacyItemActionable(item({ capability: 'browserRunning', itemCount: 0 }))).toBe(true);
    expect(isPrivacyItemActionable(item({ recommendation: 'reviewOnly' }))).toBe(false);
    expect(isPrivacyItemActionable(item({ capability: 'unsupported' }))).toBe(false);
  });

  it('allows manual personal-data cleanup without selecting it by default', () => {
    const savedPasswords = item({
      kind: 'savedPasswords',
      sensitivity: 'personalContent',
      impact: 'dataLoss',
      recommendation: 'manual',
      selectedByDefault: false,
      synchronizationMayPropagate: true,
    });

    expect(isPrivacyItemActionable(savedPasswords)).toBe(true);
    expect(initialPrivacySelection([savedPasswords])).toEqual([]);
  });

  it('summarizes only opaque selected tokens', () => {
    const summary = summarizePrivacySelection(
      [
        item({ token: 'one', itemCount: 2, estimatedBytes: 10 }),
        item({ token: 'two', itemCount: 4, estimatedBytes: 20 }),
      ],
      ['two']
    );
    expect(summary).toEqual({ itemCount: 4, estimatedBytes: 20, pendingScanCount: 0 });
  });

  it('keeps deferred running-browser scans visible in the selection summary', () => {
    const summary = summarizePrivacySelection(
      [item({ token: 'locked-cookie', kind: 'cookies', capability: 'browserRunning', itemCount: 0 })],
      ['locked-cookie']
    );

    expect(summary).toEqual({ itemCount: 0, estimatedBytes: 24, pendingScanCount: 1 });
  });

  it('restores intent after rescan while excluding browsers the user left running', () => {
    const before = [
      item({ token: 'old-edge', sourceId: 'edge' }),
      item({ token: 'old-system', sourceId: 'recent-items', profileId: null, profileName: null, kind: 'recentItems' }),
    ];
    const identities = capturePrivacySelection(
      before,
      before.map(value => value.token)
    );
    const after = [
      item({ token: 'new-edge', sourceId: 'edge' }),
      item({ token: 'new-system', sourceId: 'recent-items', profileId: null, profileName: null, kind: 'recentItems' }),
      item({ token: 'new-unrelated', sourceId: 'chrome' }),
    ];

    expect(restorePrivacySelection(after, identities, ['edge'])).toEqual(['new-system']);
  });

  it('restores a profile by stable identity when its display name changes', () => {
    const before = item({ token: 'old', profileId: 'chrome:Profile 1', profileName: 'Work' });
    const after = [
      item({ token: 'renamed', profileId: 'chrome:Profile 1', profileName: 'Team' }),
      item({ token: 'same-label', profileId: 'chrome:Profile 2', profileName: 'Work' }),
    ];

    expect(restorePrivacySelection(after, capturePrivacySelection([before], ['old']))).toEqual(['renamed']);
  });
});

import { describe, expect, it } from 'vitest';

import type { ApplicationCloseBatchResult } from '@/lib/models/application-close';
import type { PrivacyBrowserCloseRequirement } from '@/lib/models/privacy';

import { privacyBrowserCloseItems, privacyBrowserCloseRetry, privacyBrowserStatusRetry } from './privacy-browser-close';
import privacyPageSource from './index.vue?raw';

const requirements: PrivacyBrowserCloseRequirement[] = [
  { sourceId: 'chrome', sourceName: 'Google Chrome', processes: ['Google Chrome', 'chrome.exe'] },
  { sourceId: 'edge', sourceName: 'Microsoft Edge', processes: ['msedge.exe'] },
];

describe('privacy browser close', () => {
  it('keeps cleanup execution in the result workspace without flashing a preparation spinner', () => {
    expect(privacyPageSource).toContain('<MdOperationWorkspace v-if="store.scanning">');
    expect(privacyPageSource).not.toContain('store.scanning || store.executing');
    expect(privacyPageSource).not.toContain('<MdSpinner');
    expect(privacyPageSource).not.toContain('<MdSpinner v-if="store.preparing || store.executing"');
  });

  it('maps trusted plan requirements to selectable browser rows', () => {
    expect(privacyBrowserCloseItems(requirements, { chrome: '/Applications/Google Chrome.app' })).toEqual([
      {
        id: 'chrome',
        name: 'Google Chrome',
        processes: ['Google Chrome', 'chrome.exe'],
        iconPath: '/Applications/Google Chrome.app',
      },
      { id: 'edge', name: 'Microsoft Edge', processes: ['msedge.exe'], iconPath: undefined },
    ]);
  });

  it('narrows force retry to browser sources that remain active', () => {
    const result: ApplicationCloseBatchResult = {
      mode: 'graceful',
      matchedProcessCount: 2,
      requestedProcessCount: 3,
      remainingProcessCount: 1,
      failedTargetCount: 0,
      elapsedMs: 20,
      targets: [
        {
          targetId: 'chrome',
          status: 'completed',
          matchedProcessCount: 1,
          requestedProcessCount: 2,
          remainingProcesses: ['chrome.exe'],
        },
        {
          targetId: 'edge',
          status: 'completed',
          matchedProcessCount: 1,
          requestedProcessCount: 1,
          remainingProcesses: [],
        },
      ],
    };

    expect(privacyBrowserCloseRetry(requirements, result, { chrome: 'C:\\Program Files\\Google\\Chrome.exe' })).toEqual(
      {
        sourceIds: ['chrome'],
        items: [
          {
            id: 'chrome',
            name: 'Google Chrome',
            processes: ['chrome.exe'],
            iconPath: 'C:\\Program Files\\Google\\Chrome.exe',
          },
        ],
      }
    );
  });

  it('removes stopped sources after a read-only refresh and keeps missing results visible', () => {
    expect(
      privacyBrowserStatusRetry(requirements, {
        runningProcessCount: 1,
        elapsedMs: 2,
        targets: [{ sourceId: 'chrome', runningProcesses: [] }],
      })
    ).toEqual({
      sourceIds: ['edge'],
      items: [{ id: 'edge', name: 'Microsoft Edge', processes: ['msedge.exe'], iconPath: undefined }],
    });
  });
});

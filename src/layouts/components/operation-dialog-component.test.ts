// @vitest-environment happy-dom

import { flushPromises, mount } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { TooltipProvider } from 'reka-ui';
import { defineComponent } from 'vue';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n } from '@/i18n';
import { CLEANUP_OPERATION_IDS } from '@/lib/models/cleanup';
import { LANGUAGE_IDS } from '@/lib/models/settings';
import { OperatingSystemService } from '@/lib/services/operating-system-service';
import { useApplicationStore } from '@/stores/application-store';
import { useCleanupStore } from '@/stores/cleanup-store';
import { usePrivacyStore } from '@/stores/privacy-store';

import MdCleanupOperationOverlay from './md-cleanup-operation-overlay.vue';
import MdPrivacyOperationOverlay from './md-privacy-operation-overlay.vue';

const originalLocale = i18n.global.locale.value;
const wrappers: ReturnType<typeof mount>[] = [];
beforeEach(() => {
  setActivePinia(createPinia());
  vi.spyOn(OperatingSystemService, 'currentPlatform').mockReturnValue('macos');
});
afterEach(() => {
  for (const wrapper of wrappers.splice(0)) wrapper.unmount();
  i18n.global.locale.value = originalLocale;
  vi.restoreAllMocks();
});

async function mountWorkflow(kind: 'cleanup' | 'privacy') {
  const cleanup = useCleanupStore();
  cleanup.loading = kind === 'cleanup';
  cleanup.operation = CLEANUP_OPERATION_IDS.cleaning;
  cleanup.selectedRuleIds = ['fixture'];
  cleanup.executionRuleIds = ['fixture'];
  const privacy = usePrivacyStore();
  privacy.executing = kind === 'privacy';
  privacy.executionItems = [
    {
      token: 'fixture',
      sourceId: 'firefox',
      sourceName: 'Firefox',
      profileName: 'Fixture profile',
      kind: 'browsingHistory',
      impact: 'low',
      itemCount: 3,
      estimatedBytes: 0,
      requiresBrowserClose: false,
      synchronizationMayPropagate: false,
    },
  ];
  const wrapper = mount(
    defineComponent({
      components: { MdCleanupOperationOverlay, MdPrivacyOperationOverlay, TooltipProvider },
      setup: () => ({
        cleanup: kind === 'cleanup',
        rules: [{ ruleId: 'fixture', name: 'Fixture cache', fileCount: 3 }],
      }),
      template: `<TooltipProvider>
        <MdCleanupOperationOverlay v-if="cleanup" :rules="rules" :cancelling="false" />
        <MdPrivacyOperationOverlay v-else />
      </TooltipProvider>`,
    }),
    { attachTo: document.body, global: { plugins: [i18n] } }
  );
  wrappers.push(wrapper);
  await flushPromises();
  return { wrapper, cleanup, privacy };
}

function buttonWithText(dialog: Element, label: string) {
  const button = [...dialog.querySelectorAll('button')].find(element => element.textContent?.trim() === label);
  expect(button).toBeDefined();
  return button!;
}

describe.each(['cleanup', 'privacy'] as const)('%s operation modal', kind => {
  it.each(Object.values(LANGUAGE_IDS))('keeps cancellation explicit in locale %s', async locale => {
    i18n.global.locale.value = locale;
    const { wrapper } = await mountWorkflow(kind);
    const operation = document.querySelector<HTMLElement>('[role="dialog"]')!;
    expect(operation).not.toBeNull();
    expect(operation.getAttribute('aria-labelledby')).toBe(operation.querySelector('[data-slot="dialog-title"]')?.id);
    expect(operation.querySelector('[data-slot="dialog-title"]')?.textContent?.trim()).not.toBe('');
    const cancel = buttonWithText(operation, i18n.global.t('loading.cancelCleanupAction'));
    expect(document.activeElement).toBe(cancel);
    cancel.click();
    await flushPromises();

    let dialogs = document.querySelectorAll<HTMLElement>('[role="dialog"]');
    expect(dialogs).toHaveLength(2);
    const confirmation = dialogs[1]!;
    expect(confirmation.textContent).toContain(i18n.global.t('loading.cancelCleanupConfirmTitle'));
    confirmation.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    await flushPromises();
    expect(document.querySelectorAll('[role="dialog"]')).toHaveLength(1);
    expect(document.activeElement).toBe(cancel);
    const component = wrapper.findComponent(kind === 'cleanup' ? MdCleanupOperationOverlay : MdPrivacyOperationOverlay);
    expect(component.emitted('cancel')).toBeUndefined();

    cancel.click();
    await flushPromises();
    dialogs = document.querySelectorAll<HTMLElement>('[role="dialog"]');
    buttonWithText(dialogs[1]!, i18n.global.t('loading.stopCleanupAction')).click();
    await flushPromises();
    expect(component.emitted('cancel')).toHaveLength(1);
    expect(document.querySelectorAll('[role="dialog"]')).toHaveLength(1);
  });

  it('releases both modal layers when work finishes during a cancellation prompt', async () => {
    const { cleanup, privacy } = await mountWorkflow(kind);
    buttonWithText(document.querySelector('[role="dialog"]')!, i18n.global.t('loading.cancelCleanupAction')).click();
    await flushPromises();
    expect(document.querySelectorAll('[role="dialog"]')).toHaveLength(2);
    cleanup.loading = false;
    privacy.executing = false;
    await flushPromises();
    expect(document.querySelector('[role="dialog"]')).toBeNull();
    expect(document.body.style.pointerEvents).not.toBe('none');
  });
});

it('uses the same locked dialog for cleanup previews and application leftovers', async () => {
  const { cleanup } = await mountWorkflow('cleanup');
  cleanup.operation = CLEANUP_OPERATION_IDS.previewing;
  await flushPromises();
  expect(document.querySelector('[role="dialog"] button')).toBeNull();
  cleanup.loading = false;
  useApplicationStore().deletingLeftovers = true;
  await flushPromises();
  const dialog = document.querySelector('[role="dialog"]')!;
  expect(dialog.textContent).toContain(i18n.global.t('loading.cleaningApplicationLeftovers'));
  expect(dialog.querySelector('[role="progressbar"]')?.hasAttribute('aria-valuenow')).toBe(false);
  expect(dialog.querySelector('.operation-dialog-stats')).toBeNull();
  expect(dialog.querySelector('button')).not.toBeNull();
});

// @vitest-environment happy-dom

import { mount } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { beforeEach, describe, expect, it } from 'vitest';

import { i18n } from '@/i18n';
import type { PrivacyExecutionPlan } from '@/lib/models/privacy';
import { usePrivacyStore } from '@/stores/privacy-store';

import MdPrivacyOperationOverlay from './md-privacy-operation-overlay.vue';

const plan: PrivacyExecutionPlan = {
  schemaVersion: 2,
  planId: 'privacy-plan-1234567890',
  scanId: 'scan-1',
  createdAtMs: 1,
  expiresAtMs: 2,
  requiresConfirmation: true,
  requiresBrowserClose: false,
  browserCloseRequirements: [],
  items: [
    {
      token: 'history-token',
      sourceId: 'firefox',
      sourceName: 'Firefox',
      profileName: 'default',
      kind: 'browsingHistory',
      impact: 'low',
      itemCount: 3,
      estimatedBytes: 0,
      requiresBrowserClose: false,
      synchronizationMayPropagate: false,
    },
  ],
};

describe('privacy operation overlay', () => {
  beforeEach(() => setActivePinia(createPinia()));

  it('shows real execution progress and confirms cancellation inside the modal', async () => {
    const store = usePrivacyStore();
    store.plan = structuredClone(plan);
    store.executionItems = structuredClone(plan.items);
    store.executing = true;
    store.executionStartedAtMs = Date.now();
    store.executionProgress = {
      stage: 'cleaning',
      currentToken: 'history-token',
      currentSourceName: 'Firefox',
      currentKind: 'browsingHistory',
      completedItemCount: 0,
      totalItemCount: 1,
      affectedItemCount: 2,
      elapsedMs: 10,
      completedItems: [],
    };
    const wrapper = mount(MdPrivacyOperationOverlay, {
      global: {
        plugins: [i18n],
        stubs: {
          Button: {
            template: '<button class="cancel-cleanup" type="button" @click="$emit(\'click\')"><slot /></button>',
          },
          MdConfirmDialog: {
            props: ['open'],
            emits: ['confirm', 'update:open'],
            template: '<button v-if="open" class="confirm-cancel" type="button" @click="$emit(\'confirm\')" />',
          },
          MdIcon: true,
          MdOperationDialog: {
            props: ['open', 'title', 'description', 'progress', 'cancelDisabled', 'cancelLabel'],
            emits: ['cancel'],
            template: `<section v-if="open"><h2>{{ title }}</h2><p>{{ description }}</p><slot />
              <div role="progressbar" :aria-valuenow="Math.round(progress)" />
              <button class="cancel-cleanup" :disabled="cancelDisabled" @click="$emit('cancel')">{{ cancelLabel }}</button>
            </section>`,
          },
        },
      },
    });

    expect(wrapper.text()).toContain('Firefox');
    expect(wrapper.text()).toContain('Browsing history');
    expect(wrapper.get('[role="progressbar"]').attributes('aria-valuenow')).toBe('10');

    await wrapper.get('.cancel-cleanup').trigger('click');
    await wrapper.get('.confirm-cancel').trigger('click');

    expect(wrapper.emitted('cancel')).toHaveLength(1);
    wrapper.unmount();
  });

  it('keeps failed and cancelled items distinct from successful cleanup', () => {
    const store = usePrivacyStore();
    store.executionItems = [
      { ...structuredClone(plan.items[0]!), token: 'failed-token' },
      { ...structuredClone(plan.items[0]!), token: 'cancelled-token' },
    ];
    store.executing = true;
    store.executionStartedAtMs = Date.now();
    store.executionProgress = {
      stage: 'finalizing',
      currentToken: null,
      currentSourceName: null,
      currentKind: null,
      completedItemCount: 2,
      totalItemCount: 2,
      affectedItemCount: 0,
      elapsedMs: 20,
      completedItems: [
        { token: 'failed-token', status: 'failed' },
        { token: 'cancelled-token', status: 'cancelled' },
      ],
    };
    const wrapper = mount(MdPrivacyOperationOverlay, {
      global: {
        plugins: [i18n],
        stubs: {
          MdOperationDialog: { props: ['open'], template: '<section v-if="open"><slot /></section>' },
          MdConfirmDialog: true,
          MdIcon: true,
        },
      },
    });

    const items = wrapper.findAll('.privacy-operation-item');
    expect(items[0]?.classes()).toContain('is-failed');
    expect(items[0]?.text()).toContain('Could not complete');
    expect(items[1]?.classes()).toContain('is-cancelled');
    expect(items[1]?.text()).toContain('Cancelled');
    expect(wrapper.findAll('.is-cleared')).toHaveLength(0);
    wrapper.unmount();
  });
});

// @vitest-environment happy-dom

import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n } from '@/i18n';
import type { PrivacyItem } from '@/lib/models/privacy';

import MdPrivacyDetailDialog from './md-privacy-detail-dialog.vue';

const { detailsMock } = vi.hoisted(() => ({ detailsMock: vi.fn() }));
vi.mock('@/lib/services/privacy-service', () => ({ PrivacyService: { details: detailsMock } }));

const passthroughStub = { template: '<div><slot /></div>' };
const buttonStub = {
  props: ['disabled'],
  emits: ['click'],
  template: '<button :disabled="disabled" @click="$emit(\'click\')"><slot /></button>',
};

const item: PrivacyItem = {
  token: 'opaque-token',
  sourceId: 'chrome',
  sourceName: 'Google Chrome',
  profileId: 'chrome:Default',
  profileName: 'Default',
  category: 'browserAccountState',
  kind: 'cookies',
  sensitivity: 'accountState',
  impact: 'signOut',
  recommendation: 'manual',
  capability: 'ready',
  itemCount: 3,
  estimatedBytes: 32,
  selectedByDefault: false,
  requiresBrowserClose: true,
  synchronizationMayPropagate: true,
};

function mountDialog() {
  return mount(MdPrivacyDetailDialog, {
    props: { modelValue: true, scanId: 'scan-1', item },
    global: {
      plugins: [i18n],
      stubs: {
        Button: buttonStub,
        Dialog: passthroughStub,
        DialogDescription: passthroughStub,
        DialogTitle: passthroughStub,
        MdDialogContent: passthroughStub,
        MdDialogFooter: passthroughStub,
        MdDialogHeader: passthroughStub,
        MdSpinner: true,
      },
    },
  });
}

describe('privacy detail dialog component', () => {
  beforeEach(() => detailsMock.mockReset());

  it('shows a read-only page and loads the next bounded page', async () => {
    detailsMock
      .mockResolvedValueOnce({
        schemaVersion: 1,
        scanId: 'scan-1',
        token: 'opaque-token',
        totalItemCount: 3,
        presentation: 'list',
        entries: [{ label: 'example.com', itemCount: 2 }],
        nextOffset: 1,
      })
      .mockResolvedValueOnce({
        schemaVersion: 1,
        scanId: 'scan-1',
        token: 'opaque-token',
        totalItemCount: 3,
        presentation: 'list',
        entries: [{ label: 'second.example', itemCount: 1 }],
        nextOffset: null,
      });
    const wrapper = mountDialog();
    await flushPromises();

    expect(wrapper.text()).toContain('example.com');
    expect(wrapper.text()).not.toContain('Details confirm the scan result');
    expect(detailsMock).toHaveBeenCalledWith({ scanId: 'scan-1', token: 'opaque-token', offset: 0, limit: 100 });

    await wrapper.get('.detail-load-more').trigger('click');
    await flushPromises();

    expect(wrapper.text()).toContain('second.example');
    expect(detailsMock).toHaveBeenLastCalledWith({
      scanId: 'scan-1',
      token: 'opaque-token',
      offset: 1,
      limit: 100,
    });
    expect(wrapper.findAll('.detail-entry-list > div')).toHaveLength(2);
    wrapper.unmount();
  });

  it('keeps the complete path in the detail label tooltip', async () => {
    const fullPath = 'C:\\Users\\harry\\Documents\\reports\\quarterly-review.docx';
    detailsMock.mockResolvedValue({
      schemaVersion: 1,
      scanId: 'scan-1',
      token: 'opaque-token',
      totalItemCount: 1,
      presentation: 'list',
      entries: [{ label: fullPath, itemCount: 1 }],
      nextOffset: null,
    });
    const wrapper = mountDialog();
    await flushPromises();

    expect(wrapper.getComponent({ name: 'MdTooltip' }).props('text')).toBe(fullPath);
    // Tooltip's fragment must not strip the dialog's scoped path typography.
    const label = wrapper.get('.detail-entry-label');
    const dialogScopes = label.element.parentElement!.getAttributeNames().filter(name => name.startsWith('data-v-'));
    expect(dialogScopes.length).toBeGreaterThan(0);
    for (const scope of dialogScopes) expect(label.attributes(scope)).toBeDefined();
    wrapper.unmount();
  });

  it('explains sources that expose only an aggregate count', async () => {
    detailsMock.mockResolvedValue({
      schemaVersion: 1,
      scanId: 'scan-1',
      token: 'opaque-token',
      totalItemCount: 3,
      presentation: 'aggregateOnly',
      entries: [],
      nextOffset: null,
    });
    const wrapper = mountDialog();
    await flushPromises();

    expect(wrapper.text()).toContain('Only a total count is available');
    wrapper.unmount();
  });

  it('discards an in-flight private page when the dialog closes', async () => {
    let resolveDetails!: (value: unknown) => void;
    detailsMock.mockReturnValue(
      new Promise(resolve => {
        resolveDetails = resolve;
      })
    );
    const wrapper = mountDialog();
    await wrapper.setProps({ modelValue: false });

    resolveDetails({
      schemaVersion: 1,
      scanId: 'scan-1',
      token: 'opaque-token',
      totalItemCount: 3,
      presentation: 'list',
      entries: [{ label: 'private.example', itemCount: 3 }],
      nextOffset: null,
    });
    await flushPromises();

    expect(wrapper.text()).not.toContain('private.example');
    wrapper.unmount();
  });

  it('loads a newly selected item without waiting for the previous detail request', async () => {
    let resolveFirst!: (value: unknown) => void;
    detailsMock
      .mockReturnValueOnce(
        new Promise(resolve => {
          resolveFirst = resolve;
        })
      )
      .mockResolvedValueOnce({
        schemaVersion: 1,
        scanId: 'scan-1',
        token: 'second-token',
        totalItemCount: 1,
        presentation: 'list',
        entries: [{ label: 'current-detail.example', itemCount: 1 }],
        nextOffset: null,
      });
    const wrapper = mountDialog();

    await wrapper.setProps({ item: { ...item, token: 'second-token', kind: 'siteStorage' } });
    await flushPromises();

    expect(detailsMock).toHaveBeenCalledTimes(2);
    expect(wrapper.text()).toContain('current-detail.example');

    resolveFirst({
      schemaVersion: 1,
      scanId: 'scan-1',
      token: 'opaque-token',
      totalItemCount: 1,
      presentation: 'list',
      entries: [{ label: 'stale-detail.example', itemCount: 1 }],
      nextOffset: null,
    });
    await flushPromises();

    expect(wrapper.text()).not.toContain('stale-detail.example');
    wrapper.unmount();
  });
});

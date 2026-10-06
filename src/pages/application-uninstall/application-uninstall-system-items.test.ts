// @vitest-environment happy-dom

import { flushPromises, shallowMount } from '@vue/test-utils';
import { createPinia } from 'pinia';
import { createI18n } from 'vue-i18n';
import { afterEach, describe, expect, it, vi } from 'vitest';
import MdCheckbox from '@/components/custom/md-checkbox.vue';
import MdResultSummary from '@/components/custom/md-result-summary.vue';
import MdResultCheckbox from '@/components/custom/md-result-checkbox.vue';
import MdSelectionActionBar from '@/components/custom/md-selection-action-bar.vue';
import MdDestructiveActionDialog from '@/components/custom/md-destructive-action-dialog.vue';
import MdResultSearch from '@/components/custom/md-result-search.vue';
import MdCategoryFilter from '@/components/custom/md-category-filter.vue';
import { AiService } from '@/lib/services/ai-service';
import { ApplicationService } from '@/lib/services/application-service';
import { LoggerService } from '@/lib/services/logger-service';
import { useAiStore } from '@/stores/ai-store';
import type { ApplicationUninstallCandidate } from '@/lib/models/application';
import en from '@/locales/en-US.json';
import Page from './index.vue';
import Row from './components/md-application-uninstall-row.vue';
import { displayedApplications, applicationCanStartUninstall } from './application-uninstall-catalog';

vi.mock('@tauri-apps/plugin-os', () => ({ platform: () => 'windows' }));
vi.mock('vue-sonner', () => ({ toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() } }));
afterEach(() => vi.restoreAllMocks());

function candidate(id: string, systemKind: ApplicationUninstallCandidate['systemKind']): ApplicationUninstallCandidate {
  return {
    applicationId: id,
    primaryIdentifier: `private-${id}`,
    sourceIdentities: [],
    systemKind,
    name: id,
    version: null,
    publisher: 'Private publisher',
    estimatedBytes: 100,
    lastUsedAtMs: null,
    installedAtMs: null,
    platform: 'windowsRegistry',
    installerKind: 'windowsAppx',
    executionMode: 'silent',
    capability: 'ready',
    recordState: 'installed',
    uninstallDiagnostic: null,
    applicationPath: null,
    possibleRelatedPaths: [],
    iconPath: null,
    runningProcesses: [],
    totalBytes: 100,
    defaultSelectedBytes: 100,
    associatedDataComplete: false,
    components: [
      {
        componentId: `${id}-installer`,
        kind: 'nativeInstaller',
        risk: 'required',
        path: null,
        bytes: 100,
        fileCount: 1,
        defaultSelected: true,
      },
    ],
  };
}
const regular = candidate('regular', 'unclassified');
const system = candidate('system', 'windowsBuiltinApp');
const orphan = {
  ...candidate('orphan', 'sharedRuntime'),
  capability: 'viewOnly' as const,
  recordState: 'orphanedRegistration' as const,
  components: [],
};

function render(pinia = createPinia()) {
  vi.spyOn(LoggerService, 'info').mockImplementation(() => {});
  vi.spyOn(LoggerService, 'warn').mockImplementation(() => {});
  vi.spyOn(ApplicationService, 'describeIdentity').mockResolvedValue({ schemaVersion: 1, metadata: null });
  return shallowMount(Page, {
    props: {
      catalog: {
        schemaVersion: 11,
        scannedAtMs: 123,
        supported: true,
        executionSupported: true,
        catalogActionable: true,
        inventoryComplete: true,
        catalogRevision: 'private-revision',
        candidates: [regular, system, orphan],
        readyCount: 2,
        blockedCount: 1,
        hiddenCount: 0,
        relatedDirectoryCount: 0,
        relatedPathScanElapsedMs: 0,
        elapsedMs: 0,
      },
      scanning: false,
      cancelling: false,
      progress: null,
      executionProgress: null,
      plan: null,
      preview: null,
      lastResult: null,
      preparing: false,
      executing: false,
      cancellingExecution: false,
      cancellationRevision: 0,
      closingApplications: false,
      closeResult: null,
    },
    global: {
      plugins: [pinia, createI18n({ legacy: false, locale: 'en-US', messages: { 'en-US': en } })],
      renderStubDefaultSlot: true,
      stubs: {
        MdPageShell: { template: '<div><slot/><slot name="footer"/></div>' },
        MdResultWorkspace: { template: '<div><slot name="summary"/><slot name="header"/><slot/></div>' },
        MdResultSummary: false,
        MdResultTable: { methods: { scrollTo() {} }, template: '<div><slot name="header"/><slot/></div>' },
        MdResultFilterToolbar: { template: '<div><slot/><slot name="aside"/></div>' },
      },
    },
  });
}

async function showSystem(wrapper: ReturnType<typeof render>, show: boolean) {
  wrapper.getComponent(MdCheckbox).vm.$emit('update:modelValue', show);
  await flushPromises();
}

describe('application AI explanation', () => {
  it('uses native identity but ignores an identity response after the selection changes', async () => {
    const pinia = createPinia();
    const ai = useAiStore(pinia);
    const show = vi.spyOn(ai, 'show').mockResolvedValue(undefined);
    const wrapper = render(pinia);
    const metadata = {
      platform: 'windows' as const,
      productName: 'Example Product',
      fileDescription: 'Editor',
      companyName: 'Example Company',
      packageIdentity: null,
    };
    vi.mocked(ApplicationService.describeIdentity).mockResolvedValueOnce({ schemaVersion: 1, metadata });
    const row = wrapper.getComponent(Row);
    row.vm.$emit('explain');
    await flushPromises();
    expect(ApplicationService.describeIdentity).toHaveBeenCalledWith(regular.applicationId, 'private-revision');
    expect(show).toHaveBeenLastCalledWith(
      expect.objectContaining({ subject: expect.objectContaining({ identity: metadata }) }),
      'en-US'
    );

    let resolve!: (value: { schemaVersion: number; metadata: typeof metadata }) => void;
    vi.mocked(ApplicationService.describeIdentity).mockReturnValueOnce(
      new Promise(done => {
        resolve = done;
      })
    );
    show.mockClear();
    row.vm.$emit('explain');
    row.vm.$emit('toggleSelection');
    await flushPromises();
    resolve({ schemaVersion: 1, metadata });
    await flushPromises();
    expect(show).not.toHaveBeenCalled();

    vi.mocked(ApplicationService.describeIdentity).mockRejectedValueOnce(new Error('metadata unavailable'));
    row.vm.$emit('explain');
    await flushPromises();
    expect(show).toHaveBeenCalledOnce();
    expect(show.mock.calls[0]?.[0].subject).not.toHaveProperty('identity');
    wrapper.unmount();
  });
  it('explains the default or current scope without starting uninstall and discards stale context', async () => {
    const pinia = createPinia();
    const ai = useAiStore(pinia);
    const show = vi.spyOn(ai, 'show').mockResolvedValue(undefined);
    const dismiss = vi.spyOn(ai, 'dismissModule');
    const wrapper = render(pinia);
    const row = wrapper.getComponent(Row);
    row.vm.$emit('explain');
    await flushPromises();
    expect(show).toHaveBeenLastCalledWith(
      expect.objectContaining({ subject: expect.objectContaining({ selectionKind: 'default' }) }),
      'en-US'
    );
    expect(wrapper.emitted('prepare')).toBeUndefined();
    expect(wrapper.emitted('execute')).toBeUndefined();
    expect(wrapper.emitted('recordRemoved')).toBeUndefined();

    row.vm.$emit('toggleSelection');
    await flushPromises();
    expect(dismiss).toHaveBeenCalledWith('applicationUninstall');
    row.vm.$emit('explain');
    await flushPromises();
    expect(show).toHaveBeenLastCalledWith(
      expect.objectContaining({
        subject: expect.objectContaining({
          selectionKind: 'current',
          components: [expect.objectContaining({ kind: 'nativeInstaller', selected: true })],
        }),
      }),
      'en-US'
    );

    await wrapper.setProps({ scanning: true });
    const count = show.mock.calls.length;
    row.vm.$emit('explain');
    await flushPromises();
    expect(show).toHaveBeenCalledTimes(count);
    expect(dismiss).toHaveBeenCalledWith('applicationUninstall');
    wrapper.unmount();
  });
});

describe('system application visibility', () => {
  it('hides only positive Windows classifications and preserves explicit uninstall capability', () => {
    expect(displayedApplications([regular, system, orphan], false)).toEqual([regular]);
    expect(displayedApplications([regular, system, orphan], true)).toEqual([regular, system, orphan]);
    expect(displayedApplications([{ ...system, platform: 'macosBundle' }], false)).toHaveLength(1);
    expect(applicationCanStartUninstall(system)).toBe(true);
    const codec = { ...system, systemKind: 'windowsSharedPackage' as const };
    expect(displayedApplications([codec], false)).toHaveLength(0);
    expect(displayedApplications([codec], true)).toEqual([codec]);
  });

  it('keeps summary, status counts, search and select-all within the chosen scope', async () => {
    const wrapper = render();
    expect(wrapper.findAllComponents(Row)).toHaveLength(1);
    expect(wrapper.getComponent(MdResultSummary).props('title')).toContain('1');
    wrapper.getComponent(MdResultCheckbox).vm.$emit('update:checked', true);
    await flushPromises();
    expect(wrapper.getComponent(MdSelectionActionBar).props('selectedValue')).toBe('1');
    await showSystem(wrapper, true);
    expect(wrapper.findAllComponents(Row)).toHaveLength(3);
    expect(wrapper.getComponent(MdResultSummary).props('title')).toContain('3');
    const options = wrapper.getComponent(MdCategoryFilter).props('options');
    expect(options.find((option: { value: string }) => option.value === 'ready')?.count).toBe(2);
    wrapper.getComponent(MdResultSearch).vm.$emit('update:modelValue', 'system');
    await flushPromises();
    expect(wrapper.findAllComponents(Row)).toHaveLength(1);
    await showSystem(wrapper, false);
    expect(wrapper.findAllComponents(Row)).toHaveLength(0);
    wrapper.unmount();
  });

  it('removes hidden applications and components from the next explicit batch', async () => {
    const wrapper = render();
    await showSystem(wrapper, true);
    wrapper.getComponent(MdResultCheckbox).vm.$emit('update:checked', true);
    await flushPromises();
    expect(wrapper.getComponent(MdSelectionActionBar).props('selectedValue')).toBe('2');
    await showSystem(wrapper, false);
    expect(wrapper.getComponent(MdSelectionActionBar).props('selectedValue')).toBe('1');
    wrapper.getComponent(MdSelectionActionBar).vm.$emit('action');
    await flushPromises();
    expect(wrapper.emitted('prepare')).toEqual([
      [[{ applicationId: regular.applicationId, componentIds: ['regular-installer'] }]],
    ]);
    expect(wrapper.getComponent(MdCheckbox).props('disabled')).toBe(true);
    const events = vi
      .mocked(LoggerService.info)
      .mock.calls.map(call => call[1])
      .join('\n');
    expect(events).toContain('hidden_system_count=2');
    expect(events).toContain('deselected_count=1');
    expect(events).not.toContain('private-');
    expect(events).not.toContain('Private publisher');
    wrapper.unmount();
  });

  it('keeps explicit system record removal independent of classification and catalog revision', async () => {
    const remove = vi.spyOn(ApplicationService, 'removeRecord').mockResolvedValue(undefined);
    const wrapper = render();
    await showSystem(wrapper, true);
    const row = wrapper
      .findAllComponents(Row)
      .find(row => row.props('candidate').applicationId === orphan.applicationId)!;
    row.vm.$emit('removeRecord');
    await flushPromises();
    wrapper.getComponent(MdDestructiveActionDialog).vm.$emit('confirm');
    await flushPromises();
    expect(remove).toHaveBeenCalledExactlyOnceWith(orphan.applicationId);
    expect(wrapper.emitted('scan')).toBeUndefined();
    expect(wrapper.emitted('recordRemoved')).toEqual([[orphan.applicationId]]);
    wrapper.unmount();
  });
});

describe('identity preparation cancellation', () => {
  it.each(['disable-reenable', 'close', 'configuration', 'selection', 'unmount', 'none'])(
    'honors identity preparation lifecycle for %s',
    async action => {
      const pinia = createPinia();
      const ai = useAiStore(pinia);
      ai.enabled = true;
      ai.preferencesLoaded = true;
      vi.spyOn(AiService, 'setEnabled').mockImplementation(async enabled => ({ schemaVersion: 1, enabled }));
      vi.spyOn(AiService, 'settings').mockResolvedValue({
        schemaVersion: 2,
        mode: 'custom',
        freeConsent: false,
        freeAvailable: false,
        endpoint: 'https://example.invalid/v1',
        model: 'example',
        hasKey: true,
        reasoning: 'default',
      });
      const generate = vi.spyOn(ai, 'generate').mockResolvedValue(undefined);
      const wrapper = render(pinia);
      let resolve!: (value: { schemaVersion: number; metadata: null }) => void;
      vi.mocked(ApplicationService.describeIdentity).mockReturnValueOnce(
        new Promise(done => {
          resolve = done;
        })
      );
      ai.workspaces.applicationUninstall.open = true;
      const row = wrapper.getComponent(Row);
      row.vm.$emit('explain');
      await flushPromises();
      expect(generate).not.toHaveBeenCalled();
      if (action === 'disable-reenable') {
        await ai.setEnabled(false);
        await ai.setEnabled(true);
      } else if (action === 'close') {
        await ai.close('applicationUninstall');
      } else if (action === 'configuration') {
        await ai.configurationChanged();
      } else if (action === 'unmount') {
        wrapper.unmount();
      } else if (action === 'selection') {
        row.vm.$emit('toggleSelection');
        await flushPromises();
      }
      resolve({ schemaVersion: 1, metadata: null });
      await flushPromises();
      const generated = generate.mock.calls.length;
      if (action !== 'unmount') wrapper.unmount();
      expect(generated).toBe(action === 'none' ? 1 : 0);
    }
  );
});

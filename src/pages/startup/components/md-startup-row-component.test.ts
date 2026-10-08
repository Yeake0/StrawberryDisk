// @vitest-environment happy-dom
import { createPinia } from 'pinia';
import { useAiStore } from '@/stores/ai-store';
import { mount } from '@vue/test-utils';
import { describe, expect, it } from 'vitest';
import { i18n } from '@/i18n';
import type { StartupArtifact, StartupOwnerGroup } from '@/lib/models/startup';
import MdStartupRow from './md-startup-row.vue';

const service: StartupArtifact = {
  itemId: 'service-fixture',
  sourceId: 'windows.services',
  sourceKind: 'service',
  scope: 'machine',
  triggers: ['boot'],
  displayName: 'Fixture service',
  configurationPath: null,
  target: { kind: 'service', path: null, executableName: null, arguments: [] },
  ownerName: 'Fixture',
  publisher: null,
  summary: null,
  summarySource: 'sourceLabel',
  version: null,
  iconPath: null,
  identityConfidence: 'strong',
  configuredState: 'enabled',
  runtimeState: 'running',
  controlCapability: 'elevationRequired',
  trust: 'unknown',
  modifiedAtMs: null,
  diagnostics: [],
  removalSupported: false,
  removableOrphan: false,
};
const group: StartupOwnerGroup = {
  groupId: 'fixture-group',
  name: 'Fixture',
  publisher: null,
  summary: null,
  summarySource: 'sourceLabel',
  version: null,
  iconPath: null,
  identityConfidence: 'strong',
  itemIds: [service.itemId],
  sourceKinds: ['service'],
  triggers: ['boot'],
  scopes: ['machine'],
  configuredState: 'allEnabled',
  controlState: 'requiresElevation',
  systemItem: false,
};
function row(artifacts: StartupArtifact[] = [service], expanded = false) {
  return mount(MdStartupRow, {
    props: {
      group,
      artifacts,
      subtitle: null,
      startTiming: 'At startup',
      state: 'enabled',
      revealPath: null,
      isWindows: true,
      isMacOs: false,
      expanded,
      busy: false,
      changing: false,
      copiedActionKey: null,
    },
    global: {
      plugins: [enabledPinia(), i18n],
      stubs: { MdApplicationIcon: true, MdIcon: true, MdIconAction: { template: '<button><slot /></button>' } },
    },
  });
}
describe('startup row source badges and service controls', () => {
  it.each(['toggleable', 'viewOnly', 'systemManaged'] as const)(
    'offers settings and identifies a stale macOS login record with %s capability',
    async controlCapability => {
      const artifact: StartupArtifact = {
        ...service,
        sourceId: 'macos.background_tasks',
        sourceKind: 'backgroundTask',
        controlCapability,
        diagnostics: ['missingTarget'],
        target: { kind: 'application', path: '/Applications/Missing.app/', executableName: null, arguments: [] },
      };
      const wrapper = row([artifact], true);
      await wrapper.setProps({ isWindows: false, isMacOs: true });
      const note = wrapper.get('.startup-management-note');
      expect(note.text()).toContain(i18n.global.t('startup.cleanup.macOsLoginRecordGuidance'));
      expect(note.get('strong').text()).toBe(i18n.global.t('startup.cleanup.macOsLoginRecordTitle'));
      expect(note.get('details').attributes('open')).toBeUndefined();
      expect(wrapper.find('.startup-detail-row.is-warning').exists()).toBe(false);
      expect(wrapper.text()).toContain(i18n.global.t('startup.cleanup.macOsLoginRecordLocation'));
      expect(wrapper.text()).toContain(i18n.global.t('startup.detail.missingTarget'));
      expect(wrapper.text()).toContain('/Applications/Missing.app/');
      expect(wrapper.find('.startup-cleanup-action').exists()).toBe(false);
      expect(wrapper.find('.startup-location-action').exists()).toBe(false);
      await note.get('button').trigger('click');
      expect(wrapper.emitted('openSystemSettings')).toEqual([[[artifact]]]);
      expect(wrapper.emitted('removeItems')).toBeUndefined();
      expect(wrapper.emitted('reveal')).toBeUndefined();
      wrapper.unmount();
    }
  );

  it('retains other diagnostics when the missing target is explained in the management note', async () => {
    const wrapper = row(
      [{ ...service, sourceKind: 'backgroundTask', diagnostics: ['missingTarget', 'accessDenied'] }],
      true
    );
    await wrapper.setProps({ isWindows: false, isMacOs: true });
    expect(wrapper.get('.startup-detail-row.is-warning dd').text()).toBe(
      i18n.global.t('startup.diagnostics.accessDenied')
    );
    wrapper.unmount();
  });

  it('offers direct deletion only for removable leftover records in a mixed group', async () => {
    const orphan: StartupArtifact = {
      ...service,
      itemId: 'removable-login-record',
      sourceKind: 'backgroundTask',
      controlCapability: 'removeOnly',
      diagnostics: ['missingTarget'],
      removalSupported: true,
      removableOrphan: true,
    };
    const liveAgent: StartupArtifact = {
      ...service,
      itemId: 'live-agent',
      sourceKind: 'launchAgent',
      controlCapability: 'toggleable',
      removalSupported: true,
    };
    const wrapper = row([orphan, liveAgent], true);
    await wrapper.setProps({ isWindows: false, isMacOs: true });
    const note = wrapper.get('.startup-management-note');
    expect(note.text()).toContain(i18n.global.t('startup.cleanup.macOsLoginRecordRemovable'));
    expect(note.find('details').exists()).toBe(false);
    expect(note.text()).not.toContain(i18n.global.t('startup.detail.openLoginItemsSettings'));
    await note.get('button').trigger('click');
    expect(wrapper.emitted('removeOrphans')).toEqual([[[orphan.itemId]]]);
    expect(wrapper.emitted('removeItems')).toBeUndefined();
    expect(wrapper.emitted('openSystemSettings')).toBeUndefined();
    await wrapper.setProps({ busy: true });
    expect(note.get('button').attributes('disabled')).toBeDefined();
    wrapper.unmount();
  });

  it('keeps a real configuration location available beside a stale macOS login record', async () => {
    const configurationPath = '/Library/LaunchAgents/example.plist';
    const wrapper = row(
      [
        { ...service, sourceKind: 'backgroundTask', controlCapability: 'viewOnly', diagnostics: ['missingTarget'] },
        { ...service, itemId: 'agent', sourceKind: 'launchAgent', configurationPath, removalSupported: true },
      ],
      true
    );
    await wrapper.setProps({ isWindows: false, isMacOs: true });
    expect(wrapper.text()).toContain(configurationPath);
    expect(wrapper.text()).toContain(i18n.global.t('startup.cleanup.macOsLoginRecordLocation'));
    expect(wrapper.find('.startup-cleanup-action').exists()).toBe(true);
    expect(wrapper.get('.startup-management-note').find('button').exists()).toBe(true);
    wrapper.unmount();
  });

  it('does not offer stale-record guidance for an installed macOS application', async () => {
    const wrapper = row([{ ...service, sourceKind: 'backgroundTask' }], true);
    await wrapper.setProps({ isWindows: false, isMacOs: true });
    expect(wrapper.find('.startup-management-note').exists()).toBe(false);
    expect(wrapper.text()).not.toContain(i18n.global.t('startup.cleanup.macOsLoginRecordLocation'));
    expect(wrapper.text()).toContain(i18n.global.t('startup.detail.command'));
    wrapper.unmount();
  });

  it('explains a collapsed group without toggling, removing or expanding it', async () => {
    const wrapper = row();
    await wrapper.get('.md-ai-action').trigger('click');
    expect(wrapper.emitted('explain')?.[0]).toEqual([group.name, [service]]);
    expect(wrapper.emitted('toggleGroup')).toBeUndefined();
    expect(wrapper.emitted('removeItems')).toBeUndefined();
    expect(wrapper.emitted('toggleExpanded')).toBeUndefined();
    wrapper.unmount();
  });

  it('explains protected services without offering a switch or a system-tool detour', () => {
    const wrapper = row([{ ...service, controlCapability: 'systemManaged' }], true);
    expect(wrapper.find('[role="switch"]').exists()).toBe(false);
    expect(wrapper.get('.startup-management-note').text()).toContain(i18n.global.t('startup.detail.protectedService'));
    expect(wrapper.get('.startup-management-note').find('button').exists()).toBe(false);
    expect(wrapper.get('.startup-state').text()).toBe(i18n.global.t('startup.configuredStates.enabled'));
    wrapper.unmount();
  });
  it('shows one compact source badge and a switch without a system-tool detour', async () => {
    const wrapper = row();
    expect(wrapper.findAll('.md-status-badge')).toHaveLength(1);
    expect(wrapper.get('.md-status-badge').text()).toBe(i18n.global.t('startup.sourceKinds.service'));
    await wrapper.get('[role="switch"]').trigger('click');
    expect(wrapper.emitted('toggleGroup')).toEqual([[]]);
    expect(wrapper.emitted('toggleExpanded')).toBeUndefined();
    expect(wrapper.text()).not.toContain(i18n.global.t('startup.detail.viewOnly'));
    wrapper.unmount();
  });
  it('keeps each grouped artifact switch independently actionable', async () => {
    const wrapper = row([service, { ...service, itemId: 'task-fixture', configuredState: 'disabled' }], true);
    await wrapper.setProps({ state: 'mixed' });

    await wrapper.get('.startup-native-row [role="switch"]').trigger('click');

    expect(wrapper.emitted('toggleArtifact')).toEqual([[service]]);
    expect(wrapper.emitted('toggleGroup')).toBeUndefined();
    wrapper.unmount();
  });
  it('keeps mixed source groups compact and identifies each expanded member', async () => {
    const artifacts = [service, { ...service, itemId: 'task', sourceKind: 'scheduledTask' as const }];
    const wrapper = row(artifacts);
    expect(wrapper.findAll('.md-status-badge')).toHaveLength(1);
    expect(wrapper.get('.md-status-badge').text()).toBe(i18n.global.t('startup.mixedSources'));
    expect(wrapper.get('.startup-item-count').text()).toBe(i18n.global.t('startup.itemCount', { count: 2 }));
    await wrapper.setProps({ expanded: true });
    expect(wrapper.findAll('.md-status-badge')).toHaveLength(3);
    wrapper.unmount();
  });
  it('shows partially enabled groups as a status instead of a boolean switch', async () => {
    const wrapper = row([service, { ...service, itemId: 'disabled-task', configuredState: 'disabled' }]);
    await wrapper.setProps({ state: 'mixed' });

    expect(wrapper.find('.startup-main [role="switch"]').exists()).toBe(false);
    expect(wrapper.get('.startup-main .startup-state').text()).toBe(i18n.global.t('startup.configuredStates.mixed'));
    expect(wrapper.get('.startup-main .startup-state').attributes('data-state')).toBe('mixed');
    wrapper.unmount();
  });
  it('keeps metadata in details and the single-item header uncluttered', async () => {
    const wrapper = row();
    await wrapper.setProps({ subtitle: 'A long service description', group: { ...group, version: '1.2.3' } });
    expect(wrapper.text()).not.toContain('A long service description');
    expect(wrapper.text()).not.toContain('1.2.3');
    expect(wrapper.get('.startup-item-count').text()).toBe('');
    expect(wrapper.find('.startup-disclosure .md-status-badge').exists()).toBe(false);
    expect(wrapper.get('.startup-source-slot .md-status-badge').text()).toBe(
      i18n.global.t('startup.sourceKinds.service')
    );
    await wrapper.setProps({ expanded: true });
    expect(wrapper.get('.startup-details').text()).toContain('A long service description');
    expect(wrapper.get('.startup-details').text()).toContain('1.2.3');
    wrapper.unmount();
  });
  it('keeps hover actions separate from expansion and switch events', async () => {
    const wrapper = row([{ ...service, removalSupported: true }]);
    await wrapper.setProps({ revealPath: 'C:\\Fixture\\app.exe' });
    await wrapper.get('.startup-location-action').trigger('click');
    await wrapper.get('.startup-cleanup-action').trigger('click');
    expect(wrapper.emitted('reveal')).toEqual([['C:\\Fixture\\app.exe']]);
    expect(wrapper.emitted('removeItems')).toEqual([[]]);
    expect(wrapper.emitted('toggleExpanded')).toBeUndefined();
    expect(wrapper.emitted('toggleGroup')).toBeUndefined();
    wrapper.unmount();
  });
  it('explains a disabled service that remains running and blocks duplicate clicks', async () => {
    const wrapper = row([{ ...service, configuredState: 'disabled' }], true);
    expect(wrapper.text()).toContain(i18n.global.t('startup.serviceStillRunning'));
    await wrapper.setProps({ busy: true, changing: true });
    expect(wrapper.get('[role="switch"]').attributes('disabled')).toBeDefined();
    expect(wrapper.findAll('.md-switch-spinner')).toHaveLength(1);
    wrapper.unmount();
  });
});

function enabledPinia() {
  const pinia = createPinia();
  useAiStore(pinia).$patch({ enabled: true, preferencesLoaded: true });
  return pinia;
}

it('removes the unused hover-action reservation when AI is disabled', async () => {
  const wrapper = row();
  expect(wrapper.find('.startup-actions').exists()).toBe(true);
  useAiStore().enabled = false;
  await wrapper.vm.$nextTick();
  expect(wrapper.find('.startup-actions').exists()).toBe(false);
  expect(wrapper.find('.md-ai-action').exists()).toBe(false);
  expect(wrapper.find('[role="switch"]').exists()).toBe(true);
  wrapper.unmount();
});

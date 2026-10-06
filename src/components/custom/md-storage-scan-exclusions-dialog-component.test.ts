// @vitest-environment happy-dom

import { flushPromises, mount } from '@vue/test-utils';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { i18n } from '@/i18n';
import { Select } from '@/components/ui/select';
import { LANGUAGE_IDS } from '@/lib/models/settings';
import { FileManagerService } from '@/lib/services/file-manager-service';
import { FolderSelectionService } from '@/lib/services/folder-selection-service';
import type { ScanExcludedFolder } from '@/lib/models/storage-scan';

const nativeDropMock = vi.hoisted(() => ({
  listener: undefined as
    ((event: { type: 'drop'; paths: string[]; position: { x: number; y: number } }) => void) | undefined,
}));

vi.mock('@/lib/services/native-drag-drop-service', () => ({
  NativeDragDropService: {
    listen: vi.fn(
      (listener: (event: { type: 'drop'; paths: string[]; position: { x: number; y: number } }) => void) => {
        nativeDropMock.listener = listener;
        return Promise.resolve(() => undefined);
      }
    ),
  },
}));

import MdStorageScanExclusionsDialog from './md-storage-scan-exclusions-dialog.vue';
import exclusionsDialogSource from './md-storage-scan-exclusions-dialog.vue?raw';

const passthroughStub = { template: '<div><slot /></div>' };
const dialogContentStub = {
  name: 'MdDialogContent',
  emits: ['interactOutside'],
  template: '<div><slot /></div>',
};
const dialogFooterStub = {
  name: 'MdDialogFooter',
  props: { align: String },
  template: '<footer :data-align="align"><slot /></footer>',
};
const buttonStub = {
  inheritAttrs: false,
  props: { disabled: Boolean },
  emits: ['click'],
  template: '<button :class="$attrs.class" :disabled="disabled" @click="$emit(\'click\', $event)"><slot /></button>',
};
const iconActionStub = {
  props: { label: String, disabled: Boolean },
  emits: ['click'],
  template: '<button :aria-label="label" :disabled="disabled" @click="$emit(\'click\', $event)"><slot /></button>',
};

function mountDialog(
  folders: ScanExcludedFolder[] = [
    { path: '/fixture/cache', scopes: ['cleanup', 'largeFiles', 'duplicateFiles'] },
    { path: '/fixture/downloads', scopes: ['largeFiles'] },
  ]
) {
  return mount(MdStorageScanExclusionsDialog, {
    props: {
      modelValue: true,
      folders,
      saving: false,
    },
    global: {
      plugins: [i18n],
      stubs: {
        Button: buttonStub,
        Dialog: passthroughStub,
        DialogDescription: passthroughStub,
        DialogTitle: passthroughStub,
        MdDialogContent: dialogContentStub,
        MdDialogFooter: dialogFooterStub,
        MdDialogHeader: passthroughStub,
        MdIcon: true,
        MdIconAction: iconActionStub,
      },
    },
  });
}

describe('storage-scan exclusions dialog', () => {
  afterEach(() => {
    nativeDropMock.listener = undefined;
    vi.restoreAllMocks();
  });

  it('adds an exact folder name and explicitly opts it into deep cleanup', async () => {
    const wrapper = mountDialog([]);
    await wrapper.findAll('.exclusion-tabs button')[1]!.trigger('click');
    await wrapper.get('.name-empty').trigger('click');
    await wrapper.get('.name-form input').setValue('node_modules');
    await wrapper.get('.name-form').trigger('submit');
    await wrapper.get('.name-scopes button').trigger('click');
    await wrapper.findAll('footer button').at(-1)!.trigger('click');
    expect(wrapper.emitted('save')).toEqual([
      [[], [{ name: 'node_modules', kind: 'folder', scopes: ['cleanup', 'largeFiles', 'duplicateFiles'] }]],
    ]);
  });

  it('discards a cancelled name and saves only confirmed additions', async () => {
    const wrapper = mountDialog([]);
    await wrapper.findAll('.exclusion-tabs button')[1]!.trigger('click');
    await wrapper.get('.name-empty').trigger('click');
    await wrapper.get('.name-form input').setValue('cancelled-name');
    await wrapper.get('.name-form footer button').trigger('click');
    await wrapper.get('.name-empty').trigger('click');
    expect(wrapper.get('.name-form input').element).toHaveProperty('value', '');
    await wrapper.get('.name-form input').setValue('node_modules');
    await wrapper.get('.name-form').trigger('submit');
    await wrapper.findAll('.exclusion-tabs button')[0]!.trigger('click');
    await wrapper.get('.exclusion-footer-actions').findAll('button').at(-1)!.trigger('click');
    expect(wrapper.emitted('save')).toEqual([
      [[], [{ name: 'node_modules', kind: 'folder', scopes: ['largeFiles', 'duplicateFiles'] }]],
    ]);
  });

  it('rejects patterns and preserves both tab drafts until save', async () => {
    const wrapper = mountDialog();
    await wrapper.findAll('.exclusion-tabs button')[1]!.trigger('click');
    await wrapper.get('.name-empty').trigger('click');
    await wrapper.get('.name-form input').setValue('*.tmp');
    await wrapper.get('.name-form').trigger('submit');
    expect(wrapper.get('[role="alert"]').text()).toContain('wildcards');
    await wrapper.get('.name-form input').setValue('.DS_Store');
    wrapper.getComponent(Select).vm.$emit('update:modelValue', 'file');
    await flushPromises();
    await wrapper.get('.name-form').trigger('submit');
    await wrapper.findAll('.exclusion-tabs button')[0]!.trigger('click');
    expect(wrapper.findAll('.exclusion-row')).toHaveLength(2);
    await wrapper.findAll('.exclusion-tabs button')[1]!.trigger('click');
    expect(wrapper.get('.name-value').text()).toBe('.DS_Store');
    await wrapper.setProps({ modelValue: false });
    await wrapper.setProps({ modelValue: true });
    expect(wrapper.findAll('.name-row')).toHaveLength(0);
  });

  it('uses legacy WebKit-safe semantic surfaces for folder interactions', () => {
    expect(exclusionsDialogSource).toContain('background: var(--surface-primary-subtle)');
    expect(exclusionsDialogSource).toContain('background: var(--surface-muted-subtle)');
    expect(exclusionsDialogSource).not.toContain('bg-primary/10');
    expect(exclusionsDialogSource).not.toContain('ring-primary/20');
  });

  it('renders every excluded folder in one bounded scroll region', () => {
    const wrapper = mountDialog();

    expect(wrapper.findAll('.exclusion-row')).toHaveLength(2);
    expect(wrapper.get('.exclusion-list').classes()).not.toContain('scrollbar-stable');
    expect(wrapper.get('.exclusion-drop-zone').classes()).not.toContain('empty');
    expect(wrapper.text()).toContain('/fixture/cache');
  });

  it('keeps the footer focused on the save and cancel actions', () => {
    const wrapper = mountDialog([{ path: '/fixture/cache', scopes: ['largeFiles'] }]);
    const footer = wrapper.get('.exclusion-footer');

    expect(footer.attributes('data-align')).toBeUndefined();
    expect(footer.find('.exclusion-note').exists()).toBe(false);
    expect(footer.get('.exclusion-footer-actions').findAll('button')).toHaveLength(2);
  });

  it('uses the navigation label and explains the narrower cleanup scope without toggling it', async () => {
    const wrapper = mountDialog();
    const firstScope = wrapper.get('.exclusion-scope-item');
    const checkbox = firstScope.get('[role="checkbox"]');
    const help = firstScope.get('.md-help-action');

    expect(firstScope.get('.exclusion-scope').text()).toContain('Deep Cleanup');
    expect(help.attributes('aria-label')).toBe(i18n.global.t('storageScanExclusions.cleanupScopeHint'));
    await help.trigger('click');
    await vi.waitFor(() =>
      expect(document.querySelector('[role="tooltip"]')?.textContent).toContain(
        'Other Deep Cleanup items may still clean its contents'
      )
    );
    expect(checkbox.attributes('aria-checked')).toBe('true');
    expect(wrapper.emitted('save')).toBeUndefined();
    wrapper.unmount();
  });

  it('keeps the cleanup label and help text localized in every supported language', async () => {
    const previousLocale = i18n.global.locale.value;
    const wrapper = mountDialog([{ path: '/fixture/cache', scopes: ['cleanup'] }]);
    try {
      for (const locale of Object.values(LANGUAGE_IDS)) {
        i18n.global.locale.value = locale;
        await flushPromises();
        const scope = wrapper.get('.exclusion-scope-item');
        const hint = i18n.global.t('storageScanExclusions.cleanupScopeHint');
        expect(scope.get('.exclusion-scope').text()).toContain(i18n.global.t('navigation.cleanup'));
        expect(scope.get('.md-help-action').attributes('aria-label')).toBe(hint);
        expect(hint).not.toBe('storageScanExclusions.cleanupScopeHint');
      }
    } finally {
      i18n.global.locale.value = previousLocale;
      wrapper.unmount();
    }
  });

  it('uses shared checkboxes and saves changed scopes', async () => {
    const wrapper = mountDialog();
    const checkboxes = wrapper.findAll('.exclusion-row [role="checkbox"]');

    expect(checkboxes).toHaveLength(8);
    expect(checkboxes[0]!.attributes('aria-checked')).toBe('true');
    await checkboxes[0]!.trigger('click');
    await wrapper.get('.exclusion-footer-actions').findAll('button').at(-1)!.trigger('click');

    expect(wrapper.emitted('save')).toEqual([
      [
        [
          { path: '/fixture/cache', scopes: ['largeFiles', 'duplicateFiles'] },
          { path: '/fixture/downloads', scopes: ['largeFiles'] },
        ],
        [],
      ],
    ]);
  });

  it('keeps the last enabled scope checked', () => {
    const wrapper = mountDialog([{ path: '/fixture/cache', scopes: ['largeFiles'] }]);
    const checkboxes = wrapper.findAll('.exclusion-row [role="checkbox"]');

    expect(checkboxes[1]!.attributes('aria-checked')).toBe('true');
    expect(checkboxes[1]!.attributes('disabled')).toBeDefined();
  });

  it('allows opting a folder into space analysis without changing its other scopes', async () => {
    const wrapper = mountDialog([{ path: '/fixture/cache', scopes: ['largeFiles'] }]);
    const checkboxes = wrapper.findAll('.exclusion-row [role="checkbox"]');

    expect(checkboxes[3]!.attributes('aria-checked')).toBe('false');
    await checkboxes[3]!.trigger('click');
    await wrapper.get('.exclusion-footer-actions').findAll('button').at(-1)!.trigger('click');

    expect(wrapper.emitted('save')).toEqual([[[{ path: '/fixture/cache', scopes: ['largeFiles', 'analysis'] }], []]]);
  });

  it('removes a folder from the draft and saves the remaining list', async () => {
    const wrapper = mountDialog();

    await wrapper.get('[aria-label="Remove this folder"]').trigger('click');
    await wrapper.findAll('button').at(-1)?.trigger('click');

    expect(wrapper.emitted('save')).toEqual([[[{ path: '/fixture/downloads', scopes: ['largeFiles'] }], []]]);
    expect(wrapper.text()).toContain('Save');
  });

  it('keeps the empty folder region clickable without changing saved data until confirmation', async () => {
    const select = vi.spyOn(FolderSelectionService, 'select').mockResolvedValue([]);
    const wrapper = mountDialog([]);

    expect(wrapper.get('.exclusion-drop-zone').classes()).toContain('empty');
    expect(wrapper.get('.exclusion-empty-action').text()).toContain('Click to add folders, or drag them here');
    await wrapper.get('.exclusion-empty-action').trigger('click');

    expect(select).toHaveBeenCalledOnce();
    expect(wrapper.emitted('save')).toBeUndefined();
  });

  it('deduplicates folders returned in one picker selection', async () => {
    vi.spyOn(FolderSelectionService, 'select').mockResolvedValue(['/fixture/cache', '/fixture/cache']);
    vi.spyOn(FolderSelectionService, 'filterExistingDirectories').mockResolvedValue([
      '/fixture/cache',
      '/fixture/cache',
    ]);
    const wrapper = mountDialog([]);

    await wrapper.get('.exclusion-empty-action').trigger('click');
    await flushPromises();

    expect(wrapper.findAll('.exclusion-row')).toHaveLength(1);
    expect(wrapper.get('.exclusion-row').findAll('[role="checkbox"]')).toHaveLength(4);
    expect(wrapper.get('.exclusion-row').findAll('[role="checkbox"]')[3]!.attributes('aria-checked')).toBe('false');
  });

  it('waits for folder validation before allowing the draft to be saved', async () => {
    let finishSelection: (paths: string[]) => void = () => undefined;
    vi.spyOn(FolderSelectionService, 'select').mockImplementation(
      () =>
        new Promise(resolve => {
          finishSelection = resolve;
        })
    );
    vi.spyOn(FolderSelectionService, 'filterExistingDirectories').mockResolvedValue(['/fixture/selected']);
    const wrapper = mountDialog([]);
    const saveButton = wrapper.get('.exclusion-footer-actions').findAll('button').at(-1)!;

    await wrapper.get('.exclusion-empty-action').trigger('click');
    expect(saveButton.attributes('disabled')).toBeDefined();

    finishSelection(['/fixture/selected']);
    await flushPromises();
    expect(saveButton.attributes('disabled')).toBeUndefined();
    await saveButton.trigger('click');
    expect(wrapper.emitted('save')).toEqual([
      [[{ path: '/fixture/selected', scopes: ['cleanup', 'largeFiles', 'duplicateFiles'] }], []],
    ]);
  });

  it('adds dropped directories to the same scrollable list region', async () => {
    vi.spyOn(FolderSelectionService, 'filterExistingDirectories').mockResolvedValue(['/fixture/dropped']);
    const wrapper = mountDialog([]);
    await flushPromises();
    vi.spyOn(wrapper.get('.exclusion-drop-zone').element, 'getBoundingClientRect').mockReturnValue({
      left: 0,
      right: 300,
      top: 0,
      bottom: 150,
    } as DOMRect);

    nativeDropMock.listener?.({ type: 'drop', paths: ['/fixture/dropped'], position: { x: 40, y: 40 } });
    await flushPromises();

    expect(wrapper.get('.exclusion-drop-zone').classes()).not.toContain('empty');
    expect(wrapper.get('.exclusion-list').text()).toContain('/fixture/dropped');
  });

  it('ignores a native folder drop outside the list region', async () => {
    const filterDirectories = vi
      .spyOn(FolderSelectionService, 'filterExistingDirectories')
      .mockResolvedValue(['/fixture/dropped']);
    const wrapper = mountDialog([]);
    await flushPromises();
    vi.spyOn(wrapper.get('.exclusion-drop-zone').element, 'getBoundingClientRect').mockReturnValue({
      left: 0,
      right: 300,
      top: 0,
      bottom: 150,
    } as DOMRect);

    nativeDropMock.listener?.({ type: 'drop', paths: ['/fixture/dropped'], position: { x: 400, y: 40 } });
    await flushPromises();

    expect(filterDirectories).not.toHaveBeenCalled();
    expect(wrapper.get('.exclusion-drop-zone').classes()).toContain('empty');
  });

  it('reveals an excluded folder through the shared file manager service', async () => {
    const reveal = vi.spyOn(FileManagerService, 'reveal').mockResolvedValue();
    const wrapper = mountDialog();

    await wrapper.get('[aria-label="Show in File Manager"]').trigger('click');

    expect(reveal).toHaveBeenCalledWith('/fixture/cache');
  });

  it('prevents an outside interaction from dismissing an unsaved draft', () => {
    const wrapper = mountDialog();
    const preventDefault = vi.fn();

    wrapper.getComponent({ name: 'MdDialogContent' }).vm.$emit('interactOutside', { preventDefault });

    expect(preventDefault).toHaveBeenCalledOnce();
    expect(wrapper.emitted('update:modelValue')).toBeUndefined();
  });
});

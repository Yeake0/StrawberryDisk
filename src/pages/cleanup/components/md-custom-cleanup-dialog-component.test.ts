// @vitest-environment happy-dom

import { flushPromises, mount } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n } from '@/i18n';
import type { CustomCleanupRule } from '@/lib/models/custom-cleanup';
import { FolderSelectionService } from '@/lib/services/folder-selection-service';
import { NativeDragDropService, type NativeDragDropEvent } from '@/lib/services/native-drag-drop-service';
import { useCustomCleanupStore } from '@/stores/custom-cleanup-store';

import MdCustomCleanupDialog from './md-custom-cleanup-dialog.vue';
import customCleanupDialogSource from './md-custom-cleanup-dialog.vue?raw';

const passthroughStub = { template: '<div><slot /></div>' };
const buttonStub = {
  inheritAttrs: false,
  props: { disabled: Boolean },
  emits: ['click'],
  template: '<button :class="$attrs.class" :disabled="disabled" @click="$emit(\'click\', $event)"><slot /></button>',
};
const iconActionStub = {
  inheritAttrs: false,
  props: { disabled: Boolean, label: String },
  emits: ['click'],
  template: '<button :aria-label="label" :disabled="disabled" @click="$emit(\'click\', $event)"><slot /></button>',
};

function fixtureRule(): CustomCleanupRule {
  return {
    schemaVersion: 1,
    id: 'rule-1',
    name: 'Temporary logs',
    roots: ['/fixture/logs'],
    namePatterns: ['*.log'],
    minimumBytes: null,
    maximumBytes: null,
    modifiedTime: { mode: 'any' },
    recursive: true,
    removeEmptyDirectories: false,
  };
}

function mountDialog() {
  return mount(MdCustomCleanupDialog, {
    props: { modelValue: false },
    global: {
      plugins: [i18n],
      stubs: {
        Button: buttonStub,
        Checkbox: passthroughStub,
        Dialog: passthroughStub,
        DialogDescription: passthroughStub,
        DialogTitle: passthroughStub,
        Input: passthroughStub,
        MdDialogContent: passthroughStub,
        MdDialogFooter: passthroughStub,
        MdDialogHeader: passthroughStub,
        MdIcon: true,
        MdIconAction: iconActionStub,
        Select: passthroughStub,
        SelectContent: passthroughStub,
        SelectItem: passthroughStub,
        SelectTrigger: passthroughStub,
        SelectValue: passthroughStub,
        Tooltip: passthroughStub,
        TooltipContent: passthroughStub,
        TooltipTrigger: passthroughStub,
      },
    },
  });
}

describe('custom cleanup dialog component', () => {
  let nativeDropHandler: ((event: NativeDragDropEvent) => void) | undefined;

  beforeEach(() => {
    setActivePinia(createPinia());
    nativeDropHandler = undefined;
    vi.spyOn(NativeDragDropService, 'listen').mockImplementation(async handler => {
      nativeDropHandler = handler;
      return () => undefined;
    });
    vi.spyOn(FolderSelectionService, 'filterExistingDirectories').mockImplementation(async paths => [...paths]);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('keeps editor surfaces and numeric fields compatible with legacy WebKit', () => {
    expect(customCleanupDialogSource).toContain('background: var(--surface-primary-subtle)');
    expect(customCleanupDialogSource).toMatch(/\.directory-drop-zone\s*\{[\s\S]*?background: transparent;/);
    expect(customCleanupDialogSource).toContain(".compact-control[type='number']::-webkit-inner-spin-button");
    expect(customCleanupDialogSource).not.toContain('bg-muted/10');
    expect(customCleanupDialogSource).not.toContain('bg-muted/20');
    expect(customCleanupDialogSource).not.toContain('background: color-mix');
  });

  it('applies a native directory drop to the active rule rendered by the dialog', async () => {
    const store = useCustomCleanupStore();
    store.initialized = true;
    store.rules = [fixtureRule()];
    const wrapper = mountDialog();
    await flushPromises();
    await wrapper.setProps({ modelValue: true });
    await flushPromises();

    nativeDropHandler?.({
      type: 'drop',
      paths: ['/fixture/new-logs'],
      position: { x: 20, y: 40 } as Extract<NativeDragDropEvent, { type: 'drop' }>['position'],
    });
    await flushPromises();

    expect(FolderSelectionService.filterExistingDirectories).toHaveBeenCalledWith(['/fixture/new-logs']);
    expect(wrapper.text()).toContain('/fixture/new-logs');
    wrapper.unmount();
  });

  it('persists a valid rule and emits a scan request through the footer action', async () => {
    const store = useCustomCleanupStore();
    store.initialized = true;
    store.rules = [fixtureRule()];
    const save = vi.spyOn(store, 'save').mockResolvedValue();
    const wrapper = mountDialog();
    await wrapper.setProps({ modelValue: true });
    await flushPromises();

    await wrapper.get('.dialog-actions button:last-child').trigger('click');
    await flushPromises();

    expect(save).toHaveBeenCalledWith([fixtureRule()], true);
    expect(wrapper.emitted('scan')).toEqual([[[fixtureRule()], true]]);
    expect(wrapper.emitted('update:modelValue')).toEqual([[false]]);
    wrapper.unmount();
  });

  it('keeps an invalid new rule open and renders validation feedback instead of scanning', async () => {
    const store = useCustomCleanupStore();
    store.initialized = true;
    store.rules = [];
    const save = vi.spyOn(store, 'save');
    const wrapper = mountDialog();
    await wrapper.setProps({ modelValue: true });
    await flushPromises();

    await wrapper.get('.dialog-actions button:last-child').trigger('click');
    await flushPromises();

    expect(save).not.toHaveBeenCalled();
    expect(wrapper.emitted('scan')).toBeUndefined();
    expect(wrapper.findAll('.field-error').length).toBeGreaterThan(0);
    wrapper.unmount();
  });
});

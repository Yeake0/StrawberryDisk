// @vitest-environment happy-dom

import { mount } from '@vue/test-utils';
import { describe, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/plugin-os', () => ({ platform: () => 'macos' }));

import { i18n } from '@/i18n';
import {
  DUPLICATE_ENTRY_DELETE_POLICIES,
  DUPLICATE_GROUP_KINDS,
  DUPLICATE_KEEPER_RULE_IDS,
  type DuplicateFileEntry,
  type DuplicateGroup,
} from '@/lib/models/duplicate-file';
import { FILE_CATEGORY_IDS } from '@/lib/models/file-category';

import MdDuplicateFileGroups from './md-duplicate-file-groups.vue';

const entries: DuplicateFileEntry[] = ['work-a.pdf', 'work-b.pdf', 'chat-a.pdf', 'chat-b.pdf'].map((name, index) => ({
  name,
  path: `/${name}`,
  parentPath: '/',
  bytes: 1024,
  allocatedBytes: 1024,
  modifiedAtMs: index,
  deletePolicy: DUPLICATE_ENTRY_DELETE_POLICIES.protected,
}));

function groupWithEntries(groupEntries: DuplicateFileEntry[]): DuplicateGroup {
  return {
    id: 'group-1',
    hash: 'hash-1',
    kind: DUPLICATE_GROUP_KINDS.file,
    bytesPerFile: 1024,
    fileCountPerEntry: 1,
    reclaimableBytes: 0,
    entries: groupEntries,
  };
}

function mountGroups(group: DuplicateGroup, selectedPaths: string[] = []) {
  return mount(MdDuplicateFileGroups, {
    props: {
      scanId: 1,
      category: FILE_CATEGORY_IDS.all,
      groups: [group],
      keeperRule: DUPLICATE_KEEPER_RULE_IDS.shortestPath,
      selectedPaths,
      selectionDisabled: false,
      openDisabled: false,
      deleteDisabled: false,
      hasMore: false,
      loadingMore: false,
      remainingGroupCount: 0,
    },
    global: {
      plugins: [i18n],
      stubs: {
        MdResultTable: { template: '<div><slot /></div>' },
        MdResultTableHierarchy: { template: '<div><slot /><slot name="footer" /></div>' },
        MdFileEntryContextMenu: { template: '<div><slot /></div>' },
        MdResultTableRow: { template: '<div><slot /></div>' },
        MdMiddleEllipsis: { props: ['text'], template: '<span>{{ text }}</span>' },
        MdNativeFileIcon: true,
        MdResultCheckbox: true,
        MdAiAction: {
          props: ['name', 'disabled'],
          template: '<button class="explain-entry" :disabled="disabled" @click="$emit(\'explain\')">Explain</button>',
        },
        MdIconAction: true,
        MdIcon: true,
        MdLoadMoreButton: true,
      },
    },
  });
}

describe('duplicate file groups component', () => {
  it('shows an unavailable protected state instead of an applied empty selection', () => {
    const wrapper = mountGroups(groupWithEntries(entries));
    const action = wrapper.get('button.group-select-action');

    expect(action.text()).toBe('All protected');
    expect(action.attributes('disabled')).toBeDefined();
    expect(action.attributes('data-applied')).toBe('false');
    expect(action.attributes('aria-label')).toContain('Every file in this group is protected');
  });

  it('counts only cleanable copies in a mixed protected group', () => {
    const mixedEntries = entries.map((entry, index) => ({
      ...entry,
      deletePolicy: index < 2 ? DUPLICATE_ENTRY_DELETE_POLICIES.protected : DUPLICATE_ENTRY_DELETE_POLICIES.cleanable,
    }));
    const selectedPaths = mixedEntries.slice(2).map(entry => entry.path);
    const wrapper = mountGroups(groupWithEntries(mixedEntries), selectedPaths);
    const action = wrapper.get('button.group-select-action');

    expect(action.text()).toBe('2 selected');
    expect(action.attributes('disabled')).toBeUndefined();
    expect(action.attributes('data-applied')).toBe('true');
  });
});

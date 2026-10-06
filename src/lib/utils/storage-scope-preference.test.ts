import { describe, expect, it } from 'vitest';

import { MAX_RECENT_STORAGE_FOLDERS } from '@/lib/models/storage-scope';

import * as StorageScopePreferenceUtils from './storage-scope-preference';

describe('StorageScopePreferenceUtils', () => {
  it('migrates original selections while retaining single-selection analysis', () => {
    expect(
      StorageScopePreferenceUtils.parse({
        selectedPaths: { analysis: '/data', 'duplicate-files': '/work' },
        recentFolders: ['/work'],
      })
    ).toEqual({
      schemaVersion: 3,
      selectedPaths: { analysis: '/data', 'duplicate-files': ['/work'] },
      recentFolders: ['/work'],
      duplicateFileProtectedPaths: [],
    });
  });

  it('migrates version 1 large-file selections and preserves empty version 2 selections', () => {
    expect(
      StorageScopePreferenceUtils.parse({
        schemaVersion: 1,
        selectedPaths: { 'large-files': '/work', 'duplicate-files': ['/chat'] },
        recentFolders: ['/work'],
      })
    ).toEqual({
      schemaVersion: 3,
      selectedPaths: { 'large-files': ['/work'], 'duplicate-files': ['/chat'] },
      recentFolders: ['/work'],
      duplicateFileProtectedPaths: [],
    });
    expect(
      StorageScopePreferenceUtils.parse({
        schemaVersion: 2,
        selectedPaths: { 'large-files': [] },
        recentFolders: [],
      }).selectedPaths['large-files']
    ).toEqual([]);
    expect(() =>
      StorageScopePreferenceUtils.parse({
        schemaVersion: 1,
        selectedPaths: { 'large-files': ['/work'] },
        recentFolders: [],
      })
    ).toThrow();
  });

  it('preserves explicit empty selections and selections larger than recent history', () => {
    const paths = Array.from({ length: 12 }, (_, index) => `/work/${index}`);
    expect(
      StorageScopePreferenceUtils.parse({
        schemaVersion: 2,
        selectedPaths: { 'duplicate-files': paths },
        recentFolders: [],
      }).selectedPaths['duplicate-files']
    ).toEqual(paths);
    expect(
      StorageScopePreferenceUtils.parse({
        schemaVersion: 2,
        selectedPaths: { 'duplicate-files': [] },
        recentFolders: [],
      }).selectedPaths['duplicate-files']
    ).toEqual([]);
  });

  it('rejects unsupported versions, invalid items, and multi-selection on single-scope pages', () => {
    for (const value of [
      { schemaVersion: 3, selectedPaths: {}, recentFolders: [] },
      { schemaVersion: 2, selectedPaths: { 'duplicate-files': [null] }, recentFolders: [] },
      { schemaVersion: 2, selectedPaths: { analysis: ['/work'] }, recentFolders: [] },
    ])
      expect(() => StorageScopePreferenceUtils.parse(value)).toThrow();
  });
  it('parses the current storage scope document', () => {
    expect(
      StorageScopePreferenceUtils.parse({
        selectedPaths: {
          analysis: '/Users/example/Downloads',
        },
        recentFolders: ['C:\\Users\\example\\Downloads', '/Users/example/Downloads'],
      })
    ).toEqual({
      schemaVersion: 3,
      selectedPaths: {
        analysis: '/Users/example/Downloads',
      },
      recentFolders: ['C:\\Users\\example\\Downloads', '/Users/example/Downloads'],
      duplicateFileProtectedPaths: [],
    });
  });

  it('retains protected duplicate roots in the current document', () => {
    expect(
      StorageScopePreferenceUtils.parse({
        schemaVersion: 3,
        selectedPaths: { 'duplicate-files': ['E:\\Work', 'F:\\Chat'] },
        recentFolders: ['E:\\Work', 'F:\\Chat'],
        duplicateFileProtectedPaths: ['e:/work/'],
      })
    ).toEqual({
      schemaVersion: 3,
      selectedPaths: { 'duplicate-files': ['E:\\Work', 'F:\\Chat'] },
      recentFolders: ['E:\\Work', 'F:\\Chat'],
      duplicateFileProtectedPaths: ['e:/work/'],
    });
  });

  it('rejects obsolete or partially valid storage scope documents', () => {
    expect(() =>
      StorageScopePreferenceUtils.parse({
        selectedPaths: {
          analysis: '/Users/example/Downloads',
          unknown: '/ignored',
        },
        recentFolders: ['/Users/example/Downloads'],
      })
    ).toThrow('Invalid storage scope selection');
  });

  it('moves a selected folder to the front and caps history size', () => {
    const existing = Array.from({ length: MAX_RECENT_STORAGE_FOLDERS }, (_, index) => `/workspace/${index}`);

    expect(StorageScopePreferenceUtils.addRecentFolder(existing, '/workspace/new')).toEqual([
      '/workspace/new',
      ...existing.slice(0, MAX_RECENT_STORAGE_FOLDERS - 1),
    ]);
    expect(StorageScopePreferenceUtils.addRecentFolder(existing, '/workspace/3')[0]).toBe('/workspace/3');
  });

  it('removes equivalent Windows paths without affecting other entries', () => {
    expect(
      StorageScopePreferenceUtils.removePath(
        ['C:\\Users\\example\\Downloads', 'D:\\Projects'],
        'c:/Users/example/Downloads/'
      )
    ).toEqual(['D:\\Projects']);
  });
});

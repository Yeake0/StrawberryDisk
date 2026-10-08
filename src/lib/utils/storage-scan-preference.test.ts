import { describe, expect, it } from 'vitest';

import { MAX_SCAN_EXCLUDED_FOLDERS, SCAN_EXCLUSION_PREFERENCES_SCHEMA_VERSION } from '@/lib/models/storage-scan';
import * as StorageScanPreferenceUtils from '@/lib/utils/storage-scan-preference';

describe('StorageScanPreferenceUtils', () => {
  it('migrates v2 without inventing name rules or widening scopes', () => {
    expect(
      StorageScanPreferenceUtils.parse({ schemaVersion: 2, folders: [{ path: '/fixture', scopes: ['cleanup'] }] })
    ).toEqual({ schemaVersion: 3, folders: [{ path: '/fixture', scopes: ['cleanup'] }], names: [] });
  });

  it('validates exact names and compares name policies independently of rule order', () => {
    for (const name of ['', '.', '..', '*.tmp', 'a/b', 'a\\b', ' cache', 'a\nfile'])
      expect(StorageScanPreferenceUtils.validExcludedName(name)).toBe(false);
    const names = [
      { name: 'node_modules', kind: 'folder' as const, scopes: ['cleanup' as const] },
      { name: '.DS_Store', kind: 'file' as const, scopes: ['analysis' as const] },
    ];
    const parsed = StorageScanPreferenceUtils.parse({ schemaVersion: 3, folders: [], names });
    expect(StorageScanPreferenceUtils.namesForScope(parsed.names, 'cleanup')).toEqual([
      { name: 'node_modules', kind: 'folder' },
    ]);
    expect(StorageScanPreferenceUtils.sameExcludedNames(names, [...names].reverse())).toBe(true);
    expect(StorageScanPreferenceUtils.sameExcludedNames(names, [{ name: 'NODE_MODULES', kind: 'folder' }])).toBe(false);
    expect(() =>
      StorageScanPreferenceUtils.parse({ schemaVersion: 3, folders: [], names: [...names, names[0]] })
    ).toThrow('namesInvalid');
    expect(() =>
      StorageScanPreferenceUtils.parse({
        schemaVersion: 3,
        folders: [],
        names: [{ name: 'cache', kind: 'file', scopes: [] }],
      })
    ).toThrow('namesInvalid');
  });

  it('normalizes paths and collapses nested exclusions only within each scope', () => {
    expect(
      StorageScanPreferenceUtils.parse({
        schemaVersion: SCAN_EXCLUSION_PREFERENCES_SCHEMA_VERSION,
        names: [],
        folders: [
          { path: '/tmp/cache/nested', scopes: ['cleanup'] },
          { path: '/tmp/cache', scopes: ['cleanup', 'largeFiles'] },
          { path: '/tmp/downloads', scopes: ['duplicateFiles'] },
          { path: '/tmp/archive', scopes: ['analysis'] },
        ],
      })
    ).toEqual({
      schemaVersion: SCAN_EXCLUSION_PREFERENCES_SCHEMA_VERSION,
      names: [],
      folders: [
        { path: '/tmp/cache/nested', scopes: ['cleanup'] },
        { path: '/tmp/cache', scopes: ['cleanup', 'largeFiles'] },
        { path: '/tmp/downloads', scopes: ['duplicateFiles'] },
        { path: '/tmp/archive', scopes: ['analysis'] },
      ],
    });
    expect(
      StorageScanPreferenceUtils.pathsForScope(
        [
          { path: '/tmp/cache/nested', scopes: ['cleanup'] },
          { path: '/tmp/cache', scopes: ['cleanup', 'largeFiles'] },
        ],
        'cleanup'
      )
    ).toEqual(['/tmp/cache']);
    expect(
      StorageScanPreferenceUtils.pathsForScope(
        [
          { path: '/tmp/cache', scopes: ['largeFiles'] },
          { path: '/tmp/archive', scopes: ['analysis'] },
        ],
        'analysis'
      )
    ).toEqual(['/tmp/archive']);
  });

  it('normalizes Windows extended path prefixes before persistence', () => {
    expect(
      StorageScanPreferenceUtils.parse({
        schemaVersion: SCAN_EXCLUSION_PREFERENCES_SCHEMA_VERSION,
        names: [],
        folders: [{ path: '\\\\?\\C:\\Users\\fixture\\Downloads', scopes: ['largeFiles'] }],
      }).folders[0]?.path
    ).toBe('C:\\Users\\fixture\\Downloads');
  });

  it('rejects malformed and oversized preference documents', () => {
    expect(() => StorageScanPreferenceUtils.parse({ schemaVersion: 0, folders: [] })).toThrowError(
      'schemaVersionMismatch'
    );
    expect(() =>
      StorageScanPreferenceUtils.parse({
        schemaVersion: SCAN_EXCLUSION_PREFERENCES_SCHEMA_VERSION,
        names: [],
        folders: Array.from({ length: MAX_SCAN_EXCLUDED_FOLDERS + 1 }, (_, index) => ({
          path: `/tmp/${index}`,
          scopes: ['cleanup'],
        })),
      })
    ).toThrowError('foldersInvalid');
    expect(() =>
      StorageScanPreferenceUtils.parse({ schemaVersion: 2, folders: [{ path: '/tmp/a', scopes: ['unknown'] }] })
    ).toThrowError('foldersInvalid');
  });

  it('migrates legacy schemas without widening their scopes', () => {
    const legacy = { schemaVersion: 1, excludedFolders: ['/tmp/a'] };
    expect(StorageScanPreferenceUtils.fromLargeFileV1(legacy).folders[0]?.scopes).toEqual(['largeFiles']);
    expect(StorageScanPreferenceUtils.fromSharedV1(legacy).folders[0]?.scopes).toEqual([
      'largeFiles',
      'duplicateFiles',
    ]);
    expect(
      StorageScanPreferenceUtils.pathsForScope(StorageScanPreferenceUtils.fromSharedV1(legacy).folders, 'analysis')
    ).toEqual([]);
  });

  it('compares equivalent lists without exposing path details', () => {
    expect(StorageScanPreferenceUtils.sameExcludedFolders(['/tmp/a', '/tmp/b'], ['/tmp/b', '/tmp/a'])).toBe(true);
    expect(StorageScanPreferenceUtils.errorCode(new Error('/private/path'))).toBe('unexpected');
  });

  it('detects only exclusions that overlap the scanned roots', () => {
    const overlaps = StorageScanPreferenceUtils.hasExcludedFolderInScanRoots;
    expect(overlaps(['/scan'], [])).toBe(false);
    expect(overlaps([], ['/scan/private'])).toBe(false);
    expect(overlaps(['/scan'], ['/other/private'])).toBe(false);
    expect(overlaps(['/scan'], ['/scan-old/private'])).toBe(false);
    expect(overlaps(['/scan'], ['/scan/private'])).toBe(true);
    expect(overlaps(['/scan/private'], ['/scan'])).toBe(true);
    expect(overlaps(['C:\\Users\\Fixture\\Downloads'], ['c:/users/fixture/downloads/private'])).toBe(true);
  });
});

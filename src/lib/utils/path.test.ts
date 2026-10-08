import { describe, expect, it } from 'vitest';

import * as PathUtils from '@/lib/utils/path';

describe('PathUtils.display', () => {
  it('removes Windows verbatim prefixes without depending on UNC prefix casing', () => {
    expect(PathUtils.display('\\\\?\\C:\\Users\\Developer')).toBe('C:\\Users\\Developer');
    expect(PathUtils.display('\\\\?\\unc\\Server\\Share')).toBe('\\\\Server\\Share');
  });

  it('keeps an absolute Windows drive root distinct from a drive-relative path', () => {
    expect(PathUtils.comparisonKey('C:\\')).toBe('c:\\');
    expect(PathUtils.comparisonKey('C:')).toBe('c:');
    expect(PathUtils.isSameOrChildKey(PathUtils.comparisonKey('C:\\fixture'), PathUtils.comparisonKey('C:\\'))).toBe(
      true
    );
  });
});

describe('PathUtils.collapseOverlappingRoots', () => {
  it('compares equivalent scan coverage after root ordering and overlap changes', () => {
    expect(PathUtils.sameRootScope(['F:\\Chat', 'E:\\Work'], ['e:/work/child', 'f:/chat/', 'E:\\Work'])).toBe(true);
    expect(PathUtils.sameRootScope(['/work', '/chat'], ['/work', '/chat-old'])).toBe(false);
    expect(PathUtils.sameRootScope(['/Work'], ['/work'])).toBe(false);
    expect(PathUtils.sameRootScope([], ['/work'])).toBe(false);
  });
  it('ignores descendants already covered by a selected parent', () => {
    expect(
      PathUtils.collapseOverlappingRoots([
        '/Users/developer/Downloads',
        '/Users/developer/Downloads/projects',
        '/Users/developer/Documents',
      ])
    ).toEqual(['/Users/developer/Downloads', '/Users/developer/Documents']);
  });

  it('replaces earlier descendants when a parent is added later', () => {
    expect(
      PathUtils.collapseOverlappingRoots([
        '/Users/developer/Downloads/projects',
        '/Users/developer/Downloads/assets',
        '/Users/developer/Downloads',
      ])
    ).toEqual(['/Users/developer/Downloads']);
  });

  it('compares Windows roots without case or separator differences', () => {
    expect(
      PathUtils.collapseOverlappingRoots([
        'C:\\Users\\Developer\\Downloads',
        'c:/users/developer/downloads/projects',
        'D:\\Archive',
      ])
    ).toEqual(['C:\\Users\\Developer\\Downloads', 'D:\\Archive']);
  });

  it('keeps siblings whose names only share a prefix', () => {
    expect(
      PathUtils.collapseOverlappingRoots(['/Users/developer/Downloads', '/Users/developer/Downloads-archive'])
    ).toEqual(['/Users/developer/Downloads', '/Users/developer/Downloads-archive']);
  });

  it('does not treat a valid backslash in a Unix file name as a path separator', () => {
    expect(PathUtils.collapseOverlappingRoots(['/Users/developer/a\\b', '/Users/developer/a/b'])).toEqual([
      '/Users/developer/a\\b',
      '/Users/developer/a/b',
    ]);
  });
});

describe('explicit scan locations', () => {
  it('preserves mounted descendants while removing duplicate path aliases', () => {
    expect(PathUtils.uniquePaths(['/', '/Volumes/External', '/', '/Volumes/External/'])).toEqual([
      '/',
      '/Volumes/External',
    ]);
    expect(PathUtils.uniquePaths(['C:\\', 'C:\\Mount', 'c:/mount/', 'D:\\'])).toEqual(['C:\\', 'C:\\Mount', 'D:\\']);
  });

  it('does not present a missing mounted volume as matching the current selection', () => {
    expect(PathUtils.sameSelectedPaths(['/'], ['/', '/Volumes/External'])).toBe(false);
    expect(PathUtils.sameSelectedPaths(['/', '/Volumes/External'], ['/'])).toBe(false);
    expect(PathUtils.sameSelectedPaths(['/Volumes/External', '/'], ['/', '/Volumes/External/'])).toBe(true);
    expect(PathUtils.sameSelectedPaths(['E:\\Work', 'F:\\Chat'], ['f:/chat/', 'e:/work', 'E:\\Work'])).toBe(true);
    expect(PathUtils.sameSelectedPaths([], ['/'])).toBe(false);
  });
});

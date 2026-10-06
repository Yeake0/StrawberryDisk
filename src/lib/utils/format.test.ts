import { describe, expect, it } from 'vitest';

import * as FormatUtils from './format';
import { BYTE_UNIT_BASES } from './format';

describe('FormatUtils.bytes', () => {
  it('formats the same raw bytes with an explicit decimal or binary base', () => {
    expect(FormatUtils.bytes(10_842_048, BYTE_UNIT_BASES.decimal)).toBe('10.8 MB');
    expect(FormatUtils.bytes(10_842_048, BYTE_UNIT_BASES.binary)).toBe('10.3 MB');
  });

  it('handles empty and invalid values without exposing invalid numbers', () => {
    expect(FormatUtils.bytes(0, BYTE_UNIT_BASES.decimal)).toBe('0 B');
    expect(FormatUtils.bytes(Number.NaN, BYTE_UNIT_BASES.binary)).toBe('0 B');
  });

  it('retains requested precision for a capacity summary', () => {
    expect(FormatUtils.bytes(241_040_000_000, BYTE_UNIT_BASES.decimal, 2)).toBe('241.04 GB');
    expect(FormatUtils.bytes(241_640_000_000, BYTE_UNIT_BASES.decimal, 2)).toBe('241.64 GB');
  });
});

describe('FormatUtils.dateTime', () => {
  it('uses the application locale instead of the host default', () => {
    const timestamp = new Date(2024, 2, 5, 15, 18).getTime();
    const chinese = FormatUtils.dateTime(timestamp, 'zh-CN');
    const english = FormatUtils.dateTime(timestamp, 'en-US');

    expect(chinese).toContain('2024');
    expect(chinese.indexOf('03')).toBeLessThan(chinese.indexOf('05'));
    expect(english.indexOf('03')).toBeLessThan(english.indexOf('05'));
    expect(chinese).not.toBe(english);
    expect(english).not.toContain(',');
    expect(english).toBe('03/05/2024 15:18');
  });

  it('returns the empty display text for invalid values', () => {
    expect(FormatUtils.dateTime(null, 'zh-CN')).toBe('—');
    expect(FormatUtils.dateTime(Number.NaN, 'en-US')).toBe('—');
  });
});

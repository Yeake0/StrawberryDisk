import { describe, expect, it } from 'vitest';

import { normalizeExternalUrl } from '@/lib/utils/external-url';

describe('normalizeExternalUrl', () => {
  it('accepts web and email links', () => {
    expect(normalizeExternalUrl('https://github.com/Yeake0/StrawberryDisk')).toBe(
      'https://github.com/Yeake0/StrawberryDisk'
    );
    expect(normalizeExternalUrl('http://example.com/releases')).toBe('http://example.com/releases');
    expect(normalizeExternalUrl('mailto:support@example.com')).toBe('mailto:support@example.com');
  });

  it('rejects executable and local-resource schemes', () => {
    expect(normalizeExternalUrl('javascript:alert(1)')).toBeNull();
    expect(normalizeExternalUrl('file:///private/example')).toBeNull();
    expect(normalizeExternalUrl('data:text/html,unsafe')).toBeNull();
  });

  it('rejects relative and malformed links', () => {
    expect(normalizeExternalUrl('/releases/latest')).toBeNull();
    expect(normalizeExternalUrl('not a url')).toBeNull();
    expect(normalizeExternalUrl('')).toBeNull();
  });
});

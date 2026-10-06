import { describe, expect, it } from 'vitest';
import { projectWebsiteUrl } from './project-website';

describe('project documentation links', () => {
  it.each(['zh-CN', 'ja-JP', 'pt-BR', 'unknown'])('maps %s to the maintained AI guide', locale => {
    expect(projectWebsiteUrl(locale)).toBe('https://github.com/Yeake0/StrawberryDisk');
    expect(projectWebsiteUrl(locale, '/docs/ai#custom-service')).toBe(
      'https://github.com/Yeake0/StrawberryDisk/blob/main/docs/ai.md'
    );
  });
});

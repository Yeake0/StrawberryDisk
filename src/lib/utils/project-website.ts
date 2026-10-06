import { PROJECT_LINKS } from '@/lib/models/application-shell';
/** Maps the former website's AI guide to a maintained repository document. */
export function projectWebsiteUrl(_language: string, path = ''): string {
  if (path.startsWith('/docs/ai')) return `${PROJECT_LINKS.repository}/blob/main/docs/ai.md`;
  return PROJECT_LINKS.website;
}

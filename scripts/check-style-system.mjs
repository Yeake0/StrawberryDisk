/**
 * Enforces StrawberryDisk's shared visual language in project-owned frontend code.
 * Components must use semantic theme tokens, keep literal colors in theme
 * files, and avoid hover transforms that make controls visually unstable.
 *
 * Run with `pnpm check:style-system`. The command exits with a non-zero status
 * and reports every source location that bypasses these style boundaries.
 */
import { readFileSync, readdirSync } from 'node:fs';
import { extname, join, relative, resolve } from 'node:path';
import { parse } from 'vue/compiler-sfc';

const projectRoot = resolve(import.meta.dirname, '..');
const sourceRoot = join(projectRoot, 'src');
const themePath = 'src/assets/main.css';
const themeDirectoryPrefix = 'src/assets/themes/';
const generatedUiPrefix = 'src/components/ui/';
const sourceExtensions = new Set(['.css', '.ts', '.vue']);
const fixedPaletteNames =
  'slate|gray|zinc|neutral|stone|red|orange|amber|yellow|lime|green|emerald|teal|cyan|sky|blue|indigo|violet|purple|fuchsia|pink|rose';
const checks = [
  {
    pattern: new RegExp(
      String.raw`\b(?:bg|text|border|ring|shadow|fill|stroke)-(?:${fixedPaletteNames})-\d{2,3}\b`,
      'gu'
    ),
    reason: 'use a semantic theme color instead of a fixed Tailwind palette color',
    skipTheme: false,
  },
  {
    pattern: /#[\da-f]{3,8}\b|(?:rgb|hsl|oklch|oklab|lab|lch)a?\s*\(/giu,
    reason: 'define literal colors in a global theme token file and consume a semantic token',
    skipTheme: true,
  },
  {
    pattern: /\b(?:hover|active):(?:-?translate-[^\s"'`]+|-?scale-[^\s"'`]+)/gu,
    reason: 'interactive states must not translate or scale controls',
    skipTheme: false,
  },
];

const violations = [];
for (const path of collectFiles(sourceRoot)) {
  const displayPath = relative(projectRoot, path).replaceAll('\\', '/');
  if (displayPath.startsWith(generatedUiPrefix)) continue;

  const content = readFileSync(path, 'utf8');
  if (extname(path) === '.vue') {
    // Inspect native elements only: component title props still name visible headings.
    const visit = node => {
      if (node.type === 1 && node.tagType === 0) {
        const title = node.props.find(
          prop => prop.name === 'title' || (prop.name === 'bind' && prop.arg?.content === 'title')
        );
        if (title) {
          violations.push(`${displayPath}:${title.loc.start.line}: use Shadcn-Vue Tooltip instead of native title`);
        }
      }
      for (const child of node.children ?? []) visit(child);
    };
    const template = parse(content).descriptor.template;
    if (template?.ast) visit(template.ast);
  }
  for (const check of checks) {
    if (check.skipTheme && (displayPath === themePath || displayPath.startsWith(themeDirectoryPrefix))) {
      continue;
    }
    for (const match of content.matchAll(check.pattern)) {
      const line = content.slice(0, match.index).split(/\r?\n/u).length;
      violations.push(`${displayPath}:${line}: ${check.reason}`);
    }
  }
}

if (violations.length > 0) {
  console.error('Style system validation failed:');
  for (const violation of violations) console.error(`- ${violation}`);
  process.exitCode = 1;
} else {
  console.log('Style system validation passed');
}

function collectFiles(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap(entry => {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) return collectFiles(path);
    return entry.isFile() && sourceExtensions.has(extname(entry.name)) ? [path] : [];
  });
}

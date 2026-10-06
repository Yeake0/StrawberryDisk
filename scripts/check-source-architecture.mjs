import { readdir, readFile } from 'node:fs/promises';
import path from 'node:path';
import process from 'node:process';

const repositoryRoot = process.cwd();
const sourceRoot = path.join(repositoryRoot, 'src');
const tauriSourceRoot = path.join(repositoryRoot, 'src-tauri');
const violations = [];

async function collectFiles(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const entryPath = path.join(directory, entry.name);
    if (entry.isDirectory()) files.push(...(await collectFiles(entryPath)));
    else files.push(entryPath);
  }
  return files;
}

function relativePath(filePath) {
  return path.relative(repositoryRoot, filePath).split(path.sep).join('/');
}

function sourceLayer(relativeFilePath) {
  if (relativeFilePath.startsWith('src/lib/utils/')) return 'utils';
  if (relativeFilePath.startsWith('src/lib/models/')) return 'models';
  if (relativeFilePath.startsWith('src/lib/services/')) return 'services';
  if (relativeFilePath.startsWith('src/stores/')) return 'stores';
  if (relativeFilePath.startsWith('src/components/')) return 'shared-components';
  if (relativeFilePath.startsWith('src/layouts/')) return 'layouts';
  if (relativeFilePath.startsWith('src/pages/')) return 'pages';
  return 'root';
}

function importedSpecifiers(source) {
  const specifiers = new Set();
  const staticImports = /(?:import|export)\s+(?:type\s+)?(?:[^;]*?\sfrom\s+)?['"]([^'"]+)['"]/g;
  const dynamicImports = /import\(\s*['"]([^'"]+)['"]\s*\)/g;
  for (const expression of [staticImports, dynamicImports]) {
    for (const match of source.matchAll(expression)) specifiers.add(match[1]);
  }
  return [...specifiers];
}

function resolveFrontendImport(relativeFilePath, specifier, sourceFilePaths) {
  let candidate = null;
  if (specifier.startsWith('@/')) candidate = `src/${specifier.slice(2)}`;
  if (specifier.startsWith('.')) {
    candidate = path.posix.normalize(path.posix.join(path.posix.dirname(relativeFilePath), specifier));
  }
  if (!candidate) return null;
  const cleanCandidate = candidate.split('?', 1)[0];
  const resolvedCandidates = [
    cleanCandidate,
    `${cleanCandidate}.ts`,
    `${cleanCandidate}.vue`,
    `${cleanCandidate}/index.ts`,
    `${cleanCandidate}/index.vue`,
  ];
  return resolvedCandidates.find(resolved => sourceFilePaths.has(resolved)) ?? null;
}

const forbiddenLayerImports = {
  utils: ['services', 'stores', 'pages', 'layouts', 'shared-components'],
  models: ['services', 'utils', 'stores', 'pages', 'layouts', 'shared-components'],
  services: ['stores', 'pages', 'layouts', 'shared-components'],
  stores: ['pages', 'layouts', 'shared-components'],
  'shared-components': ['stores', 'pages', 'layouts'],
  // The application shell is the explicit composition root for route pages.
  // Lower layers remain unable to reach either pages or layouts.
  layouts: [],
  pages: [],
  root: [],
};

function checkImportBoundaries(relativeFilePath, source) {
  const layer = sourceLayer(relativeFilePath);
  for (const specifier of importedSpecifiers(source)) {
    let importedFilePath = null;
    if (specifier.startsWith('@/')) importedFilePath = `src/${specifier.slice(2)}`;
    if (specifier.startsWith('.')) {
      importedFilePath = path.posix.normalize(path.posix.join(path.posix.dirname(relativeFilePath), specifier));
    }
    if (importedFilePath && forbiddenLayerImports[layer].includes(sourceLayer(importedFilePath))) {
      violations.push(`${relativeFilePath}: ${layer} must not import ${specifier}`);
    }
    if (specifier.startsWith('@tauri-apps/') && layer !== 'services') {
      violations.push(`${relativeFilePath}: Tauri APIs belong in src/lib/services, found ${specifier}`);
    }
  }
}

function checkUtilityApi(relativeFilePath, source) {
  if (!relativeFilePath.startsWith('src/lib/utils/') || !relativeFilePath.endsWith('.ts')) return;
  if (/\.(?:test|spec)\.ts$/u.test(relativeFilePath)) return;
  if (/export\s+(?:default\s+)?class\s+[A-Za-z0-9]+Utils\b/u.test(source)) {
    violations.push(`${relativeFilePath}: pure utility modules must export functions instead of classes`);
  }
  if (/export\s+const\s+[A-Za-z0-9]+Utils\s*=/u.test(source)) {
    violations.push(`${relativeFilePath}: pure utility modules must not export Utils namespace objects`);
  }
}

function checkFrontendName(relativeFilePath) {
  if (relativeFilePath.startsWith('src/components/ui/')) return;
  const extension = path.extname(relativeFilePath);
  if (!['.vue', '.ts', '.css'].includes(extension)) return;
  const basename = path.basename(relativeFilePath, extension);
  if (extension === '.vue' && ['App', 'index'].includes(basename)) return;
  if (extension === '.ts' && ['index', 'i18n.d'].includes(basename)) return;
  const logicalName = basename.replace(/\.(?:test|spec|d)$/, '');
  if (!/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(logicalName)) {
    violations.push(`${relativeFilePath}: project-owned frontend files must use kebab-case`);
  }
  if (
    extension === '.vue' &&
    (relativeFilePath.startsWith('src/components/custom/') || relativeFilePath.startsWith('src/components/icons/')) &&
    !basename.startsWith('md-')
  ) {
    violations.push(`${relativeFilePath}: reusable project-owned components must use the md- prefix`);
  }
}

function checkBusinessBarrel(relativeFilePath) {
  if (!relativeFilePath.endsWith('/index.ts')) return;
  if (relativeFilePath.startsWith('src/components/ui/')) return;
  if (relativeFilePath === 'src/lib/utils/index.ts') return;
  violations.push(`${relativeFilePath}: business modules must be imported from concrete files`);
}

function checkImportCycles(frontendSources) {
  const sourceFilePaths = new Set(frontendSources.keys());
  const dependencies = new Map();
  for (const [relativeFilePath, source] of frontendSources) {
    const resolvedImports = importedSpecifiers(source)
      .map(specifier => resolveFrontendImport(relativeFilePath, specifier, sourceFilePaths))
      .filter(Boolean);
    dependencies.set(relativeFilePath, [...new Set(resolvedImports)]);
  }

  const visitState = new Map();
  const stack = [];
  const reportedCycles = new Set();
  function visit(relativeFilePath) {
    visitState.set(relativeFilePath, 'visiting');
    stack.push(relativeFilePath);
    for (const dependency of dependencies.get(relativeFilePath) ?? []) {
      if (visitState.get(dependency) === 'visiting') {
        const cycleStart = stack.indexOf(dependency);
        const cycle = [...stack.slice(cycleStart), dependency];
        const signature = [...new Set(cycle)].sort().join('|');
        if (!reportedCycles.has(signature)) {
          reportedCycles.add(signature);
          violations.push(`circular frontend dependency: ${cycle.join(' -> ')}`);
        }
      } else if (!visitState.has(dependency)) {
        visit(dependency);
      }
    }
    stack.pop();
    visitState.set(relativeFilePath, 'visited');
  }

  for (const relativeFilePath of frontendSources.keys()) {
    if (!visitState.has(relativeFilePath)) visit(relativeFilePath);
  }
}

function checkRustName(relativeFilePath) {
  if (!relativeFilePath.endsWith('.rs')) return;
  const basename = path.basename(relativeFilePath, '.rs');
  if (!/^[a-z][a-z0-9]*(?:_[a-z0-9]+)*$/.test(basename)) {
    violations.push(`${relativeFilePath}: Rust source files must use snake_case`);
  }
}

const frontendFiles = await collectFiles(sourceRoot);
const frontendSources = new Map();
for (const filePath of frontendFiles) {
  const relativeFilePath = relativePath(filePath);
  checkFrontendName(relativeFilePath);
  checkBusinessBarrel(relativeFilePath);
  if (!/\.(?:ts|vue)$/.test(relativeFilePath) || relativeFilePath.startsWith('src/components/ui/')) continue;
  const source = await readFile(filePath, 'utf8');
  frontendSources.set(relativeFilePath, source);
  checkImportBoundaries(relativeFilePath, source);
  checkUtilityApi(relativeFilePath, source);
}
checkImportCycles(frontendSources);

for (const filePath of await collectFiles(tauriSourceRoot)) {
  checkRustName(relativePath(filePath));
}

if (violations.length) {
  console.error(`Source architecture validation failed with ${violations.length} violation(s):`);
  for (const violation of violations) console.error(`- ${violation}`);
  process.exitCode = 1;
} else {
  console.log('Source architecture validation passed.');
}

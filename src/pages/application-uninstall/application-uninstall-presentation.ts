import type {
  ApplicationUninstallCandidate,
  ApplicationUninstallDiagnostic,
  ApplicationUninstallInstallerKind,
} from '@/lib/models/application';

export type ApplicationSizeHintKey =
  'applicationUninstall.windowsAppPackageSizeHint' | 'applicationUninstall.applicationSizeEstimateHint';

/**
 * Windows AppX inventory can include shared package files whose logical size is not attributable
 * to one application. Its dedicated hint explains that limitation; other installer kinds retain
 * the general measured-or-estimated size explanation.
 */
export function applicationSizeHintKey(
  installerKind: ApplicationUninstallInstallerKind | null
): ApplicationSizeHintKey {
  return installerKind === 'windowsAppx'
    ? 'applicationUninstall.windowsAppPackageSizeHint'
    : 'applicationUninstall.applicationSizeEstimateHint';
}

/** AppX uses a certificate subject when the manifest has no resolved publisher display name.
 * Show its common name in the catalog while retaining the original value in inventory data.
 */
export function applicationPublisherLabel(
  candidate: Pick<ApplicationUninstallCandidate, 'installerKind' | 'publisher' | 'primaryIdentifier'>
): string {
  const publisher = candidate.publisher || candidate.primaryIdentifier;
  if (candidate.installerKind !== 'windowsAppx') return publisher;

  // Windows quotes values containing commas and doubles embedded quotation marks.
  // Read every attribute in order so a quoted value cannot masquerade as a CN field.
  const attribute = /([a-z][a-z0-9.]*)=("(?:[^"]|"")*"|(?:\\[,\\]|[^,"\\=+<>#;])+)(?:,\s*(?=\S)|$)/iy;
  let commonName: string | undefined;
  while (attribute.lastIndex < publisher.length) {
    const match = attribute.exec(publisher);
    // Keep malformed or unsupported subjects intact instead of displaying a partial name.
    if (!match) return publisher;
    if (match[1].toUpperCase() !== 'CN' || commonName !== undefined) continue;
    const value = match[2].trim();
    commonName = value.startsWith('"') ? value.slice(1, -1).replaceAll('""', '"') : value.replace(/\\([,\\])/g, '$1');
  }
  return commonName || publisher;
}

/** Keep the explanation tied to observed evidence; a Settings button does not prove that
 * Windows can launch the registered uninstaller. Unknown failures retain the neutral fallback.
 */
export function applicationUnavailableTitleKey(diagnostic: ApplicationUninstallDiagnostic | null): string {
  switch (diagnostic) {
    case 'executableMissing':
      return 'applicationUninstall.uninstallerMissing';
    case 'executableAccessDenied':
      return 'applicationUninstall.uninstallerAccessDenied';
    default:
      return 'applicationUninstall.uninstallEntryUnavailableDescription';
  }
}

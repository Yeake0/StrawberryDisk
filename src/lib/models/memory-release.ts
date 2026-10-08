export interface ExcludedApplication {
  name: string;
  path: string;
}
export interface MemoryReleasePreferences {
  schemaVersion: 1;
  revision: number;
  automatic: boolean;
  intervalMinutes: number;
  thresholdPercent: number;
  skipForeground: boolean;
  exclusions: ExcludedApplication[];
}

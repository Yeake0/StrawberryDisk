import type { ApplicationCpu, ApplicationMemory } from './system-resources';

export type ResourceMetric = 'cpu' | 'memory';
export type ResourceApplication = ApplicationCpu | ApplicationMemory;
export interface ResourceSort {
  column: 'name' | 'usage';
  direction: 'ascending' | 'descending';
}
export interface ResourceSortPreferences {
  schemaVersion: 1;
  cpu: ResourceSort;
  memory: ResourceSort;
}
export interface ResourceListRow {
  application: ResourceApplication;
  available: boolean;
  members: { pid: number; startedAt: number; usedPercent: number | null }[];
}

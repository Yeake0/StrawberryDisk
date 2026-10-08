import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import type {
  SystemMaintenanceCatalog,
  SystemMaintenanceExecutionRequest,
  SystemMaintenanceJob,
  SystemMaintenanceRuntimeState,
} from '@/lib/models/system-maintenance';
import { EVENT_NAMES } from '@/lib/models/telemetry';

export class SystemMaintenanceService {
  static scan(): Promise<SystemMaintenanceCatalog> {
    return invoke<SystemMaintenanceCatalog>('scan_system_maintenance');
  }

  static cancelScan(): Promise<void> {
    return invoke<void>('cancel_system_maintenance_scan');
  }

  static execute(request: SystemMaintenanceExecutionRequest): Promise<SystemMaintenanceJob> {
    return invoke<SystemMaintenanceJob>('execute_system_maintenance', { request });
  }

  static cancelExecution(executionId: string): Promise<SystemMaintenanceJob> {
    return invoke<SystemMaintenanceJob>('cancel_system_maintenance_execution', { executionId });
  }

  static runtime(): Promise<SystemMaintenanceRuntimeState> {
    return invoke<SystemMaintenanceRuntimeState>('get_system_maintenance_runtime');
  }

  static listenJobUpdates(handler: (job: SystemMaintenanceJob) => void): Promise<UnlistenFn> {
    return listen<SystemMaintenanceJob>(EVENT_NAMES.systemMaintenanceJobUpdated, event => handler(event.payload));
  }
}

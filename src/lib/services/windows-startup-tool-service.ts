import { invoke } from '@tauri-apps/api/core';
import { parseCommandError } from '@/lib/utils/error';

export type WindowsStartupTool = 'services' | 'taskScheduler';

export const WindowsStartupToolService = {
  async open(tool: WindowsStartupTool): Promise<void> {
    try {
      await invoke<void>('open_windows_startup_tool', { tool });
    } catch (error) {
      // Declining UAC is an intentional cancellation, not a failed operation.
      if (parseCommandError(error)?.code !== 'operationCancelled') throw error;
    }
  },
};

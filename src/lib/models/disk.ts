export interface DiskInfo {
  name: string;
  mountPoint: string;
  totalBytes: number;
  /** Display availability includes system-reclaimable storage on macOS. */
  availableBytes: number;
  usedBytes: number;
}

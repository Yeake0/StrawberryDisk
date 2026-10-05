export type GpuActivityKind =
  'graphics' | 'copy' | 'videoDecode' | 'videoEncode' | 'videoProcessing' | 'other' | 'renderer' | 'tiler';
export interface GpuActivity {
  id: string;
  kind: GpuActivityKind;
  name: string | null;
  usedPercent: number;
  includedInSummary: boolean;
}
export interface GpuDetails {
  activities: GpuActivity[];
  telemetry: {
    coreCount: number | null;
    temperatureCelsius: number | null;
    coreClockMhz: number | null;
    memoryClockMhz: number | null;
    fanPercent: number | null;
  };
  memoryArchitecture: 'unified' | 'dedicated' | 'shared' | 'unknown';
  memoryStatus: 'ready' | 'unsupported' | 'failed';
  memory: {
    dedicatedUsedBytes: number | null;
    dedicatedTotalBytes: number | null;
    sharedUsedBytes: number | null;
  } | null;
}
export const GPU_ACTIVITY_LABEL_KEYS: Record<GpuActivityKind, string> = {
  graphics: 'gpuDetails.graphics',
  copy: 'gpuDetails.copy',
  videoDecode: 'gpuDetails.videoDecode',
  videoEncode: 'gpuDetails.videoEncode',
  videoProcessing: 'gpuDetails.videoProcessing',
  other: 'gpuDetails.other',
  renderer: 'gpuDetails.renderer',
  tiler: 'gpuDetails.tiler',
};

export const GPU_FACT_LABEL_KEYS = {
  coreCount: 'gpuDetails.coreCount',
  average: 'gpuDetails.average',
  peak: 'gpuDetails.peak',
  temperature: 'gpuDetails.temperature',
  coreClock: 'gpuDetails.coreClock',
  memoryClock: 'gpuDetails.memoryClock',
  fanSpeed: 'gpuDetails.fanSpeed',
} as const;

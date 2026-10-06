import { BYTE_SIZE_UNITS, type ByteSizePreset, type ByteSizeUnit } from '@/lib/models/byte-size';
import type { ByteUnitBase } from '@/lib/utils/format';
const BYTE_SIZE_UNIT_EXPONENTS: Readonly<Record<ByteSizeUnit, number>> = {
  [BYTE_SIZE_UNITS.kilobytes]: 1,
  [BYTE_SIZE_UNITS.megabytes]: 2,
  [BYTE_SIZE_UNITS.gigabytes]: 3,
};
export function bytes(preset: ByteSizePreset, unitBase: ByteUnitBase): number {
  return preset.amount * unitBase ** BYTE_SIZE_UNIT_EXPONENTS[preset.unit];
}
export function byteValues(presets: readonly ByteSizePreset[], unitBase: ByteUnitBase): number[] {
  return presets.map(preset => bytes(preset, unitBase));
}

import type { VectorBits } from '$lib/protocol/generated/VectorBits';

export const laneFormats = [
  'hex',
  'u8',
  'i8',
  'u16',
  'i16',
  'u32',
  'i32',
  'u64',
  'i64',
  'f32',
  'f64',
] as const;
export type LaneFormat = (typeof laneFormats)[number];

/**
 * Interpret a raw 128-bit value, with lane zero first (least-significant bits).
 * Integer lanes remain exact; floating-point views retain signed zero. NaN payloads
 * remain available in the raw value, not in JavaScript's numeric interpretation.
 *
 * @throws RangeError - The value is not a canonical 128-bit hexadecimal string.
 */
export function vectorLanes(value: VectorBits, format: LaneFormat): string[] {
  if (!/^0x[\da-f]{32}$/.test(value)) throw new RangeError('Invalid vector bits');
  if (format === 'hex') return [value.slice(2)];
  const width = Number(format.slice(1));
  if (format === 'f32' || format === 'f64') {
    const data = new DataView(Uint8Array.fromHex(value.slice(2)).buffer);
    return Array.from({ length: 128 / width }, (_, index) => {
      // Wire hex is most-significant first; lane zero starts at its low end.
      const offset = data.byteLength - ((index + 1) * width) / 8;
      const number = format === 'f32' ? data.getFloat32(offset) : data.getFloat64(offset);
      return Object.is(number, -0) ? '-0' : String(number);
    });
  }
  const bits = BigInt(value);
  return Array.from({ length: 128 / width }, (_, index) => {
    const lane = bits >> BigInt(index * width);
    return (
      format.startsWith('i') ? BigInt.asIntN(width, lane) : BigInt.asUintN(width, lane)
    ).toString();
  });
}

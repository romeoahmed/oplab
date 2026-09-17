import type { Registers } from '$lib/protocol/generated/Registers';
import type { RoundingMode } from '$lib/protocol/generated/RoundingMode';
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
  'f16',
  'f32',
  'f64',
] as const;
export type LaneFormat = (typeof laneFormats)[number] | 'bit';
export type VectorView = 'full' | 'low' | 'predicates';
export interface VectorEntry {
  name: string;
  value: VectorBits;
}

/** Project active SVE storage or low XMM/V aliases without changing the complete bank. */
export function vectorEntries(bank: Registers, view: VectorView): VectorEntry[] {
  if (bank.type === 'x86_64') {
    return bank.data.ymm.map((value, index) => ({
      name: `${view === 'low' ? 'xmm' : 'ymm'}${String(index)}`,
      value: view === 'low' ? `0x${value.slice(-32)}` : value,
    }));
  }
  if (view === 'predicates') {
    return [...bank.data.p, bank.data.ffr].map((value, index) => ({
      name: index === 16 ? 'ffr' : `p${String(index)}`,
      value: `0x${value.slice(-(bank.data.vl / 4))}`,
    }));
  }
  return bank.data.z.map((value, index) => ({
    name: `${view === 'low' ? 'v' : 'z'}${String(index)}`,
    value: `0x${value.slice(-(view === 'low' ? 32 : bank.data.vl * 2))}`,
  }));
}

/** Width in bits of an already validated wire value, including leading zero bytes. */
export function vectorWidth(value: VectorBits): number {
  return (value.length - 2) * 4;
}

/**
 * Interpret a raw vector value, with lane zero first (least-significant bits).
 *
 * @remarks
 * Integer lanes remain exact; floating-point views retain signed zero. NaN payloads
 * remain available in the raw value, not in JavaScript's numeric interpretation.
 *
 * @throws RangeError - The value is not a canonical, bounded hexadecimal byte string.
 */
export function vectorLanes(value: VectorBits, format: LaneFormat): string[] {
  if (value.trim() !== value || !/^0x(?:[\da-f]{2}){1,256}$/.test(value))
    throw new RangeError('Invalid vector bits');
  if (format === 'hex') return [value.slice(2)];
  const width = laneWidth(format, vectorWidth(value));
  if (format.startsWith('f')) {
    const data = new DataView(Uint8Array.fromHex(value.slice(2)).buffer);
    return Array.from({ length: vectorWidth(value) / width }, (_, index) => {
      // Wire hex is most-significant first; lane zero starts at its low end.
      const offset = data.byteLength - ((index + 1) * width) / 8;
      const number =
        format === 'f16'
          ? data.getFloat16(offset)
          : format === 'f32'
            ? data.getFloat32(offset)
            : data.getFloat64(offset);
      return Object.is(number, -0) ? '-0' : String(number);
    });
  }
  const bits = BigInt(value);
  return Array.from({ length: vectorWidth(value) / width }, (_, index) => {
    const lane = bits >> BigInt(index * width);
    return (
      format.startsWith('i') ? BigInt.asIntN(width, lane) : BigInt.asUintN(width, lane)
    ).toString();
  });
}

/** Width of a displayed lane; raw hexadecimal edits replace the complete register. */
export function laneWidth(format: LaneFormat, bits: number): number {
  return format === 'hex' ? bits : format === 'bit' ? 1 : Number(format.slice(1));
}

/**
 * Encode a typed lane or complete register as unshifted wire bits.
 *
 * @remarks
 * Integers use decimal; complete registers use hex. Floats accept decimal, scientific
 * notation, signed zero, Infinity and NaN. Conversion uses DataView rounding, independent
 * of guest rounding; raw hex preserves exact NaN payloads.
 *
 * @returns `null` for malformed, oversized or out-of-range input, including finite
 * float overflow. Subnormal rounding and underflow follow IEEE-754 conversion.
 */
export function vectorInput(
  text: string,
  format: LaneFormat,
  registerBits: number,
): VectorBits | null {
  const value = text.trim();
  if (value.length === 0 || value.length > 514) return null;
  const width = laneWidth(format, registerBits);
  let bits: bigint;
  if (format === 'hex') {
    if (!/^(?:0x)?[\da-f]+$/i.test(value) || value.replace(/^0x/i, '').length > width / 4)
      return null;
    bits = BigInt(`0x${value.replace(/^0x/i, '')}`);
  } else if (format.startsWith('f')) {
    if (!/^(?:[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:e[+-]?\d+)?|[+-]?Infinity|NaN)$/i.test(value))
      return null;
    // Keep special values case-insensitive without broadening numeric syntax.
    const number = /^[-+]?infinity$/i.test(value)
      ? value.startsWith('-')
        ? -Infinity
        : Infinity
      : /^nan$/i.test(value)
        ? NaN
        : Number(value);
    if (
      !/infinity|nan/i.test(value) &&
      !Number.isFinite(
        format === 'f16' ? Math.f16round(number) : format === 'f32' ? Math.fround(number) : number,
      )
    )
      return null;
    const data = new DataView(new ArrayBuffer(width / 8));
    if (format === 'f16') data.setFloat16(0, number);
    else if (format === 'f32') data.setFloat32(0, number);
    else data.setFloat64(0, number);
    bits =
      format === 'f16'
        ? BigInt(data.getUint16(0))
        : format === 'f32'
          ? BigInt(data.getUint32(0))
          : data.getBigUint64(0);
  } else {
    if (!/^[+-]?\d+$/.test(value)) return null;
    bits = BigInt(value);
    const signed = format.startsWith('i');
    const bound = 1n << BigInt(signed ? width - 1 : width);
    if (bits < (signed ? -bound : 0n) || bits >= bound) return null;
    bits = BigInt.asUintN(width, bits);
  }
  return `0x${bits.toString(16).padStart(Math.ceil(width / 8) * 2, '0')}`;
}

/** Decode architectural rounding fields; x86 and A64 swap the directed encodings. */
export function roundingMode(bank: Registers): RoundingMode {
  if (bank.type === 'x86_64') {
    switch ((bank.data.mxcsr >>> 13) & 3) {
      case 1:
        return 'down';
      case 2:
        return 'up';
      case 3:
        return 'toward_zero';
      default:
        return 'nearest_even';
    }
  }
  switch ((bank.data.fpcr >>> 22) & 3) {
    case 1:
      return 'up';
    case 2:
      return 'down';
    case 3:
      return 'toward_zero';
    default:
      return 'nearest_even';
  }
}

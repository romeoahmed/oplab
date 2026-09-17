import type { Counter } from './generated/Counter';
import type { HexAddress } from './generated/HexAddress';

const maximum = (1n << 64n) - 1n;

/**
 * Normalize hexadecimal user input, with optional prefix and surrounding whitespace.
 *
 * @throws RangeError - The input is empty, nonhexadecimal or outside the unsigned 64-bit range.
 */
export function normalizeAddress(value: string): HexAddress {
  const digits = value.trim().replace(/^0x/i, '');
  if (!/^[\da-f]+$/i.test(digits)) throw new RangeError('Invalid address');
  return formatAddress(BigInt(`0x${digits}`));
}

/**
 * Parse `0x` followed by exactly sixteen lowercase hexadecimal digits.
 *
 * @throws RangeError - The address is not in canonical wire form.
 */
export function parseAddress(value: HexAddress): bigint {
  if (!/^0x[0-9a-f]{16}$/.test(value)) throw new RangeError('Invalid wire address');
  return BigInt(value);
}

/**
 * Format an unsigned 64-bit address in canonical wire form without wrapping.
 *
 * @throws RangeError - The value is outside the unsigned 64-bit range.
 */
export function formatAddress(value: bigint): HexAddress {
  if (value < 0n || value > maximum) throw new RangeError('Address exceeds 64 bits');
  return `0x${value.toString(16).padStart(16, '0')}`;
}

/**
 * Parse an unsigned 64-bit decimal string without leading zeros, except `0`.
 *
 * @throws RangeError - The counter is noncanonical or exceeds 64 bits.
 */
export function parseCounter(value: Counter): bigint {
  if (!/^(0|[1-9][0-9]{0,19})$/.test(value)) throw new RangeError('Invalid wire counter');
  const result = BigInt(value);
  if (result > maximum) throw new RangeError('Counter exceeds 64 bits');
  return result;
}

/**
 * Format an unsigned 64-bit counter as decimal text for JSON transport.
 *
 * @throws RangeError - The value is outside the unsigned 64-bit range.
 */
export function formatCounter(value: bigint): Counter {
  if (value < 0n || value > maximum) throw new RangeError('Counter exceeds 64 bits');
  return value.toString();
}

/**
 * Parse unsigned decimal or 0x-prefixed input, allowing surrounding whitespace.
 *
 * @remarks
 * The result is exact but not width-limited; callers enforce their own numeric bounds.
 *
 * @throws RangeError - The trimmed input is empty or contains invalid digits.
 */
export function parseUnsigned(value: string): bigint {
  const text = value.trim();
  if (!/^(?:[0-9]+|0x[\da-f]+)$/i.test(text)) throw new RangeError('Invalid unsigned integer');
  return BigInt(text);
}

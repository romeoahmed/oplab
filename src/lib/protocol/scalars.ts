import type { Counter } from './generated/Counter';
import type { HexAddress } from './generated/HexAddress';
import type { BuildIdentity } from './generated/BuildIdentity';

const maximum = (1n << 64n) - 1n;

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

/** Compare document, revision and all assembly settings before accepting a build. */
export function sameBuildIdentity(left: BuildIdentity, right: BuildIdentity): boolean {
  return (
    left.document === right.document &&
    left.revision === right.revision &&
    left.target === right.target &&
    left.base === right.base &&
    left.assembler.name === right.assembler.name &&
    left.assembler.version === right.assembler.version
  );
}

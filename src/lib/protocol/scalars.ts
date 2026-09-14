import type { Counter } from './generated/Counter';
import type { HexAddress } from './generated/HexAddress';
import type { BuildIdentity } from './generated/BuildIdentity';

const maximum = (1n << 64n) - 1n;

/** Decode canonical wire text; never route a guest address through Number. */
export function parseAddress(value: HexAddress): bigint {
  if (!/^0x[0-9a-f]{16}$/.test(value)) throw new RangeError('Invalid wire address');
  return BigInt(value);
}

/** Encode exactly 64 bits. Arithmetic wraparound requires an explicit guest operation. */
export function formatAddress(value: bigint): HexAddress {
  if (value < 0n || value > maximum) throw new RangeError('Address exceeds 64 bits');
  return `0x${value.toString(16).padStart(16, '0')}`;
}

/** Decode a precision-sensitive protocol counter with the same canonical syntax as Rust. */
export function parseCounter(value: Counter): bigint {
  if (!/^(0|[1-9][0-9]{0,19})$/.test(value)) throw new RangeError('Invalid wire counter');
  const result = BigInt(value);
  if (result > maximum) throw new RangeError('Counter exceeds 64 bits');
  return result;
}

/** Encode an exact nonnegative counter for JSON transport. */
export function formatCounter(value: bigint): Counter {
  if (value < 0n || value > maximum) throw new RangeError('Counter exceeds 64 bits');
  return value.toString();
}

/** A stale document, revision, target, origin, or backend must never replace a newer build. */
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

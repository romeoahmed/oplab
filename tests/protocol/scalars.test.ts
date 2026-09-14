import { expect, test } from 'vitest';
import fc from 'fast-check';
import {
  formatAddress,
  formatCounter,
  parseAddress,
  parseCounter,
  sameBuildIdentity,
} from '$lib/protocol/scalars';
import type { BuildIdentity } from '$lib/protocol/generated/BuildIdentity';

test('Rust-compatible scalar syntax round-trips every generated 64-bit value', () => {
  fc.assert(
    fc.property(fc.bigInt({ min: 0n, max: (1n << 64n) - 1n }), (value) => {
      expect(parseAddress(formatAddress(value))).toBe(value);
      expect(parseCounter(formatCounter(value))).toBe(value);
    }),
  );
  for (const invalid of ['01', '-1', '+1', '1.0', '18446744073709551616'])
    expect(() => parseCounter(invalid)).toThrow(RangeError);
  expect(() => formatAddress(1n << 64n)).toThrow(RangeError);
  expect(() => parseAddress('0x00000000000000FF')).toThrow(RangeError);
});

test('complete build identity rejects cross-document and stale settings', () => {
  const identity: BuildIdentity = {
    document: 'one',
    revision: '1',
    target: 'x86_64',
    base: '0x0000000000001000',
    assembler: { name: 'llvm-mc', version: '23.1.1' },
  };
  expect(sameBuildIdentity(identity, structuredClone(identity))).toBe(true);
  const different: BuildIdentity[] = [
    { ...identity, document: 'two' },
    { ...identity, revision: '2' },
    { ...identity, target: 'aarch64' },
    { ...identity, base: '0x0000000000002000' },
    { ...identity, assembler: { ...identity.assembler, version: '23.1.2' } },
    { ...identity, assembler: { ...identity.assembler, name: 'other' } },
  ];
  for (const candidate of different) expect(sameBuildIdentity(identity, candidate)).toBe(false);
});

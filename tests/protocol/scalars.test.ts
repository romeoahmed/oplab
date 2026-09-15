import { expect, test } from 'vitest';
import fc from 'fast-check';
import {
  formatAddress,
  formatCounter,
  parseAddress,
  parseCounter,
  normalizeAddress,
  sameBuildIdentity,
} from '$lib/protocol/scalars';
import type { BuildIdentity } from '$lib/protocol/generated/BuildIdentity';

test('wire scalars match canonical decimal and hexadecimal without losing bits', () => {
  fc.assert(
    fc.property(fc.bigInt({ min: 0n, max: (1n << 64n) - 1n }), (value) => {
      const hex = `0x${value.toString(16).padStart(16, '0')}`;
      const decimal = value.toString(10);
      expect(formatAddress(value)).toBe(hex);
      expect(formatCounter(value)).toBe(decimal);
      expect(parseAddress(hex)).toBe(value);
      expect(parseCounter(decimal)).toBe(value);
      for (const input of [value.toString(16), ` 0X${value.toString(16).toUpperCase()} `])
        expect(normalizeAddress(input)).toBe(hex);
    }),
  );
  for (const invalid of ['01', '-1', '+1', '1.0', '18446744073709551616'])
    expect(() => parseCounter(invalid)).toThrow(RangeError);
  for (const invalid of [-1n, 1n << 64n]) {
    expect(() => formatAddress(invalid)).toThrow(RangeError);
    expect(() => formatCounter(invalid)).toThrow(RangeError);
  }
  expect(() => parseAddress('0x00000000000000FF')).toThrow(RangeError);
  for (const invalid of ['', ' ', '0x', '+1', '-1', '1.5', '0b10z', '1 2', '10000000000000000'])
    expect(() => normalizeAddress(invalid)).toThrow(RangeError);
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

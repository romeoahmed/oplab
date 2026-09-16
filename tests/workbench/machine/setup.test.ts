import { initialState } from '$lib/workbench/machine/setup';
import fc from 'fast-check';
import { expect, test } from 'vitest';

const u64 = fc.bigInt({ min: 0n, max: (1n << 64n) - 1n });

test('initial values preserve all 64 bits in decimal and hexadecimal input', () => {
  fc.assert(
    fc.property(
      u64,
      u64,
      fc.integer({ min: 1, max: 67108864 }),
      fc.integer({ min: 0, max: 7 }),
      (value, address, length, flags) => {
        for (const radix of [10, 16]) {
          const prefix = radix === 16 ? '0x' : '';
          const state = initialState({
            cpu: null,
            registers: [{ name: 'rax', value: prefix + value.toString(radix) }],
            mappings: [
              { address: address.toString(16), length: prefix + length.toString(radix), flags },
            ],
          });
          expect(state.registers).toEqual([{ name: 'rax', value: value.toString() }]);
          expect(state.mappings).toEqual([
            { address: `0x${address.toString(16).padStart(16, '0')}`, length, flags },
          ]);
        }
      },
    ),
    {
      examples: [
        [0n, 0n, 1, 0],
        [(1n << 64n) - 1n, (1n << 64n) - 1n, 67108864, 7],
      ],
    },
  );
});

test('incomplete and overflowing setup values never reach the wire', () => {
  for (const value of ['', '-1', '+1', '1.5', '1e3', '0b10', '0x', '0x10000000000000000']) {
    expect(() =>
      initialState({ cpu: null, registers: [{ name: 'rax', value }], mappings: [] }),
    ).toThrow(RangeError);
  }
  for (const [length, flags] of [
    ['0', 6],
    ['67108865', 6],
    ['4096', 8],
    ['4096', 0.5],
  ] as const) {
    expect(() =>
      initialState({ cpu: null, registers: [], mappings: [{ address: '0x8000', length, flags }] }),
    ).toThrow(RangeError);
  }
  expect(() =>
    initialState({
      cpu: null,
      registers: Array.from({ length: 33 }, () => ({ name: 'rax', value: '0' })),
      mappings: [],
    }),
  ).toThrow(RangeError);
  expect(() =>
    initialState({
      cpu: null,
      registers: [],
      mappings: Array.from({ length: 64 }, () => ({ address: '0x8000', length: '4096', flags: 6 })),
    }),
  ).toThrow(RangeError);
});

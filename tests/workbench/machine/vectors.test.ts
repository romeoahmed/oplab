import {
  laneFormats,
  vectorLanes,
  vectorInput,
  vectorEntries,
} from '$lib/workbench/machine/vectors';
import fc from 'fast-check';
import { expect, test } from 'vitest';

import { observation } from '../../fixtures/protocol';

const hex = (value: bigint, bits = 128) =>
  `0x${value.toString(16).padStart(Math.ceil(bits / 8) * 2, '0')}`;

test('integer and predicate lanes reconstruct every bit at all supported lengths', () => {
  fc.assert(
    fc.property(fc.uint8Array({ minLength: 256, maxLength: 256 }), (bytes) => {
      for (let vq = 1; vq <= 16; vq++) {
        for (const [length, formats] of [
          [vq * 16, ['u8', 'i8', 'u16', 'i16', 'u32', 'i32', 'u64', 'i64']],
          [vq * 2, ['bit']],
        ] as const) {
          const value = `0x${bytes.subarray(0, length).toHex()}`;
          expect(vectorInput(value, 'hex', length * 8)).toBe(value);
          expect(vectorLanes(value, 'hex')).toEqual([value.slice(2)]);
          for (const format of formats) {
            const width = format === 'bit' ? 1 : Number(format.slice(1));
            const lanes = vectorLanes(value, format);
            expect(lanes).toHaveLength((length * 8) / width);
            const restored = lanes.reduce(
              (bits, lane, index) =>
                bits | (BigInt.asUintN(width, BigInt(lane)) << BigInt(index * width)),
              0n,
            );
            expect(restored).toBe(BigInt(value));
            const signed = format.startsWith('i');
            const bound = 1n << BigInt(signed ? width - 1 : width);
            expect(
              lanes.every((lane) => BigInt(lane) >= (signed ? -bound : 0n) && BigInt(lane) < bound),
            ).toBe(true);
          }
        }
      }
    }),
    { examples: [[new Uint8Array(256)], [new Uint8Array(256).fill(255)]] },
  );
});

test.each(['f32', 'f64'] as const)(
  '%s lanes recover independently encoded IEEE-754 values',
  (format) => {
    const width = format === 'f32' ? 4 : 8;
    const count = 16 / width;
    const values = fc.array(format === 'f32' ? fc.float() : fc.double(), {
      minLength: count,
      maxLength: count,
    });
    fc.assert(
      fc.property(values, (lanes) => {
        const bytes = new Uint8Array(16);
        const data = new DataView(bytes.buffer);
        // Encode guest-order bytes, then express the complete register as wire hex.
        lanes.forEach((value, index) => {
          if (format === 'f32') data.setFloat32(index * width, value, true);
          else data.setFloat64(index * width, value, true);
        });
        expect(vectorLanes(`0x${bytes.toReversed().toHex()}`, format).map(Number)).toEqual(lanes);
      }),
      {
        examples: (format === 'f32'
          ? [
              [1.5, -0, Infinity, NaN],
              [0, -Infinity, 2 ** -149, -(2 ** -126)],
            ]
          : [
              [1.5, -Infinity],
              [-0, Number.MIN_VALUE],
              [NaN, Number.MAX_VALUE],
            ]
        ).map((lanes) => [lanes]),
      },
    );
  },
);

test('noncanonical vector input is rejected before interpretation', () => {
  const canonical = hex(0n);
  for (const invalid of [
    '0x0',
    '0x' + 'f'.repeat(33),
    '0x' + 'F'.repeat(32),
    '0x' + 'g'.repeat(32),
    '-1',
    ...[' ', '\n', '\r', '\r\n', '\u2028', '\u2029'].flatMap((space) => [
      space + canonical,
      canonical + space,
    ]),
  ])
    for (const format of laneFormats)
      expect(() => vectorLanes(invalid, format)).toThrow(RangeError);
});

test('integer edits preserve raw bits and signed two’s complement', () => {
  fc.assert(
    fc.property(fc.bigInt({ min: 0n, max: (1n << 128n) - 1n }), (bits) => {
      expect(vectorInput(bits.toString(16), 'hex', 128)).toBe(hex(bits));
      for (const format of ['u8', 'i8', 'u16', 'i16', 'u32', 'i32', 'u64', 'i64'] as const) {
        const width = BigInt(format.slice(1));
        const modulus = 2n ** width;
        const unsigned = bits % modulus;
        const signed = unsigned >= modulus / 2n ? unsigned - modulus : unsigned;
        expect(
          vectorInput((format.startsWith('i') ? signed : unsigned).toString(), format, 128),
        ).toBe(hex(unsigned, Number(width)));
      }
    }),
  );
});

test.each(['f32', 'f64'] as const)(
  '%s edits round trip finite values and signed zero',
  (format) => {
    fc.assert(
      fc.property(
        format === 'f32' ? fc.float({ noNaN: true }) : fc.double({ noNaN: true }),
        (value) => {
          const parsed = vectorInput(Object.is(value, -0) ? '-0' : String(value), format, 128);
          if (parsed === null) throw new Error('Rejected representable float');
          const bytes = new Uint8Array(Number(format.slice(1)) / 8);
          const view = new DataView(bytes.buffer);
          if (format === 'f32') view.setFloat32(0, value, true);
          else view.setFloat64(0, value, true);
          expect(parsed).toBe(`0x${bytes.toReversed().toHex()}`);
        },
      ),
      { examples: [[-0], [Infinity], [-Infinity], [1.5]] },
    );
    const nan = vectorInput('NaN', format, 128);
    if (nan === null) throw new Error('Rejected NaN');
    expect(Number(vectorLanes(nan, format)[0])).toBeNaN();
  },
);

test('vector input rejects overflow and malformed numbers instead of wrapping', () => {
  for (const format of laneFormats) {
    for (const value of ['', ' ', '1 trailing', '0x', '1'.repeat(515)])
      expect(vectorInput(value, format, 128)).toBeNull();
  }
  for (const format of ['u8', 'i8', 'u16', 'i16', 'u32', 'i32', 'u64', 'i64'] as const) {
    const width = Number(format.slice(1));
    const bound = 1n << BigInt(format.startsWith('i') ? width - 1 : width);
    for (const value of [
      bound.toString(),
      (format.startsWith('i') ? -bound - 1n : -1n).toString(),
      '1.5',
      '1e2',
    ])
      expect(vectorInput(value, format, 128)).toBeNull();
  }
  for (const format of ['f32', 'f64'] as const) {
    for (const value of ['1e999', '-1e999', '0xff', 'inf', 'nan(123)'])
      expect(vectorInput(value, format, 128)).toBeNull();
  }
  expect(vectorInput('3.5e38', 'f32', 128)).toBeNull();
  expect(vectorInput('f'.repeat(33), 'hex', 128)).toBeNull();
  expect(vectorInput('1', 'bit', 256)).toBe('0x01');
  expect(vectorInput('2', 'bit', 256)).toBeNull();
  expect(vectorInput('1'.repeat(513), 'hex', 2048)).toBeNull();
  expect(vectorInput('0X7FC12345', 'hex', 128)).toBe(hex(0x7fc12345n));
});

test('active SVE views expose low aliases and predicate bits without discarding inactive state', () => {
  fc.assert(
    fc.property(
      fc.uint8Array({ minLength: 256, maxLength: 256 }),
      fc.uint8Array({ minLength: 32, maxLength: 32 }),
      (vector, predicate) => {
        const bank = observation('1', 'aarch64').registers;
        if (bank?.type !== 'aarch64') throw new Error('Missing AArch64 bank');
        bank.data.z.fill(`0x${vector.toReversed().toHex()}`);
        bank.data.p.fill(`0x${predicate.toReversed().toHex()}`);
        bank.data.ffr = `0x${predicate.toReversed().toHex()}`;
        for (let vq = 1; vq <= 16; vq++) {
          bank.data.vl = vq * 16;
          const retained = structuredClone(bank);
          for (const [view, bytes, count, prefix] of [
            ['full', vector.subarray(0, vq * 16), 32, 'z'],
            ['low', vector.subarray(0, 16), 32, 'v'],
            ['predicates', predicate.subarray(0, vq * 2), 17, 'p'],
          ] as const) {
            expect(vectorEntries(bank, view)).toEqual(
              Array.from({ length: count }, (_, index) => ({
                name: index === 16 && view === 'predicates' ? 'ffr' : `${prefix}${String(index)}`,
                value: `0x${bytes.toReversed().toHex()}`,
              })),
            );
          }
          expect(bank).toEqual(retained);
        }
      },
    ),
  );
});

test('half precision matches the IEEE-754 sign, exponent and fraction fields', () => {
  fc.assert(
    fc.property(fc.integer({ min: 0, max: 65535 }), (bits) => {
      const sign = bits >= 32768 ? -1 : 1;
      const exponent = (bits >>> 10) & 31;
      const fraction = bits & 1023;
      const expected =
        exponent === 31
          ? fraction === 0
            ? sign * Infinity
            : NaN
          : sign *
            (exponent === 0 ? fraction * 2 ** -24 : (1024 + fraction) * 2 ** (exponent - 25));
      expect(Number(vectorLanes(hex(BigInt(bits), 16), 'f16')[0])).toBe(expected);
    }),
    {
      examples: [0, 0x8000, 1, 0x3ff, 0x400, 0x7bff, 0x7c00, 0xfc00, 0x7fff].map((bits) => [bits]),
    },
  );
  for (const [input, expected] of [
    ['-0', '0x8000'],
    ['1.5', '0x3e00'],
    ['65504', '0x7bff'],
    ['Infinity', '0x7c00'],
    ['1.00048828125', '0x3c00'],
    ['1.00146484375', '0x3c02'],
    ['0.0000000298023223876953125', '0x0000'],
    ['0.000000059604644775390625', '0x0001'],
  ] as const)
    expect(vectorInput(input, 'f16', 128)).toBe(expected);
  expect(vectorInput('65520', 'f16', 128)).toBeNull();
});

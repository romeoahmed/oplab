import { laneFormats, vectorLanes } from '$lib/workbench/machine/vectors';
import fc from 'fast-check';
import { expect, test } from 'vitest';

const hex = (value: bigint) => `0x${value.toString(16).padStart(32, '0')}`;

test('unsigned and signed lanes reconstruct every original bit in significance order', () => {
  fc.assert(
    fc.property(fc.bigInt({ min: 0n, max: (1n << 128n) - 1n }), (bits) => {
      expect(vectorLanes(hex(bits), 'hex')).toEqual([bits.toString(16).padStart(32, '0')]);
      for (const format of ['u8', 'i8', 'u16', 'i16', 'u32', 'i32', 'u64', 'i64'] as const) {
        const width = Number(format.slice(1));
        const kind = format[0];
        const lanes = vectorLanes(hex(bits), format);
        expect(lanes).toHaveLength(128 / width);
        const restored = lanes.reduce(
          (value, lane, index) =>
            value | (BigInt.asUintN(width, BigInt(lane)) << BigInt(index * width)),
          0n,
        );
        expect(restored).toBe(bits);
        const bound = 1n << BigInt(kind === 'i' ? width - 1 : width);
        for (const lane of lanes) {
          expect(BigInt(lane)).toBeGreaterThanOrEqual(kind === 'i' ? -bound : 0n);
          expect(BigInt(lane)).toBeLessThan(bound);
        }
      }
    }),
    { examples: [[0n], [(1n << 128n) - 1n], [0x80000000000000017fffffffffffffffn]] },
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

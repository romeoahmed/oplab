import { expect, test } from 'vitest';
import fc from 'fast-check';
import { decodeWindow, segmentBytes } from '$lib/workbench/instructions/bytes';

const windowInput = fc.uint8Array({ minLength: 1, maxLength: 256 }).chain((code) =>
  fc.record({
    code: fc.constant(code),
    offset: fc.integer({ min: 0, max: code.length - 1 }),
    base: fc.bigInt({ min: 0n, max: (1n << 64n) - BigInt(code.length) }),
    before: fc.uint8Array({ maxLength: 64 }),
    after: fc.uint8Array({ maxLength: 64 }),
  }),
);

test('inspection preserves segment bytes and exact 64-bit addresses at arbitrary offsets', () => {
  fc.assert(
    fc.property(windowInput, ({ code, offset, base, before, after }) => {
      const address = `0x${base.toString(16).padStart(16, '0')}`;
      const image = new Uint8Array([...before, ...code, ...after]);
      const bytes = segmentBytes(image, {
        address,
        file_offset: before.length,
        file_bytes: code.length,
        memory_bytes: String(code.length + 1024),
        flags: 5,
      });
      expect(bytes).toEqual(code);
      const window = decodeWindow(bytes, address, offset);
      expect(window.bytes).toEqual(code.slice(offset));
      expect(window.base).toBe(`0x${(base + BigInt(offset)).toString(16).padStart(16, '0')}`);
    }),
  );
});

test('byte windows reject invalid geometry without clamping or wrapping', () => {
  const bytes = new Uint8Array([0x90]);
  expect(decodeWindow(bytes, '0xffffffffffffffff', 0).bytes).toEqual(bytes);
  expect(() => decodeWindow(new Uint8Array(2), '0xffffffffffffffff', 0)).toThrow(RangeError);
  for (const offset of [-1, 0.5, 1, NaN])
    expect(() => decodeWindow(bytes, '0x0000000000001000', offset)).toThrow(RangeError);
  for (const [start, length] of [
    [-1, 1],
    [0.5, 1],
    [NaN, 1],
    [1, 1],
    [0, 0],
    [0, -1],
    [0, 0.5],
    [0, Infinity],
  ] as const) {
    expect(() =>
      segmentBytes(bytes, {
        address: '0x0000000000001000',
        file_offset: start,
        file_bytes: length,
        memory_bytes: '1',
        flags: 5,
      }),
    ).toThrow(RangeError);
  }
});

test('large ELF extents stay exportable while each decode request stays bounded', () => {
  const image = Uint8Array.from({ length: 200000 }, (_, index) => index % 251);
  const bytes = segmentBytes(image, {
    address: '0x0000000000001000',
    file_offset: 4096,
    file_bytes: 131073,
    memory_bytes: '262144',
    flags: 5,
  });
  expect(bytes).toEqual(image.subarray(4096, 135169));
  for (const offset of [0, 65535, 65536, 131072]) {
    const window = decodeWindow(bytes, '0x0000000000001000', offset);
    expect(window.bytes).toEqual(
      image.subarray(4096 + offset, Math.min(4096 + offset + 65536, 135169)),
    );
    expect(window.base).toBe(`0x${(0x1000n + BigInt(offset)).toString(16).padStart(16, '0')}`);
  }
});

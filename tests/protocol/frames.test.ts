import { encodeCall, decodeResponse, decodeStream } from '$lib/protocol/frames';
import type { DesktopCall } from '$lib/protocol/generated/DesktopCall';
import type { LoadImage } from '$lib/protocol/generated/LoadImage';
import fc from 'fast-check';
import { expect, test } from 'vitest';

// Independent wire fixtures, including literal little-endian header lengths.
const closed = new Uint8Array([
  79,
  80,
  0,
  0,
  52,
  0,
  0,
  0,
  ...new TextEncoder().encode('{"id":"9007199254740993","result":{"type":"closed"}}'),
]);
const delta = new Uint8Array([
  79,
  80,
  2,
  0,
  21,
  1,
  0,
  0,
  ...new TextEncoder().encode(
    '{"subscription":"3","update":{"type":"delta","data":{"key":{"session":"2","generation":"0"},"base":"9007199254740993","sequence":"9007199254740994","status":{"type":"running"},"instructions":"1","dispatches":"1","registers":{"type":"unchanged"},"fault":null,"memory_bytes":8}}}',
  ),
  79,
  80,
  1,
  0,
  8,
  0,
  0,
  0,
]);

test('native replies retain exact counters and reject incomplete or malformed framing', () => {
  expect(decodeResponse(closed.buffer)).toEqual({
    response: { id: '9007199254740993', result: { type: 'closed' } },
    payloads: [],
  });
  for (let length = 0; length < closed.length; length++)
    expect(() => decodeResponse(closed.slice(0, length).buffer)).toThrow();
  for (const offset of [0, 1, 2, 3, 4, 7]) {
    const malformed = closed.slice();
    malformed[offset] = 255;
    expect(() => decodeResponse(malformed.buffer)).toThrow(RangeError);
  }
  expect(() => decodeResponse(new Uint8Array([...closed, 0]).buffer)).toThrow(RangeError);
});

test('stream memory preserves arbitrary bytes and publishes only complete windows', () => {
  fc.assert(
    fc.property(fc.uint8Array({ minLength: 8, maxLength: 8 }), (bytes) => {
      const complete = new Uint8Array([...delta, ...bytes]);
      expect(decodeStream(complete.buffer).memory).toEqual(bytes);
      for (let missing = 1; missing <= 8; missing++)
        expect(() => decodeStream(complete.slice(0, -missing).buffer)).toThrow(RangeError);
    }),
  );
});

test.each([
  { type: 'elf' },
  { type: 'raw', data: { base: '0x0020000000000000', entry: '0x0020000000000000' } },
] satisfies LoadImage[])('$type load calls preserve every documented chunk boundary', (format) => {
  for (const length of [1, 65535, 65536, 65537, 131073, 1048576]) {
    const image = Uint8Array.from(
      { length },
      (_, index) => (index ^ (index >>> 8) ^ (index >>> 16)) & 255,
    );
    const call: DesktopCall = {
      connection: '9007199254740993',
      view: '2',
      command: {
        type: 'load',
        data: {
          image: format,
          replace: null,
          target: 'aarch64',
          completion: '0x0000000000001008',
          instruction_budget: '100',
          image_bytes: length,
          initial: { registers: [], mappings: [] },
        },
      },
    };
    const bytes = encodeCall(call, image);
    // Independent consumer of the documented envelope, not the production decoder.
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    const controlLength = view.getUint32(4, true);
    expect(bytes.slice(0, 4)).toEqual(new Uint8Array([79, 80, 0, 0]));
    expect(JSON.parse(new TextDecoder().decode(bytes.slice(8, 8 + controlLength)))).toEqual(call);
    let offset = 8 + controlLength;
    let consumed = 0;
    while (consumed < image.length) {
      expect(bytes.slice(offset, offset + 4)).toEqual(new Uint8Array([79, 80, 1, 0]));
      const size = view.getUint32(offset + 4, true);
      expect(size).toBe(Math.min(65536, image.length - consumed));
      expect(
        bytes
          .subarray(offset + 8, offset + 8 + size)
          .every((byte, index) => byte === image[consumed + index]),
      ).toBe(true);
      consumed += size;
      offset += 8 + size;
    }
    expect(offset).toBe(bytes.length);
    expect(() => encodeCall(call, image.subarray(1))).toThrow(RangeError);
  }
  expect(() =>
    encodeCall({ connection: '1', view: '1', command: { type: 'shutdown' } }, new Uint8Array(1)),
  ).toThrow(RangeError);
});

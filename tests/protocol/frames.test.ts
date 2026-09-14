import { expect, test } from 'vitest';
import fc from 'fast-check';
import { decodeResponse, decodeStream } from '$lib/protocol/frames';

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

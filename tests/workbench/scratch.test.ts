import { readScratch, type Scratch } from '$lib/workbench/scratch';
import fc from 'fast-check';
import { expect, test } from 'vitest';

const document: Scratch = {
  documentId: 'fixture',
  revision: '9007199254740993',
  source: '// \u4e2d\u6587\nmov x0, #42',
  target: 'aarch64',
  base: '0x',
  completion: '',
  budget: '',
};

test('scratch recovery preserves exact revisions, Unicode, and incomplete human inputs', () => {
  fc.assert(
    fc.property(
      fc.bigInt({ min: 0n, max: (1n << 64n) - 1n }),
      fc.string({ unit: 'grapheme', maxLength: 200 }),
      fc.constantFrom('x86_64' as const, 'aarch64' as const),
      fc.record({
        base: fc.string({ maxLength: 40 }),
        completion: fc.string({ maxLength: 80 }),
        budget: fc.string({ maxLength: 40 }),
      }),
      (revision, source, target, inputs) => {
        const scratch = { ...document, ...inputs, revision: String(revision), source, target };
        expect(readScratch(JSON.parse(JSON.stringify(scratch)))).toEqual(scratch);
      },
    ),
  );
});

test('malformed recovery inputs cannot cross the document boundary', () => {
  for (const value of [
    null,
    [],
    {},
    { ...document, revision: 9007199254740993n },
    { ...document, revision: '18446744073709551616' },
    { ...document, documentId: '../fixture' },
    { ...document, target: 'arm' },
  ])
    expect(() => readScratch(value)).toThrow(RangeError);
});

test.each(['a', '\u00e9', '\u20ac', '\u{1f600}'])(
  'scratch limits count UTF-8 bytes for %j, not code units or characters',
  (scalar) => {
    const encoder = new TextEncoder();
    const budget = 256 * 1024;
    const width = encoder.encode(scalar).length;
    for (const size of [budget - 1, budget, budget + 1]) {
      const source = scalar.repeat(Math.floor(size / width)) + 'a'.repeat(size % width);
      const draft = { ...document, source };
      expect(encoder.encode(source)).toHaveLength(size);
      if (size <= budget) expect(readScratch(draft)).toEqual(draft);
      else expect(() => readScratch(draft)).toThrow(RangeError);
    }
  },
);

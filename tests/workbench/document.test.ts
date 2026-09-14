import { expect, test } from 'vitest';
import fc from 'fast-check';
import { readScratch, type Scratch } from '$lib/workbench/document';

const document: Scratch = {
  documentId: 'fixture',
  revision: '9007199254740993',
  source: '// 中文\nmov x0, #42',
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
      (revision, source, target) => {
        const scratch = { ...document, revision: String(revision), source, target };
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
    { ...document, source: '中'.repeat(100000) },
  ])
    expect(() => readScratch(value)).toThrow(RangeError);
});

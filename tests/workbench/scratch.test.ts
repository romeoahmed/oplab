import { readScratch, readWorkspace, type Scratch } from '$lib/workbench/scratch';
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

const draft = fc.record({
  documentId: fc.uuid(),
  revision: fc.bigInt({ min: 0n, max: (1n << 64n) - 1n }).map(String),
  source: fc.string({ unit: 'grapheme', maxLength: 200 }),
  target: fc.constantFrom('x86_64' as const, 'aarch64' as const),
  base: fc.string({ maxLength: 80 }),
  completion: fc.string({ maxLength: 1024 }),
  budget: fc.string({ maxLength: 80 }),
});

test('scratch recovery preserves exact revisions, Unicode, and incomplete human inputs', () => {
  fc.assert(
    fc.property(draft, (scratch) => {
      expect(readScratch(JSON.parse(JSON.stringify(scratch)))).toEqual(scratch);
    }),
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

test('workspace recovery preserves document order and rejects ambiguous identities atomically', () => {
  const workspaces = fc
    .uniqueArray(
      fc.tuple(draft, fc.string({ maxLength: 32 })).map(([inputs, name]) => ({
        ...inputs,
        name: `source-${name}`,
      })),
      { minLength: 1, maxLength: 48, selector: (entry) => entry.documentId },
    )
    .chain((documents) =>
      fc.record({
        documents: fc.constant(documents),
        activeIndex: fc.integer({ min: 0, max: documents.length - 1 }),
        damagedIndex: fc.integer({ min: 0, max: documents.length - 1 }),
      }),
    );
  fc.assert(
    fc.property(workspaces, ({ documents, activeIndex, damagedIndex }) => {
      const workspace = { active: documents[activeIndex]?.documentId, documents };
      expect(readWorkspace(JSON.parse(JSON.stringify(workspace)))).toEqual(workspace);
      expect(() => readWorkspace({ ...workspace, active: 'missing' })).toThrow(RangeError);
      expect(() =>
        readWorkspace({
          ...workspace,
          documents: [...documents, { ...documents[activeIndex], source: 'different contents' }],
        }),
      ).toThrow(RangeError);
      for (const damage of [{ name: ' \t' }, { revision: '-1' }, { source: null }]) {
        expect(() =>
          readWorkspace({
            ...workspace,
            documents: documents.map((entry, index) =>
              index === damagedIndex ? { ...entry, ...damage } : entry,
            ),
          }),
        ).toThrow(RangeError);
      }
    }),
  );
  expect(() => readWorkspace({ active: 'fixture', documents: [] })).toThrow(RangeError);
  expect(() =>
    readWorkspace({ active: 'fixture', documents: [{ ...document, name: '' }] }),
  ).toThrow(RangeError);
});

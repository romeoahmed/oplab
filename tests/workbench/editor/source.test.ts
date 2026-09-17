import { sourceInfo, sourceLocation, sourceIndex } from '$lib/workbench/editor/source';
import { EditorState } from '@codemirror/state';
import fc from 'fast-check';
import { expect, test } from 'vitest';

const encoder = new TextEncoder();

test('UTF-8 boundaries locate the same Unicode prefix in the editor', () => {
  fc.assert(
    fc.property(
      fc.array(
        fc.oneof(
          fc.string({ unit: 'grapheme', maxLength: 4 }),
          fc.constantFrom('\r\n', '\n', '\r', '\uFEFF'),
        ),
        { maxLength: 40 },
      ),
      fc.nat(),
      (parts, index) => {
        const source = parts.join('');
        expect(sourceInfo(source).lines).toBe(EditorState.create({ doc: source }).doc.lines);
        const scalars = Array.from(source);
        const prefix = scalars.slice(0, index % (scalars.length + 1)).join('');
        const offset = encoder.encode(prefix).length;
        // Let CodeMirror define line normalization and UTF-16 positions.
        // The interior CRLF boundary is covered by an explicit case below.
        if (!(prefix.endsWith('\r') && source[prefix.length] === '\n')) {
          const document = EditorState.create({ doc: prefix }).doc;
          const line = document.lineAt(document.length);
          expect(sourceLocation(source, offset)).toEqual({
            position: document.length,
            line: line.number,
            column: document.length - line.from + 1,
          });
        }
        const next = scalars[index % (scalars.length + 1)];
        if (next !== undefined) {
          for (let byte = 1; byte < encoder.encode(next).length; byte += 1)
            expect(sourceLocation(source, offset + byte)).toBeNull();
        }
      },
    ),
  );
});

test('locations preserve BOM, supplementary characters, combining marks and EOF', () => {
  const source = '\uFEFF// \u{1f600}e\u0301\r\n\u4e2d\u6587: invalid';
  expect(sourceInfo(source)).toEqual({ lines: 2, ending: 'CRLF' });
  expect(sourceInfo('a\r\nb\rc\n')).toEqual({ lines: 4, ending: 'mixed' });
  const prefix = source.slice(0, source.indexOf('invalid'));
  expect(sourceLocation(source, encoder.encode(prefix).length)).toEqual({
    position: 13,
    line: 2,
    column: 5,
  });
  expect(sourceLocation('a\r\nb', 2)).toEqual({ position: 1, line: 1, column: 2 });
  expect(sourceLocation('a\r\nb', 3)).toEqual({ position: 2, line: 2, column: 1 });
  expect(sourceLocation('', 0)).toEqual({ position: 0, line: 1, column: 1 });
  expect(sourceLocation('nop\n', 4)).toEqual({ position: 4, line: 2, column: 1 });
  for (const offset of [null, -1, 0.5, NaN, Infinity, 4])
    expect(sourceLocation('nop', offset)).toBeNull();
  expect(sourceLocation('\uD800', 0)).toBeNull();
});

test('source indexes contain exactly the input relation in both directions', () => {
  fc.assert(
    fc.property(
      fc.array(
        fc.record({
          line: fc.integer({ min: 1, max: 16 }),
          address: fc.integer({ min: 0, max: 15 }).map((n) => `0x${n.toString(16)}`),
        }),
        { maxLength: 80 },
      ),
      (points) => {
        const index = sourceIndex(points);
        expect(new Set(index.lines.keys())).toEqual(new Set(points.map((point) => point.line)));
        expect(new Set(index.addresses.keys())).toEqual(
          new Set(points.map((point) => point.address)),
        );
        for (const [line, addresses] of index.lines)
          expect(addresses).toEqual(
            new Set(points.filter((point) => point.line === line).map((point) => point.address)),
          );
        for (const [address, lines] of index.addresses)
          expect(lines).toEqual(
            new Set(points.filter((point) => point.address === address).map((point) => point.line)),
          );
      },
    ),
  );
});

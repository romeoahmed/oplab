import type { Target } from '$lib/protocol/generated/Target';
import { assembly } from '$lib/workbench/editor/language';
import { ensureSyntaxTree } from '@codemirror/language';
import { EditorState } from '@codemirror/state';
import { classHighlighter, highlightTree } from '@lezer/highlight';
import fc from 'fast-check';
import { expect, test } from 'vitest';

function highlighted(source: string, target: Target) {
  const state = EditorState.create({ doc: source, extensions: [assembly(target)] });
  const tree = ensureSyntaxTree(state, state.doc.length, 1000);
  if (tree === null) throw new Error('Language did not finish parsing');
  const result: { text: string; style: string }[] = [];
  highlightTree(tree, classHighlighter, (from, to, style) => {
    result.push({ text: state.sliceDoc(from, to), style });
  });
  return { state, tree, result };
}

test('target comments, immediate operands and quoted comment markers stay distinct', () => {
  for (const target of ['x86_64', 'aarch64'] as const) {
    const comment = target === 'x86_64' ? '# actual comment' : '// actual comment';
    const { result } = highlighted(
      `.ascii "# // literal"\n${comment}\nmov ${target === 'x86_64' ? 'rax, 42' : 'x0, #42'}`,
      target,
    );
    expect(
      result.some(({ text, style }) => text === comment && style.includes('tok-comment')),
    ).toBe(true);
    expect(
      result.some(
        ({ text, style }) => text.includes('# // literal') && style.includes('tok-string'),
      ),
    ).toBe(true);
    expect(result.some(({ text, style }) => text === '42' && style.includes('tok-number'))).toBe(
      true,
    );
  }
});

test('incremental highlighting agrees with a fresh parse after arbitrary edits', () => {
  const text = fc.oneof(
    fc.string({ unit: 'grapheme', maxLength: 120 }),
    fc
      .array(
        fc.constantFrom(
          '/*',
          '*/',
          '#',
          '//',
          '\n',
          '\r\n',
          '"',
          '\\',
          'mov x0, #42',
          'label:',
          '中文',
        ),
        { maxLength: 30 },
      )
      .map((parts) => parts.join('')),
  );
  fc.assert(
    fc.property(
      text,
      fc.constantFrom<Target>('x86_64', 'aarch64'),
      text,
      fc.nat(),
      fc.nat(),
      (source, target, insert, left, right) => {
        const { state } = highlighted(source, target);
        const first = left % (state.doc.length + 1);
        const last = right % (state.doc.length + 1);
        const updated = state.update({
          changes: { from: Math.min(first, last), to: Math.max(first, last), insert },
        }).state;
        const tree = ensureSyntaxTree(updated, updated.doc.length, 1000);
        if (tree === null) throw new Error('Incremental parse did not complete');
        const spans: { text: string; style: string }[] = [];
        highlightTree(tree, classHighlighter, (from, to, style) => {
          spans.push({ text: updated.sliceDoc(from, to), style });
        });
        expect(spans).toEqual(highlighted(updated.sliceDoc(), target).result);
      },
    ),
  );
});

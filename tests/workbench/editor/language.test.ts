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
          '\u4e2d\u6587',
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

test('GNU labels, prefixes, AT&T immediates, SIMD arrangements and macros retain their roles', () => {
  const cases = [
    {
      target: 'x86_64',
      source: 'again: lock addq $0x2a, %rax; movdqu %xmm15, (%rdi)\n1: jmp 1b\n.ascii "\u03bb;#"',
      expected: [
        ['again', 'labelName'],
        ['lock', 'keyword'],
        ['addq', 'keyword'],
        ['0x2a', 'number'],
        ['rax', 'variableName2'],
        ['xmm15', 'variableName2'],
        ['1b', 'labelName'],
      ],
    },
    {
      target: 'aarch64',
      source:
        '.macro sum reg\nadd \\reg, v1.4s, v31.4s\n.endm\n.L\u503c: mov x0, #42\n# line marker\n.float 1.25e-3',
      expected: [
        ['.macro', 'meta'],
        ['\\reg', 'variableName2'],
        ['v1.4s', 'variableName2'],
        ['v31.4s', 'variableName2'],
        ['.L\u503c', 'labelName'],
        ['42', 'number'],
        ['1.25e-3', 'number'],
      ],
    },
  ] as const;
  for (const { target, source, expected } of cases) {
    const { result } = highlighted(source, target);
    for (const [text, role] of expected) {
      expect(result.some((span) => span.text === text && span.style.includes(`tok-${role}`))).toBe(
        true,
      );
    }
  }
});

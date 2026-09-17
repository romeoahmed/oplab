import { assembly } from '$lib/workbench/editor/language';
import { documentStructure, labelDestination } from '$lib/workbench/editor/structure';
import { CompletionContext, type CompletionSource } from '@codemirror/autocomplete';
import { ensureSyntaxTree, foldable } from '@codemirror/language';
import { EditorState } from '@codemirror/state';
import { expect, test } from 'vitest';

function parsed(doc: string) {
  const state = EditorState.create({ doc, extensions: [assembly('x86_64')] });
  expect(ensureSyntaxTree(state, state.doc.length, 1000)).not.toBeNull();
  return state;
}

test('folding follows nested directives, excluding strings and comments', () => {
  const state = parsed(
    '.macro sum reg\n/* .endm */\n.rept 2\n.ascii ".endr"\nadd \\reg, 1\n.endr\n.endm',
  );
  const outer = foldable(state, 0, state.doc.line(1).to);
  const inner = foldable(state, state.doc.line(3).from, state.doc.line(3).to);
  expect(outer).toEqual({ from: state.doc.line(1).to, to: state.doc.line(7).from - 1 });
  expect(inner).toEqual({ from: state.doc.line(3).to, to: state.doc.line(6).from - 1 });
  expect(foldable(parsed('.if 1\n nop'), 0, 5)).toBeNull();
  for (const condition of ['.ifeqs "a", "a"', '.ifnes "a", "b"', '.ifnotdef symbol']) {
    const conditional = parsed(`${condition}\nnop\n.else\nint3\n.endif`);
    expect(foldable(conditional, 0, conditional.doc.line(1).to)).toEqual({
      from: conditional.doc.line(1).to,
      to: conditional.doc.line(5).from - 1,
    });
  }
});

test('label navigation respects numeric direction, Unicode, case and ambiguous definitions', () => {
  const source =
    '1: nop\njmp 1f\njmp 1b\n1: nop\n.L\u503c: nop\njmp .L\u503c\n# fake: jmp .L\u503c\n.ascii "fake:"\nfoo: nop\nfoo: nop\njmp foo';
  const state = parsed(source);
  const resolve = (text: string) => labelDestination(state, source.indexOf(text) + text.length);
  expect(resolve('jmp 1f')?.from).toBe(source.indexOf('1: nop', 1));
  expect(resolve('jmp 1b')?.from).toBe(0);
  expect(resolve('jmp .L\u503c')?.from).toBe(source.indexOf('.L\u503c:'));
  expect(resolve('jmp foo')).toBeUndefined();
  expect(labelDestination(state, source.indexOf('# fake:') + 7)).toBeUndefined();
  expect(documentStructure(state).labels.map(({ name }) => name)).not.toContain('fake');
  const changed = state.update({
    changes: {
      from: source.indexOf('.L\u503c:'),
      to: source.indexOf('.L\u503c:') + 3,
      insert: '.L\u65b0',
    },
  }).state;
  expect(
    labelDestination(changed, source.indexOf('jmp .L\u503c') + 'jmp .L\u503c'.length),
  ).toBeUndefined();
});

test('completion includes literal labels and target vocabulary but excludes comments and strings', async () => {
  const state = parsed('.Lanswer: nop\nmov %xmm0, %xmm1\njmp .La\n# comment\n.ascii "string"');
  async function complete(position: number) {
    const sources = state.languageDataAt<CompletionSource>('autocomplete', position);
    return Promise.all(
      sources.map(async (source) => source(new CompletionContext(state, position, true))),
    );
  }
  const results = await complete(state.doc.line(3).to);
  expect(results.flatMap((result) => result?.options.map(({ label }) => label) ?? [])).toEqual(
    expect.arrayContaining(['.Lanswer', 'xmm15', 'movdqu']),
  );
  for (const position of [state.doc.line(4).to, state.doc.line(5).to - 1]) {
    const completions = await complete(position);
    expect(completions.flatMap((result) => result?.options ?? [])).toEqual([]);
  }
});

test('explicit navigation parses beyond the initial viewport without changing the document', () => {
  const source = `jmp distant\n${'# padding\n'.repeat(800)}distant: nop`;
  const state = EditorState.create({ doc: source, extensions: [assembly('x86_64')] });
  expect(labelDestination(state, 'jmp distant'.length)?.from).toBe(source.lastIndexOf('distant:'));
});

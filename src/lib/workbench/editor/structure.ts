import { syntaxTree, ensureSyntaxTree } from '@codemirror/language';
import type { EditorState } from '@codemirror/state';
import { EditorView, type Command } from '@codemirror/view';

type Label = { name: string; from: number; to: number };
type Structure = { labels: Label[]; folds: Map<number, { from: number; to: number }> };
const structures = new WeakMap<ReturnType<typeof syntaxTree>, Structure>();
const blocks = new Map([
  ['.macro', '.endm'],
  ['.cfi_startproc', '.cfi_endproc'],
  ['.rept', '.endr'],
  ['.irp', '.endr'],
  ['.irpc', '.endr'],
]);

/** Index only parsed tokens, excluding comments and strings; cache by immutable Lezer tree. */
export function documentStructure(state: EditorState, tree = syntaxTree(state)): Structure {
  const cached = structures.get(tree);
  if (cached !== undefined) return cached;
  const result: Structure = { labels: [], folds: new Map() };
  const stack: { end: string; start: number; from: number }[] = [];
  tree.iterate({
    enter(node) {
      if (node.name === 'labelDefinition') {
        result.labels.push({
          name: state.sliceDoc(node.from, node.to),
          from: node.from,
          to: node.to,
        });
      } else if (node.name === 'directive') {
        const word = state.sliceDoc(node.from, node.to).toLowerCase();
        const end =
          blocks.get(word) ??
          (/^\.if(?:def|ndef|notdef|eqs?|nes?|gt|ge|lt|le|c|nc|b|nb)?$/.test(word)
            ? '.endif'
            : undefined);
        const line = state.doc.lineAt(node.from);
        if (end !== undefined) stack.push({ end, start: line.from, from: line.to });
        else if (word === stack.at(-1)?.end) {
          const open = stack.pop();
          if (open !== undefined && line.from > open.from)
            result.folds.set(open.start, { from: open.from, to: line.from - 1 });
        }
      }
    },
  });
  structures.set(tree, result);
  return result;
}

/** Resolve a unique literal label or nearest numeric `b`/`f` label; no macro expansion. */
export function labelDestination(state: EditorState, position: number): Label | undefined {
  const tree = ensureSyntaxTree(state, state.doc.length, 50);
  if (tree === null) return undefined;
  const node = tree.resolveInner(position, -1);
  if (!['symbol', 'localReference', 'labelDefinition'].includes(node.name)) return undefined;
  const word = state.sliceDoc(node.from, node.to);
  const labels = documentStructure(state, tree).labels;
  if (/^\d+[bf]$/.test(word)) {
    const name = word.slice(0, -1);
    return word.endsWith('b')
      ? labels.findLast((label) => label.name === name && label.from < node.from)
      : labels.find((label) => label.name === name && label.from > node.to);
  }
  const matches = labels.filter(({ name }) => name === word);
  return matches.length === 1 ? matches[0] : undefined;
}

export const jumpToLabel: Command = (view) => {
  const label = labelDestination(view.state, view.state.selection.main.head);
  if (label === undefined) return false;
  view.dispatch({
    selection: { anchor: label.from, head: label.to },
    effects: EditorView.scrollIntoView(label.from, { y: 'center' }),
    userEvent: 'select',
  });
  view.focus();
  return true;
};

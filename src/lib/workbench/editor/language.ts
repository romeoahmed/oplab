import {
  LanguageSupport,
  StreamLanguage,
  syntaxTree,
  HighlightStyle,
  syntaxHighlighting,
} from '@codemirror/language';
import {
  completeFromList,
  completeAnyWord,
  type CompletionContext,
} from '@codemirror/autocomplete';
import { tags } from '@lezer/highlight';
import type { Target } from '$lib/protocol/generated/Target';

const registers: Record<Target, string[]> = {
  x86_64: [
    'rax',
    'rbx',
    'rcx',
    'rdx',
    'rsi',
    'rdi',
    'rsp',
    'rbp',
    'rip',
    'eax',
    'ebx',
    'ecx',
    'edx',
    'esi',
    'edi',
    'esp',
    'ebp',
    'ax',
    'bx',
    'cx',
    'dx',
    'al',
    'bl',
    'cl',
    'dl',
    ...Array.from({ length: 8 }, (_, index) => `r${String(index + 8)}`).flatMap((name) => [
      name,
      `${name}d`,
      `${name}w`,
      `${name}b`,
    ]),
  ],
  aarch64: [
    ...Array.from({ length: 31 }, (_, index) => [`x${String(index)}`, `w${String(index)}`]).flat(),
    'sp',
    'wsp',
    'xzr',
    'wzr',
  ],
};
const directives = [
  '.text',
  '.data',
  '.bss',
  '.section',
  '.global',
  '.type',
  '.size',
  '.byte',
  '.word',
  '.long',
  '.quad',
  '.ascii',
  '.asciz',
  '.skip',
  '.balign',
  '.p2align',
  '.equ',
  '.macro',
  '.endm',
  '.rept',
  '.endr',
];

/** Lexical assistance only. LLVM owns grammar, macro expansion, and instruction validity. */
export function assembly(target: Target): LanguageSupport {
  const names = new Set(registers[target]);
  const language = StreamLanguage.define({
    name: `assembly-${target}`,
    startState: () => ({ comment: false, head: true }),
    token(stream, state) {
      if (stream.sol()) state.head = true;
      if (state.comment) {
        if (stream.skipTo('*/')) {
          stream.match('*/');
          state.comment = false;
        } else stream.skipToEnd();
        return 'blockComment';
      }
      if (stream.eatSpace()) return null;
      if (stream.match('/*')) {
        state.comment = true;
        return 'blockComment';
      }
      if (stream.match(target === 'x86_64' ? '#' : '//')) {
        stream.skipToEnd();
        return 'lineComment';
      }
      if (stream.match(/"(?:[^"\\]|\\.)*(?:"|$)/)) return 'string';
      if (stream.match(/(?:[\p{L}_.$][\p{L}\p{N}_.$]*|\d+):/u)) return 'labelName';
      if (stream.match(/(?:0[xX][\da-fA-F]+|0[bB][01]+|\d+)(?:[bf]\b)?/)) return 'number';
      if (stream.match(/%?[\p{L}_.$][\p{L}\p{N}_.$]*/u)) {
        const word = stream.current().replace(/^%/, '').toLowerCase();
        if (names.has(word) || /^(?:[xyz]mm\d+|[bhsdqv]\d+)$/.test(word))
          return 'variableName.special';
        if (state.head) {
          state.head = false;
          return word.startsWith('.') ? 'meta' : 'keyword';
        }
        return 'variableName';
      }
      const character = stream.next();
      if (character === ';') state.head = true;
      return character !== undefined && '[](){}'.includes(character) ? 'bracket' : 'operator';
    },
    languageData: {
      commentTokens: { line: target === 'x86_64' ? '#' : '//', block: { open: '/*', close: '*/' } },
    },
  });
  const complete = completeFromList([
    ...registers[target].map((label) => ({ label, type: 'variable' })),
    ...[...directives, ...(target === 'x86_64' ? ['.intel_syntax', '.att_syntax'] : [])].map(
      (label) => ({ label, type: 'keyword' }),
    ),
  ]);
  const outsideLiteral = (context: CompletionContext) =>
    !/comment|string/i.test(syntaxTree(context.state).resolveInner(context.pos, -1).name);
  return new LanguageSupport(language, [
    language.data.of({
      autocomplete: (context: CompletionContext) =>
        outsideLiteral(context) ? complete(context) : null,
    }),
    language.data.of({
      autocomplete: async (context: CompletionContext) => {
        if (!outsideLiteral(context)) return null;
        const words = await completeAnyWord(context);
        return words === null
          ? null
          : {
              ...words,
              options: words.options.filter(
                ({ label }) => !names.has(label.toLowerCase()) && !directives.includes(label),
              ),
            };
      },
    }),
  ]);
}

export const assemblyHighlighting = syntaxHighlighting(
  HighlightStyle.define([
    { tag: tags.keyword, color: 'var(--syntax-instruction)' },
    { tag: tags.meta, color: 'var(--syntax-directive)' },
    { tag: tags.special(tags.variableName), color: 'var(--syntax-register)' },
    { tag: [tags.labelName, tags.variableName], color: 'var(--text)' },
    { tag: [tags.number, tags.string], color: 'var(--syntax-literal)' },
    { tag: tags.comment, color: 'var(--muted)' },
    { tag: [tags.operator, tags.bracket], color: 'var(--muted)' },
  ]),
);

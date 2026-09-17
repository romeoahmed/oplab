import type { Target } from '$lib/protocol/generated/Target';
import { type CompletionContext } from '@codemirror/autocomplete';
import {
  LanguageSupport,
  StreamLanguage,
  syntaxTree,
  HighlightStyle,
  syntaxHighlighting,
  foldService,
} from '@codemirror/language';
import { tags } from '@lezer/highlight';

import { documentStructure } from './structure';
import { directives, instructions, registers } from './vocabulary';

const identifier = /[\p{L}_.$][\p{L}\p{N}_.$]*/u;
const prefixes = new Set('lock rep repe repz repne repnz data16 addr32 rex rex.w'.split(' '));

/** Lexical assistance only. LLVM owns grammar, macro expansion and instruction validity. */
export function assembly(target: Target): LanguageSupport {
  const names = new Set(registers[target]);
  const language = StreamLanguage.define({
    name: `assembly-${target}`,
    mergeTokens: false,
    tokenTable: {
      labelDefinition: tags.labelName,
      localReference: tags.labelName,
      symbol: tags.variableName,
      directive: tags.meta,
      register: tags.special(tags.variableName),
      mnemonic: tags.keyword,
      parameter: tags.special(tags.variableName),
    },
    startState: () => ({ comment: false, head: true }),
    token(stream, state) {
      if (stream.sol()) state.head = true;
      if (state.comment || stream.match('/*')) {
        state.comment = true;
        if (stream.skipTo('*/')) {
          stream.match('*/');
          state.comment = false;
        } else stream.skipToEnd();
        return 'blockComment';
      }
      if (stream.eatSpace()) return null;
      if (stream.match('//') || ((target === 'x86_64' || state.head) && stream.match('#'))) {
        stream.skipToEnd();
        return 'lineComment';
      }
      if (stream.match(/"(?:[^"\\]|\\.)*(?:"|$)/)) return 'string';
      if (stream.match(/'(?:\\.|[^\\\s])'?/)) return 'character';
      if (state.head && stream.match(/(?:[\p{L}_.$][\p{L}\p{N}_.$]*|\d+)(?=\s*:)/u))
        return 'labelDefinition';
      if (stream.match(/\\(?:[\p{L}_][\p{L}\p{N}_]*|[@+]|\(\))/u)) return 'parameter';
      if (stream.match(/\d+[bf]\b/)) return 'localReference';
      if (stream.match(/(?:0[xX][\da-fA-F]+|0[bB][01]+|(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?)/))
        return 'number';
      if (target === 'aarch64' && stream.match(/:[a-z][a-z0-9_]*:/i)) return 'modifier';
      if (target === 'x86_64' && stream.match(/@[a-z][a-z0-9_]*/i)) return 'modifier';
      if (stream.match('%')) return 'operator';
      if (target === 'x86_64' && stream.match('$')) return 'operator';
      if (stream.match(identifier)) {
        const word = stream.current().toLowerCase();
        if (state.head) {
          if (target !== 'x86_64' || !prefixes.has(word)) state.head = false;
          return word.startsWith('.') ? 'directive' : 'mnemonic';
        }
        const base = target === 'aarch64' ? word.replace(/\.(?:\d*[bhsdq]|[bhsd])$/, '') : word;
        if (names.has(base)) return 'register';
        if (
          target === 'x86_64' &&
          /^(?:byte|word|dword|qword|tbyte|xmmword|ymmword|zmmword|ptr|offset|short)$/.test(word)
        )
          return 'typeName';
        return 'symbol';
      }
      const character = stream.next();
      if (character === ';') state.head = true;
      return character !== undefined && '[](){}'.includes(character) ? 'bracket' : 'operator';
    },
    languageData: {
      commentTokens: { line: target === 'x86_64' ? '#' : '//', block: { open: '/*', close: '*/' } },
      // Dots and dollars are part of GNU symbol names, including local .L labels.
      wordChars: '.$',
      closeBrackets: { brackets: ['(', '[', '{', '"'] },
    },
  });
  const choices = [
    ...registers[target].map((label) => ({ label, type: 'variable' })),
    ...instructions[target].map((label) => ({ label, type: 'keyword' })),
    ...[
      ...directives,
      ...(target === 'x86_64'
        ? ['.intel_syntax', '.att_syntax']
        : ['.arch', '.arch_extension', '.cpu', '.inst']),
    ].map((label) => ({ label, type: 'keyword' })),
  ];
  return new LanguageSupport(language, [
    foldService.of((state, start) => documentStructure(state).folds.get(start) ?? null),
    language.data.of({
      autocomplete: (context: CompletionContext) => {
        const node = syntaxTree(context.state).resolveInner(context.pos, -1);
        if (/comment|string|character|parameter/i.test(node.name)) return null;
        const word = context.matchBefore(/[\p{L}\p{N}_.$]+/u);
        if (word === null && !context.explicit) return null;
        const labels = documentStructure(context.state)
          .labels.filter(({ name }) => !/^\d+$/.test(name))
          .map(({ name }) => ({ label: name, type: 'constant', boost: 2 }));
        const options = [
          ...new Map([...choices, ...labels].map((item) => [item.label, item])).values(),
        ];
        const from = word?.from ?? context.pos;
        return {
          from: from + Number(target === 'x86_64' && word?.text.startsWith('$')),
          options,
          validFor: /^[\p{L}\p{N}_.$]*$/u,
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
    { tag: tags.labelName, color: 'var(--syntax-label)', fontWeight: '600' },
    { tag: tags.variableName, color: 'var(--text)' },
    { tag: tags.number, color: 'var(--syntax-literal)' },
    { tag: [tags.string, tags.character], color: 'var(--syntax-string)' },
    { tag: [tags.typeName, tags.modifier], color: 'var(--syntax-directive)' },
    { tag: tags.comment, color: 'var(--syntax-comment)', fontStyle: 'italic' },
    { tag: [tags.operator, tags.bracket], color: 'var(--muted)' },
  ]),
);

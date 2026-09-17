import type { Target } from '$lib/protocol/generated/Target';

const x86Names = [
  'rax',
  'rcx',
  'rdx',
  'rbx',
  'rsp',
  'rbp',
  'rsi',
  'rdi',
  'r8',
  'r9',
  'r10',
  'r11',
  'r12',
  'r13',
  'r14',
  'r15',
];
const aarch64Names = [...Array.from({ length: 31 }, (_, index) => `x${String(index)}`), 'sp'];

/** Canonical GPR names in display order; AArch64 appends SP after X0–X30. */
export function registerNames(target: Target): readonly string[] {
  return target === 'x86_64' ? x86Names : aarch64Names;
}

type RegisterGroup = { kind: '64' | '32' | '16' | '8' | 'pc' | 'flags'; names: readonly string[] };

const extended = Array.from({ length: 8 }, (_, index) => `r${String(index + 8)}`);
const x86: readonly RegisterGroup[] = [
  { kind: '64', names: registerNames('x86_64') },
  {
    kind: '32',
    names: [
      'eax',
      'ecx',
      'edx',
      'ebx',
      'esp',
      'ebp',
      'esi',
      'edi',
      ...extended.map((name) => `${name}d`),
    ],
  },
  {
    kind: '16',
    names: ['ax', 'cx', 'dx', 'bx', 'sp', 'bp', 'si', 'di', ...extended.map((name) => `${name}w`)],
  },
  {
    kind: '8',
    names: [
      'al',
      'cl',
      'dl',
      'bl',
      'spl',
      'bpl',
      'sil',
      'dil',
      ...extended.map((name) => `${name}b`),
      'ah',
      'ch',
      'dh',
      'bh',
    ],
  },
  { kind: 'pc', names: ['rip'] },
  { kind: 'flags', names: ['cf', 'pf', 'af', 'zf', 'sf', 'df', 'of'] },
];
const aarch64: readonly RegisterGroup[] = [
  { kind: '64', names: [...registerNames('aarch64'), 'fp', 'lr'] },
  { kind: '32', names: [...Array.from({ length: 31 }, (_, index) => `w${String(index)}`), 'wsp'] },
  { kind: 'pc', names: ['pc'] },
  { kind: 'flags', names: ['n', 'z', 'c', 'v'] },
];

/** Live edit choices; initial configuration continues to use canonical GPRs. */
export function registerGroups(target: Target): readonly RegisterGroup[] {
  return target === 'x86_64' ? x86 : aarch64;
}

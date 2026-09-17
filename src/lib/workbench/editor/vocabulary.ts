import type { Target } from '$lib/protocol/generated/Target';

const numbered = (prefix: string, count: number) =>
  Array.from({ length: count }, (_, index) => `${prefix}${String(index)}`);

/** Editing suggestions, not a list of instructions supported by the emulator. */
export const registers: Record<Target, string[]> = {
  x86_64: [
    ...'rax rbx rcx rdx rsi rdi rsp rbp rip eax ebx ecx edx esi edi esp ebp ax bx cx dx si di sp bp al bl cl dl ah bh ch dh sil dil spl bpl'.split(
      ' ',
    ),
    ...numbered('r', 16)
      .slice(8)
      .flatMap((name) => [name, `${name}d`, `${name}w`, `${name}b`]),
    ...['xmm', 'ymm', 'zmm'].flatMap((prefix) => numbered(prefix, 32)),
    ...numbered('k', 8),
    ...'cs ds es fs gs ss'.split(' '),
  ],
  aarch64: [
    ...['x', 'w'].flatMap((prefix) => numbered(prefix, 31)),
    ...['v', 'b', 'h', 's', 'd', 'q', 'z'].flatMap((prefix) => numbered(prefix, 32)),
    ...numbered('p', 16),
    ...'sp wsp xzr wzr fp lr nzcv fpcr fpsr ffr'.split(' '),
  ],
};

export const directives =
  '.text .data .rodata .bss .section .pushsection .popsection .global .globl .local .type .size .byte .hword .short .word .long .quad .octa .float .double .ascii .asciz .string .zero .space .skip .balign .p2align .equ .equiv .set .macro .endm .rept .irp .irpc .endr .if .ifdef .ifndef .else .elseif .endif .include .incbin .cfi_startproc .cfi_endproc'.split(
    ' ',
  );

export const instructions: Record<Target, string[]> = {
  x86_64:
    'mov movabs movzx movsx movsxd lea push pop add adc sub sbb imul mul idiv div inc dec neg cmp test and or xor not shl shr sar rol ror bt bts cmove cmovne sete setne jmp je jne jz jnz ja jae jb jbe jg jge jl jle call ret nop endbr64 syscall rep repe repne lock movsb stosb movd movq movdqu movdqa movaps movups movapd movupd paddb paddw paddd paddq pxor pand por addps addpd addss addsd subps subpd mulps mulpd divps divpd sqrtps shufps pshufd cvtsi2ss cvtsi2sd cvtss2si cvtsd2si ldmxcsr stmxcsr vmovd vmovq vmovdqu vmovdqa vmovups vmovaps vpxor vpand vpor vpaddb vpaddw vpaddd vpaddq vpaddusb vpaddusw vpsubb vpsubw vpsubd vpsubq vpsadbw vpshufb vpshufd vpslld vpsrld vpsrldq vpmulld vpmaddwd vpbroadcastb vpbroadcastd vextracti128 vinserti128 vperm2i128 vpermd vptest vzeroupper vaddps vaddpd vsubps vmulps vdivps vsqrtps'.split(
      ' ',
    ),
  aarch64:
    'mov movz movn movk adr adrp add adds adc adcs sub subs sbc sbcs mul madd msub sdiv udiv neg cmp cmn tst and ands orr eor bic lsl lsr asr ror ldr ldrb ldrh ldrsw str strb strh ldp stp ld1 st1 b bl br blr ret b.eq b.ne b.lt b.le b.gt b.ge b.hi b.hs b.lo b.ls b.any b.none b.first b.nfirst b.last b.nlast cbz cbnz tbz tbnz csel cset csinc nop svc mrs msr fmov fadd fsub fmul fdiv fsqrt fcmp fcsel scvtf ucvtf fcvtzs fcvtzu dup ins ext tbl zip1 zip2 uzp1 uzp2 ptrue pfalse whilelo whilelt ptest cntb cnth cntw cntd incb inch incw incd addvl rdvl index ld1b ld1h ld1w ld1d st1b st1h st1w st1d ld4b st4b movprfx sel uqadd sqadd uaddv saddv faddv umin umax smin smax'.split(
      ' ',
    ),
};

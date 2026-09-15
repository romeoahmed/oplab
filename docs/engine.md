# Engine contract

Assembly, decoding, loading and execution are separate capabilities. LLVM accepting
an instruction does not establish decoder coverage or Unicorn execution support.
This document defines the implemented engine; [roadmap](roadmap.md) tracks additions.

## Assembly

LLVM 23 MC compiles a complete source document into relocatable ELF. Matching LLD
links that immutable object at the requested base. `assembly::compile` and
`assembly::link` are independently usable; the worker combines them into a build.
The artifact retains both complete files and a bounded ELF-derived image view.

### Language and layout

Use LLVM's GNU-style language: initially Intel operands on x86_64 and native
LLVM/GNU syntax on little-endian AArch64. Source reaches LLVM unchanged. There is
no NASM/MASM translation, custom preprocessor or per-instruction fallback.
Declare x86 operand syntax explicitly in portable examples:

```asm
.intel_syntax noprefix
.text
mov rax, 40
add rax, 2
done: nop
```

The corresponding AArch64 source is:

```asm
.text
mov x0, #40
add x0, x0, #2
done: nop
```

Link at `0x1000` and choose `done` as the desktop stop symbol. The machine stops
before executing `done`; RAX or X0 is 42. Neither example needs a stack or host ABI.
`ret` would require an explicit return/stack policy that these examples do not supply.

GNU directives, labels, expressions, macros and pseudo-instructions remain toolchain
owned. Use `.byte`, `.quad`, `.macro`/`.endm` and `.rept`/`.endr`.
[GNU Intel syntax](https://sourceware.org/binutils/docs/as/i386_002dVariations.html)
still uses GNU directives. Prefer `.balign N` for bytes or `.p2align N` for powers
of two; [`.align`](https://sourceware.org/binutils/docs/as/Align.html) varies by target.

The base anchors `.text`. Its initial alignment is one byte on x86_64 and four on
AArch64; directives and literal pools may increase it. The requested base must
satisfy the final alignment. No hidden prefix bytes or adjusted address make an
invalid base fit. [`.org`](https://sourceware.org/binutils/docs/as/Org.html) advances
a section offset: `.org 16, 0x42` places the next byte at base + 16, not address 16.
Invalid or backward origins follow LLVM's parser/layout errors.

LLD resolves references and uses its [orphan-section rules](https://lld.llvm.org/ELF/linker_script.html)
for additional sections. Storage following `.text` aligns to the target's maximum
page boundary; source alignment and the requested code address remain unchanged.
The original object retains symbols, sections, relocations and debug information.
Data-only objects, BSS and TLS keep their standard representations, although a
successfully linked image can still require unsupported runtime behavior.

Oplab validates ELF kind, architecture, byte order, section geometry, the `.text`
anchor and load-segment ranges. Undefined symbols and unrepresentable relocations
fail. An exclusive range end may equal `2^64`; emitted bytes may not wrap. ELF is
portable guest data regardless of host OS. Guest instructions run in Unicorn.

### Encodings and diagnostics

There are no optimization passes. LLVM still selects legal encodings, expands
aliases/pseudo-instructions and relaxes assembler branches for final layout.
Optional LLD relaxation is disabled. `mov rax, 42` retains a 64-bit operand;
`movabs rax, 42` explicitly requests the 64-bit immediate form. Textual round trips
need not reproduce original instruction bytes. A future byte-import workflow must
preserve exact bytes directly.

Warnings fail assembly. Diagnostics expose stable categories and an original UTF-8
byte offset when available, with no raw host paths, source excerpts or macro stacks.
An offset is not a complete source map. DWARF is retained, but editor ranges,
macro provenance and source breakpoints are not implemented.

The C++23/CXX adapter owns resources through RAII and call-scoped borrows. MC
finalization runs exactly once, including pools, relaxation, layout, fixups and
DWARF. Calling layout as an extra preflight can mutate state and is prohibited.
CXX translates exceptions; native aborts/hangs require process supervision.

### Job boundaries and limits

The assembler uses an empty virtual filesystem, so `.include` and `.incbin` cannot
read host files. A directive handler rejects `.print`, including quoted spellings,
because stdout carries protocol bytes. LLD uses `--no-dependent-libraries` to prevent
`.deplibs` metadata from causing implicit host-library loading. Source text is not
rewritten or filtered by regular expressions to impose these boundaries.

| Resource                                                   | Bound   |
| ---------------------------------------------------------- | ------- |
| Source UTF-8                                               | 256 KiB |
| Total allocated ELF sections                               | 64 KiB  |
| Each complete object or image file                         | 1 MiB   |
| Total load-segment memory, including padding and zero-fill | 1 MiB   |

Section size/alignment checks occur after normal finalization. Output storage bounds
do not limit internal LLVM allocations or expansion work. Cancellation cannot
interrupt an active LLVM/LLD call. [Supervisor cutoffs](protocol.md#desktop-integration)
provide recovery, not OS-enforced quotas or a complete adversarial sandbox.

## Loading

`LoadPlan::from_elf` validates an image before guest allocation.
`Machine::from_elf` queries Unicorn's page size, applies the plan's permissions,
initializes memory and sets the program counter. A successful assembly and a
successful load are distinct outcomes.

### Accepted runtime subset

The loader accepts little-endian ELF64 `ET_EXEC` for x86_64 or AArch64.
[Program headers](https://gabi.xinuos.com/elf/07-pheader.html) define memory and
permissions. Ordinary section headers and symbols are unnecessary; sectionless ELF
is valid. Extended program-header counts use standard section-zero metadata.
There is no implicit relocation, load bias, stack, return address or OS environment.

The entry must meet target alignment and fit the target's minimum instruction width
inside an executable `PT_LOAD` extent. Padding alone is not a valid entry. This
validates the initial fetch range, not the instruction or eventual termination.
Completion address and instruction budget are validated separately.

Dynamic images/interpreters/linking, TLS, RELRO, GNU processor properties, nonzero
processor flags and unknown program-header types are rejected. A non-executable
`PT_GNU_STACK` without an allocation request is accepted without creating a stack.
Notes, program-header metadata and unwind indexes add no mappings. This is a static
experiment loader, not a general operating-system executable loader.

### Mapping invariants

- `PT_LOAD` entries are ordered by virtual address. Nonempty extents cannot overlap
  or wrap; their exclusive end may equal `2^64`.
- File bytes fit both the input and segment memory. Alignments 0/1 impose no extra
  requirement; larger alignments are powers of two. File/virtual offsets are congruent
  modulo segment alignment and backend page size.
- Mappings cover full backend pages. Only declared file bytes initialize their
  declared addresses; BSS and padding are zero. Unrelated file bytes never leak into
  page padding. A zero-file-size segment does not read its conceptual file offset.
- Adjacent/shared pages coalesce only with identical permissions. Conflicting
  permissions fail instead of being combined. LLD's placement cannot guarantee
  every requested base or unusual section layout will satisfy this policy.
- All ranges and aggregate budgets are checked before allocating initial buffers.
  Zero-fill remains implicit in the retained initial image.

| Loader resource      | Bound                      |
| -------------------- | -------------------------- |
| Input ELF            | 1 MiB                      |
| Program headers      | 256                        |
| Page-rounded regions | 64                         |
| Total mapped memory  | 64 MiB                     |
| Debugger read        | 64 KiB, within one mapping |

These are separate from assembly's tighter emission limits. Host initialization
through `mem_write` does not grant guest write access to RX pages. Debugger reads
can inspect mapped execute-only/guard pages without altering guest permissions.
Unmapped reads fail; they never synthesize zero bytes.

## Execution

A `Session` binds a validated static image, a Unicorn machine and explicit policy.
The native owner is neither `Send`, `Sync` nor cloneable; construct it on its execution
thread. Drops clean up successful and partially initialized native state. Desktop
code accesses it only through the worker.

### Control, completion and accounting

`start` enters Running. The owner polls commands between `advance` calls, each
bounded by 1,024 native dispatches and a cooperative two-millisecond target checked
before instruction admission. Translation or a single dispatch can exceed that
interval. It is not a hard deadline.

`step` may yield in Running while completing a repeated string instruction; the
owner continues advancing until a pause or termination. Pause and cancel take effect
between slices. Pause preserves partial effects, including an interrupted REP.
Terminated sessions require reset before resuming.

Completion occurs at the declared address **before fetch**, even immediately beyond
a mapping. Reaching completion on the last budgeted instruction yields Completed,
not Budget. An unrelated unmapped fetch, guest fault, environment request or native
failure cannot imply completion.

Instruction count means **observed starts**, not retired instructions. A started
instruction may fault. Fetch/decode failures can happen before a start is observed.
A separate checked dispatch counter measures native work. An asynchronous native
timer can stop between a pre-execution hook and effects, so it is not used to infer
exact step/count semantics. Tests verify actual Unicorn effects independently.

REP iterations at the same address with the same bytes belong to one instruction.
They do not consume another instruction budget or retrigger its breakpoint. An
ordinary self-loop is a new start each visit; address alone cannot identify REP.
The decoder classifies control behavior; Unicorn implements instruction effects.

Native exits at completion and executable mapping ends prevent translation
read-ahead from faulting before a valid final instruction executes. At a mapping
end the next slice removes that auxiliary exit and attempts the requested fetch.
Changing exit sets flushes translated blocks. Only a post-dispatch-limit fetch
fault is deferred to the next slice; admitted-instruction data faults remain immediate.
A step can therefore pause at an unmapped destination and fault on the next step
without starting another instruction.

### Breakpoints, observations and reset

Address breakpoints stop before effects. Resume bypasses the stop until admission,
including across zero-work yields, then re-arms for the next architectural visit.
The set is bounded to 256 addresses with AArch64 alignment checks. An address
breakpoint does not establish instruction-boundary or source-location validity.
Source breakpoints and watchpoints remain planned.

Observations capture canonical x86_64 GPRs/RIP/RFLAGS or AArch64 X0–X30/SP/PC/NZCV,
status, counters, fault data and an optional memory window at one owner boundary.
Array order is defined in `oplab-core::registers`. Subregister effects appear in
canonical storage; debugger alias writes are not implemented. Raw flags make no
architectural-definedness claim. Integer observations are not full CPU snapshots.

Faults distinguish unmapped, prohibited and unaligned access, invalid instructions
and processor exceptions, retaining access address/width where available. PC is an
observation, not inferred source provenance. Partial guest effects remain visible.
Infrastructure failure produces Crashed and null registers. Syscall/sysenter,
software environment requests, port I/O and wait/halt terminate as
UnsupportedEnvironment without acquiring host services. This is not an OS ABI or
a promise of arbitrary privileged-instruction support.

Reset constructs a fresh machine from the initial image, restores processor/memory
state, clears execution progress and faults, and retains address breakpoints.
Replacement failure preserves the old machine. Generation advances only on success
and never wraps. A new load receives a new connection-scoped session identity;
commands for stale generations cannot mutate the current machine.

[Protocol](protocol.md) defines asynchronous replies, subscriptions and backpressure.
[Testing](testing.md) defines acceptance; the [roadmap](roadmap.md#release-gates)
tracks containment and distribution work.

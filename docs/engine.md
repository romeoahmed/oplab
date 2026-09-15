# Engine contract

Assembly, instruction inspection, loading and execution are separate capabilities.
LLVM acceptance does not establish decoder coverage or Unicorn execution support.
The [roadmap](roadmap.md) tracks planned additions.

## Assembly

LLVM 23 MC compiles a complete source document into relocatable ELF. Matching LLD
links that immutable object at the requested base. `assembly::compile` and
`assembly::link` are independently usable; the worker combines them into a build.
The artifact retains both complete files and a bounded ELF-derived image view.

### Language and layout

Use LLVM's GNU-style language: Intel operands by default on x86_64 and native
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
Data-only objects, BSS and TLS keep their standard representations. The desktop
address view excludes TLS offsets; complete ELF output retains them. A linked image
can still require unsupported runtime behavior.

Oplab validates ELF kind, architecture, byte order, section geometry, the `.text`
anchor and load-segment ranges. Undefined symbols and unrepresentable relocations
fail. An exclusive range end may equal `2^64`; emitted bytes may not wrap. ELF is
portable guest data regardless of host OS. Guest instructions run in Unicorn.

### Encodings and diagnostics

There are no optimization passes. LLVM still selects legal encodings, expands
aliases/pseudo-instructions and relaxes assembler branches for final layout.
Optional LLD relaxation is disabled. `mov rax, 42` retains a 64-bit operand;
`movabs rax, 42` explicitly requests the 64-bit immediate form. Textual round trips
need not reproduce original instruction bytes. Raw-byte import/export preserves
the original bytes directly.

Warnings fail assembly. Diagnostics expose stable categories and an original UTF-8
byte offset when available, with no raw host paths, source excerpts or macro stacks.
The desktop converts valid offsets to CodeMirror UTF-16 positions, marks the point
and offers navigation. Invalid/missing offsets produce a general error without a
fabricated location. Macro expansion buffers can lack an original-source offset;
link failures have no source point. An offset is not an instruction range or source
map. DWARF is retained; macro provenance and source breakpoints remain planned.

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

## Instruction inspection

The decoder accepts 1–65,536 bytes at an explicit base, with an instruction
limit of 1–4,096. The desktop requests at most 256 instructions per view and advances
by the returned instruction lengths. Addresses cannot wrap; AArch64 starts must be
four-byte aligned. Invalid or incomplete instructions before the limit fail the call
at the reported address. Bytes beyond the instruction limit remain unexamined.

Inputs are imported raw bytes or one [ELF segment](https://gabi.xinuos.com/elf/07-pheader.html)'s
exact `p_filesz` extent, up to 1 MiB. Exports retain the entire extent; decode
requests take at most 64 KiB from the chosen offset. This window exceeds 256
maximum-length instructions on either target, so paging cannot truncate an
instruction before the page limit. The view lists segment permissions and
initially prefers an executable segment. It never concatenates disjoint segments,
synthesizes BSS/padding, infers boundaries from symbols or reconstructs bytes from
formatted text. ELF headers and embedded data remain data even if a decoder
recognizes their bytes as instructions.

Imported bytes use the selected architecture and base; ELF bytes retain their build
identity. Changing bytes, target, base or connection invalidates the visible decode
result, even if the previous values are restored.
These are static file bytes, not a live machine view or verified source mapping.
Select an instruction for a separate, bounded analysis request. Lists remain
lightweight; changing the input or page removes the selection and its pending
result. Locale and panel changes retain the current analysis.

### Static analysis

`decode::analyze` accepts exactly one instruction: 1–15 bytes for x86_64 or four
bytes for AArch64. Trailing bytes, invalid encodings, misalignment and wrapping
input ranges fail. Relative destinations retain the architecture's 64-bit wrapping
arithmetic; they need not lie inside the input range or mapped memory. Analysis
reads no machine state and changes no session.

- x86 uses [iced-x86 instruction information](https://docs.rs/iced-x86/latest/iced_x86/struct.InstructionInfoFactory.html):
  explicit/implicit register accesses, conditional accesses, memory widths,
  control flow, direct destinations, CPUID identifiers and privileged classification.
  Computed, cleared, set and undefined flags remain distinct. Save/restore
  instructions explicitly mark their register list incomplete.
- AArch64 uses [Capstone detail](https://docs.rs/capstone/latest/capstone/struct.InsnDetail.html):
  reported register reads/writes, relative destinations, groups, NZCV updates and
  base writeback. Register names retain backend aliases such as `lr`.
- AArch64 memory operands have **unknown access direction and width**. The current
  operand API has no width, and its access flags can conflate memory effects and
  base writeback: `STR X0,[X1,#8]!` reports ReadWrite. Oplab does not expose that as
  a data read or repair metadata with a handwritten opcode table. Literal loads
  may have no memory operand in this API. An empty list is not proof of no access.

Access lists contain reported data reads/writes, including conditional accesses.
Unknown access is distinct from an absent entry. iced-x86 omits operands classified
as `None` or [`NoMemAccess`](https://docs.rs/iced-x86/latest/iced_x86/enum.OpAccess.html#variant.NoMemAccess)
from its used-access lists; Oplab does not turn them into unknown data accesses.
This classification is backend-specific, not a complete inventory of cache or
translation effects. Memory sizes describe operands, not measured traffic, total
REP traffic or cache-line extents. Address expressions, branch conditions and
effective addresses are not evaluated. A null branch target may mean indirect
control or an unreported destination, not fallthrough.
Aliases, system behavior, partial state and unreported effects prevent treating
this metadata as a complete ISA model.
It does not establish source provenance or retire-time effects.

### Verified capability samples

The analysis suite compares LLVM output with fixed architectural encodings and
checks the available decoder metadata. These are representative instructions,
not blanket extension-support claims. Exact dependency releases remain in lockfiles.

| Guest / sample                                           | Assembly and recognition             | Static metadata                                                              | Execution evidence                                       |
| -------------------------------------------------------- | ------------------------------------ | ---------------------------------------------------------------------------- | -------------------------------------------------------- |
| x86_64 integer arithmetic, branches and stack operations | Verified                             | Registers, memory, flags and control flow                                    | Existing integer/session tests; stack policy is explicit |
| x86_64 SSE2 `PXOR`                                       | Verified                             | SSE2 tag and register effects                                                | Not verified by this matrix                              |
| x86_64 AVX `VADDPS`                                      | Verified                             | AVX tag and register effects                                                 | Not verified by this matrix                              |
| AArch64 integer arithmetic, branches and writeback       | Verified                             | Registers, flags, destinations and writeback; memory direction/width unknown | Existing integer/session tests                           |
| AArch64 Advanced SIMD `ADD`                              | Verified                             | NEON group                                                                   | Not verified by this matrix                              |
| AArch64 crypto `AESE`                                    | Verified with `.arch armv8-a+crypto` | Crypto group                                                                 | Not verified by this matrix                              |
| AArch64 SVE `PTRUE`                                      | Verified with `.arch armv8-a+sve`    | No SVE group returned by the current backend                                 | Not verified by this matrix                              |

CPUID identifiers describe x86 decoder requirements. Capstone groups are a different,
incomplete taxonomy; no groups does not mean no extension is required. Neither
selects an emulator CPU or guarantees execution. Raw-code loading, broader ISA
coverage and an execution capability matrix remain planned.

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

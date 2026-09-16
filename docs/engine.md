# Engine contract

This reference defines assembly syntax, static analysis, loading and execution.
LLVM acceptance, decoder recognition and Unicorn execution support are separate
capabilities. See [development](development.md#native-toolchain) for build setup and
[protocol](protocol.md) for wire and CLI interfaces.

## Assembly

LLVM 23 MC compiles a complete source document into relocatable ELF. Matching LLD
links that immutable object at the requested base. `assembly::compile` and
`assembly::link` are independently usable; the worker combines them into a build.
The artifact retains both complete files and a bounded ELF-derived image view.

### Language and layout

Use LLVM's GNU-style assembly: Intel operands by default on x86_64 and LLVM/GNU
syntax on little-endian AArch64. Source reaches LLVM unchanged. There is
no NASM/MASM translation, custom preprocessor or per-instruction fallback.
The built-in [x86_64](../examples/x86_64.s) and [AArch64](../examples/aarch64.s)
programs copy eight signed integers from `.rodata` to `.bss`, insertion-sort them
and compute their sum. Link at `0x1000` and stop at `done`: RAX or X0 and `total`
contain 42; `output` contains `[-19, -7, 0, 2, 3, 8, 13, 42]` as little-endian
64-bit integers. The first writable segment shows this array in the desktop.

Both programs declare `_start`, function symbols and their own 16-byte-aligned
stack storage. The sort is a leaf function using caller-saved registers and the
integer argument registers of [System V AMD64](https://gitlab.com/x86-psABIs/x86-64-ABI)
or [AAPCS64](https://github.com/ARM-software/abi-aa/blob/main/aapcs64/aapcs64.rst).
These are self-contained guest programs; Oplab does not supply an OS process stack
or an implicit return address. Execution stops before the `done` loop.

x86 explicitly selects `.intel_syntax noprefix`, uses RIP-relative addresses and
copies with `rep movsq`. `offset count` selects a symbolic immediate rather than
a memory operand. AArch64 uses [page-relative relocations](https://sourceware.org/binutils/docs/as/AArch64_002dRelocations.html),
post-indexed loads/stores and scaled register offsets. Array lengths derive from
assembler expressions rather than duplicated numeric constants.

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

The C++23/CXX adapter uses RAII ownership and call-scoped borrows. A guest enum
selects MC; named borrowed paths form one LLD request. Assembly and linking use
separate translation units, with native handles confined to each call. MC
finalization runs exactly once, including pools, relaxation, layout, fixups and
DWARF. Calling layout as an extra preflight can mutate state and is prohibited.
CXX translates initialization exceptions into backend failures; structured assembly
errors remain independent of LLVM's wording. LLD calls are serialized to protect
its process-wide context; each call retains LLD's default internal parallelism.
An unrecoverable `lldMain` result exits through LLD's own cleanup API. Native aborts
and hangs still require process supervision.

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

File inputs are imported raw bytes or one [ELF segment](https://gabi.xinuos.com/elf/07-pheader.html)'s
exact `p_filesz` extent, up to 1 MiB. Exports retain the entire extent; decode
requests take at most 64 KiB from the chosen offset. This window exceeds 256
maximum-length instructions on either target, so paging cannot truncate an
instruction before the page limit. The view lists segment permissions and
initially prefers an executable segment. It never concatenates disjoint segments,
synthesizes BSS/padding, infers boundaries from symbols or reconstructs bytes from
formatted text. ELF headers and embedded data remain data even if a decoder
recognizes their bytes as instructions.

Imported bytes use the Raw code panel's architecture and load address; ELF bytes
retain their build identity. Changing bytes, target, base or connection invalidates
the visible decode result, even if the previous values are restored.
File bytes remain separate from the loaded machine. Captured memory is another
input, bound to its session, generation, address and register snapshot. Controls
without memory retain that capture; reset or replacement invalidates it. Refresh
memory and disassemble again to inspect changed bytes. Neither input provides
verified source mapping.
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
Unreported effects and partial state prevent treating this metadata as a complete
ISA model, source provenance or evidence of retired instructions.

### Verified capability samples

The analysis suite compares LLVM output with fixed architectural encodings and
checks the available decoder metadata. These are representative instructions,
not blanket extension-support claims. Exact dependency releases remain in lockfiles.

| Guest / sample                                           | Assembly and recognition             | Static metadata                                                              | Execution evidence                                       |
| -------------------------------------------------------- | ------------------------------------ | ---------------------------------------------------------------------------- | -------------------------------------------------------- |
| x86_64 integer arithmetic, branches and stack operations | Verified                             | Registers, memory, flags and control flow                                    | Existing integer/session tests; stack policy is explicit |
| x86_64 SSE2 `PXOR`                                       | Verified                             | SSE2 tag and register effects                                                | SIMD initialization/FP-status tests                      |
| x86_64 AVX `VADDPS`                                      | Verified                             | AVX tag and register effects                                                 | Not verified by this matrix                              |
| AArch64 integer arithmetic, branches and writeback       | Verified                             | Registers, flags, destinations and writeback; memory direction/width unknown | Existing integer/session tests                           |
| AArch64 Advanced SIMD `ADD`                              | Verified                             | NEON group                                                                   | Packed lane arithmetic on Cortex-A53/A72                 |
| AArch64 crypto `AESE`                                    | Verified with `.arch armv8-a+crypto` | Crypto group                                                                 | Not verified by this matrix                              |
| AArch64 SVE `PTRUE`                                      | Verified with `.arch armv8-a+sve`    | No SVE group returned by the current backend                                 | Not verified by this matrix                              |

CPUID identifiers describe x86 decoder requirements. Capstone groups are a different,
incomplete taxonomy; no groups does not mean no extension is required. Neither
selects an emulator CPU or guarantees execution. CPU profiles affect execution only;
they do not change LLVM assembly or decoder recognition. Broader ISA coverage remains planned.

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
| Debugger read/write  | 64 KiB, within one mapping |

These are separate from assembly's tighter emission limits. Host initialization
through `mem_write` does not grant guest write access to RX pages. Debugger reads
can inspect mapped execute-only/guard pages without altering guest permissions.
Unmapped reads fail; they never synthesize zero bytes.

### Raw code and initial conditions

`Machine::load` accepts either an ELF image or an explicit `Image::Raw` byte extent.
`LoadPlan::new` validates the complete setup against the backend's page size before
mapping guest memory; `Session::new` binds the resulting machine to execution policy.
ELF convenience constructors without an explicit GPR bank retain backend GPR
defaults. Worker/CLI loads supply a bank and zero unspecified GPRs.

Raw input is 1 byte–1 MiB, mapped RX at its supplied base without relocation or an
ELF wrapper. The base need not be page-aligned. The aligned entry must fit the
minimum instruction width entirely inside the actual input, not page padding.
Pages round outward; surrounding bytes initialize to zero. This validates initial
fetch geometry, not decoding or instruction boundaries. Completion remains explicit.
Desktop, worker and CLI expose raw execution through this same loader. Importing
bytes alone never starts or replaces a session.

`MachineSetup` optionally selects a CPU profile and supplies an architecture-shaped
`InitialRegisters` bank and additional `InitialMapping` regions. Canonical names are
lowercase: the 16 x86 GPRs (including `rsp`), or `x0`–`x30` and separate `sp`.
Unspecified GPRs in an explicit bank are zero. Aliases, duplicate names, PC, flags and wrong-target banks
are rejected.
GPR values are arbitrary unsigned 64-bit bit patterns; a pointer value is not proof
of a mapping or valid alignment for a later guest access. PC comes from the image;
integer flags retain backend defaults. The SIMD environment below is explicit.

Additional regions must be page-aligned, disjoint from each other and all image
pages, and fit the combined 64-region/64-MiB budget. Permissions remain exact; zero
permissions create a guard region. The engine API accepts initial bytes followed
by implicit zero-fill; desktop and CLI additional regions are zero-filled. A mapping
has no implicit ABI role. A stack requires both a region and an explicit RSP/SP value.
ELF addresses, entry and permissions remain authoritative.

Initialization failure drops the new native handle; worker replacement failure
preserves the old session. The complete setup is retained for
[reset](#breakpoints-observations-and-reset). [Live edits](#live-editing) change only
the active machine; they do not alter this retained setup.

## Execution

A `Session` binds a loaded Unicorn machine to explicit execution policy.
The native owner is neither `Send` nor `Sync` and cannot be cloned. Construct it on
its execution thread. Drops clean up successful and partially initialized native state. Desktop
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

### CPU profiles and SIMD

The loader explicitly selects Haswell (default) or Nehalem for x86_64, and
Cortex-A72 (default) or Cortex-A53 for AArch64. A wrong-target profile is rejected
before replacing a session. Reset preserves the resolved profile. These are
Unicorn models, not cycle-accurate processors or promises of complete ISA support.

The application floating-point environment is initialized on load and reset:

- x86 enables CR4.OSFXSR and CR4.OSXMMEXCPT, preserving other CR4 bits, and sets
  MXCSR to `0x1f80`: nearest-even rounding, masked exceptions, no flush-to-zero or
  denormals-are-zero, cleared status. In Unicorn 2.1.5, Nehalem's default CR4 does
  not enable SSE, so relying on native reset defaults is insufficient.
- AArch64 sets CPACR_EL1.FPEN to `0b11` and initializes FPCR/FPSR to zero.
  This enables FP/Advanced SIMD with nearest-even rounding and cleared status.
- Vector storage starts at zero. Guest instructions may alter these controls;
  observations report the resulting raw bits. No host floating-point environment
  is copied into the guest.

The adapter follows [Unicorn's register/control API](https://docs.rs/unicorn-engine/latest/unicorn_engine/struct.Unicorn.html)
and its resolved native implementation. Architectural controls follow the
[Intel SDM](https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html)
and Arm's AArch64 system-register definitions.

The [verification record](roadmap.md#verification) covers profile identity, register
banks, arithmetic, rounding, guest stores and reset. AArch64 reports inexact and
divide-by-zero status in the tested cases.
Unicorn 2.1.5 does not merge accrued SSE exception flags into MXCSR: both register
reads and guest `stmxcsr` omit them. Rounding controls work, but the displayed
status is not reliable floating-point exception history.
Oplab reports the public register API unchanged; it does not inspect private
backend layouts or execute hidden guest instructions to manufacture flags. These
samples do not establish complete SSE/NEON, AVX or SVE support.

### Breakpoints, observations and reset

Address breakpoints stop before effects. Resume bypasses the stop until admission,
including across zero-work yields, then re-arms for the next architectural visit.
The set is bounded to 256 addresses with AArch64 alignment checks. An address
breakpoint does not establish instruction-boundary or source-location validity.
Breakpoints can be edited in ready/paused states. The desktop adds/removes addresses
in the machine panel or toggles decoded rows from captured memory. Successful
observations carry the authoritative sorted set; repeated adds/removals are
idempotent, and rejected changes leave it intact. Source breakpoints and watchpoints
remain planned.

Observations capture the resolved CPU, canonical x86_64 GPRs/RIP/RFLAGS and
XMM0–XMM15/MXCSR, or AArch64 X0–X30/SP/PC/NZCV and V0–V31/FPCR/FPSR, together
with status, counters, faults and optional memory at one owner boundary.
Array order is defined in `oplab-core::registers`. Subregister effects appear in
canonical storage, including effects of live alias writes. Raw flags make no
architectural-definedness claim. SIMD values preserve all 128 raw bits; lane zero
occupies the least-significant bits. x87, AVX upper halves, AVX-512, SVE and other
system state are not captured, so observations are not full CPU snapshots.

Faults distinguish unmapped, prohibited and unaligned access, invalid instructions
and processor exceptions, retaining access address/width where available. PC is an
observation, not inferred source provenance. Partial guest effects remain visible.
An unusable native machine produces Crashed and null registers. Process loss is
reported separately by the supervisor. Syscall/sysenter,
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

### Live editing

`Session::write_register` and `Session::write_memory` accept Ready and Paused
sessions, including breakpoint/step pauses. Running, terminated and crashed
sessions reject writes. Inputs are validated before native mutation. These are
explicit debugger operations, independent of source, artifacts and initial setup.

Register writes accept lowercase names and exact unsigned values that fit the
selected width. Oversized values are rejected, never silently truncated.

| Target  | Writable names                                 | Effect                                                                          |
| ------- | ---------------------------------------------- | ------------------------------------------------------------------------------- |
| x86_64  | rax–r15, including rsp                         | Replace the full 64-bit value                                                   |
| x86_64  | eax–edi, r8d–r15d                              | Replace the low 32 bits and clear the upper 32 bits                             |
| x86_64  | ax–di, r8w–r15w; al–dil, r8b–r15b; ah/ch/dh/bh | Replace only the selected word or byte; high-byte aliases address bits 15:8     |
| AArch64 | x0–x30, sp; fp/lr                              | Replace the full value; fp/lr address X29/X30                                   |
| AArch64 | w0–w30, wsp                                    | Zero-extend the 32-bit value into X0–X30 or SP                                  |
| x86_64  | cf, pf, af, zf, sf, df, of                     | Set one bit to 0 or 1, preserving every other RFLAGS bit                        |
| AArch64 | n, z, c, v                                     | Set one bit to 0 or 1 in NZCV bits 31:28                                        |
| Both    | rip / pc                                       | Replace the next fetch address without executing; A64 requires 4-byte alignment |

These are debugger writes, not execution of MOV or privileged status instructions.
The pure register policy implements architectural operand widths; the native adapter
merges into canonical storage rather than depending on debugger API alias behavior.
See [Intel's architecture manuals](https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html)
and [Arm's NZCV definition](https://developer.arm.com/documentation/ddi0601/latest/AArch64-Registers/NZCV--Condition-Flags).
Whole RFLAGS/NZCV, system registers, EIP/IP and zero-register writes are rejected;
SIMD is not yet editable. Initial setup remains canonical-only to prevent overlapping
assignments from depending on order.

GPR and flag edits preserve interrupted REP continuation and instruction accounting.
Writing PC, **even to its current value**, abandons continuation and rearms address
breakpoints. A paused session becomes an ordinary requested pause; ready stays ready.
The next dispatched instruction is counted as a new start. PC writes do not decode
bytes, require an executable mapping or consume budget: fetch faults and explicit
completion are observed only when execution resumes. Reset restores the image entry
and initial flags.

Memory writes accept 1–65,536 bytes in one mapped region, including RX and guard
pages. They do not change mapping permissions or require guest write access.
Executable writes invalidate overlapping translations through Unicorn's
[`ctl_remove_cache`](https://docs.rs/unicorn-engine/latest/unicorn_engine/struct.Unicorn.html#method.ctl_remove_cache).
The API takes an exclusive `u64` end; a patch ending at `2^64` uses a full cache flush.
Writes occur outside native execution, so no in-hook PC rewrite is needed; see
[Unicorn's cache guidance](https://github.com/unicorn-engine/unicorn/wiki/FAQ#editing-an-instruction-doesnt-take-effecthooks-added-during-emulation-are-not-called).
This is debugger patching, not a general self-modifying-code guarantee.

Writes preserve counters, completion policy, generation and breakpoints. Reset
restores original bytes and initial registers. Validation failures leave the
machine intact. A native write or invalidation failure may have partial effects:
the session becomes Crashed, cannot resume or reset, and must be loaded again.
There is no rollback or automatic mutation retry.

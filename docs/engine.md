# Engine contract

This reference defines assembly syntax, static analysis, loading and execution.
LLVM acceptance, decoder recognition and QEMU/LLVM execution support are separate
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
The bundled [x86_64](../examples/x86_64.s) and [AArch64](../examples/aarch64.s)
programs brighten eight straight-alpha RGBA8 pixels: saturating RGB + 32, unchanged
alpha. AVX2 handles eight pixels together; SVE2 uses a vector-length-agnostic loop
with a predicated tail. Both use read-only input, BSS output and PC-relative
relocations, with no stack or OS services.

Link at `0x1000` and stop before `done`. `output` contains 32 bytes, followed by a
64-bit little-endian `checksum`; RAX/X0 also contains **4814** (`0x12ce`). The first
pixel becomes `[32, 64, 96, 255]`; the third clips to `[255, 255, 255, 64]`. The
initial memory view selects the writable segment. This is a byte transform, not
linear-light exposure or premultiplied-alpha processing.

x86 declares `.intel_syntax noprefix`; AArch64 declares `.arch armv9-a` and needs
SVE2, not SVE2.1. Array lengths use assembler expressions. There is no scalar tail
or intermediate sum buffer.

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
portable guest data regardless of host OS.

### Encodings and diagnostics

Assembly does not run LLVM IR optimization passes. MC selects legal encodings, expands
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

- x86 uses [Intel XED](https://intelxed.github.io/ref-manual/): explicit/implicit
  registers, conditional access, memory width, flags, control flow and an ISA-set
  identifier. Save/restore operations can report incomplete register effects.
- AArch64 uses LLVM MC instruction descriptors and `MCInstrAnalysis`: register
  operands, implicit uses/defs, branch destinations, NZCV and tied-register
  writeback. Memory direction comes from `mayLoad`/`mayStore`; width remains unknown.
  Control-flow groups are not inferred extension requirements.

Metadata is conservative and static. Missing facts do not prove absence of effects;
operand widths do not measure traffic, REP totals or cache-line extents. Effective
addresses and branch conditions are not evaluated. Recognition does not establish
execution support. MAX is bounded by QEMU's implemented TCG features and Oplab's
validated lowering; in particular QEMU 11.1.1 TCG does not provide AVX-512 execution.

## Loading

`LoadPlan::from_elf` validates an image before guest allocation.
`Machine::load` uses 4-KiB guest pages, applies the plan's permissions,
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
through native mapped RAM does not grant guest write access to RX pages. Debugger reads
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

`MachineSetup` supplies an optional architecture-shaped
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

A `Session` binds a loaded QEMU machine to explicit execution policy.
The native owner is neither `Send` nor `Sync` and cannot be cloned. Construct it on
its execution thread; dropping it releases its native state. Desktop code accesses
it only through the worker.

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
A separate checked dispatch counter measures native work. Each translated block
contains one instruction. QEMU's single-step translation limits REP to one iteration;
its resume state distinguishes repeated work from ordinary self-loops. REP consumes
one instruction budget and does not retrigger its breakpoint on each iteration.

Completion and breakpoint policy run before translation. A branch step can pause at
an unmapped destination; the next step reports the fetch fault. Admitted-instruction
data faults remain immediate. Native exceptions stay within C execution frames,
with QEMU's instruction-boundary metadata restoring precise guest state.

### Runtime and SIMD

Each architecture uses its QEMU MAX runtime; there is no separate CPU selection.
MAX is a functional virtual CPU, not a cycle-accurate model or a promise to execute
every published extension.
Guests run at x86 CPL3 or AArch64 EL0 without APIC devices, SMP, an OS or firmware.
The adapter uses QEMU system translators; it does not launch QEMU user-mode executables.

Load and reset initialize x86 long mode at CPL3, enable native SSE/AVX controls and
available XCR0 state, and set MXCSR to `0x1f80`. AArch64 starts at EL0 with native
FP/Advanced SIMD and SVE access enabled; FPCR/FPSR start at zero. Vector storage
starts at zero. No host floating-point environment is copied into the guest.

QEMU owns floating-point helpers, rounding and exception status. The x86 adapter
synchronizes native SoftFloat status into MXCSR before observation. Scalar/packed
arithmetic, all rounding directions, guest stores and reset have independent tests.
AVX2 and predicated SVE lane arithmetic are tested through independent guest
memory results. Observations include full YMM and Z/P/FFR storage. SVE length
comes from QEMU: the current effective length and maximum storage length are
reported separately. These samples do not establish full extension, NaN, denormal
or exception coverage.

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

Observations capture canonical x86_64 GPRs/RIP/RFLAGS and
YMM0–YMM15/MXCSR, or AArch64 X0–X30/SP/PC/NZCV, Z0–Z31, P0–P15, FFR,
current/max vector lengths and FPCR/FPSR, together
with status, counters, faults and optional memory at one owner boundary.
Array order is defined in `oplab-core::registers`. Subregister effects appear in
canonical storage, including effects of live alias writes. Raw flags make no
architectural-definedness claim. SIMD values preserve all 256 YMM bits or all
maximum-length SVE storage (up to 2048 bits per Z register). Lane zero occupies
the least-significant bits. XMM/V views are low 128-bit aliases. P/FFR use one
predicate bit per vector byte; `.h`, `.s` and `.d` views select bits at strides
2, 4 and 8. x87, SME matrix and other system state are not captured, so
observations are not full CPU snapshots.

Faults distinguish unmapped, prohibited and unaligned access, invalid instructions,
unsupported lowering and processor exceptions. Fault PC identifies the instruction
whose translation or execution failed; access address/width are retained when
available. Neither supplies source provenance. Partial effects remain visible.
An unusable native machine produces Crashed and null registers. Process loss is
reported separately by the supervisor. Syscall/sysenter,
software environment requests and architectural wait exits terminate as
UnsupportedEnvironment without acquiring host services. Privileged x86 port I/O
and HLT at CPL3 retain QEMU's processor exception. This is not an OS ABI or
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

`Session::write_register`, `write_vector`, `set_rounding` and `write_memory` accept Ready and Paused
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
Whole RFLAGS/NZCV, unrestricted system-register, EIP/IP and zero-register writes are rejected;
initial setup remains canonical-only to prevent overlapping assignments from depending
on order.

`write_vector` edits YMM0–15 or Z0–31, with XMM0–15 and V0–31 as low
128-bit aliases. Vector writes replace the selected register or one
8/16/32/64/128-bit lane. P0–15 and FFR accept full active predicates or single
bits. Full-width writes require lane zero; other indices must fit the selected
register at the current vector length. Values have the exact byte width of the
write, with unused bits zero for one-bit writes.

The owner reads current native storage and merges before writing. Unselected
lanes, upper alias bits and inactive SVE storage survive. These are raw debugger
edits, not scalar guest instructions that may clear neighboring bits. AArch64
scalar aliases are not write targets. Native storage is little-endian; wire hex
is most-significant first. The desktop offers integer lanes and native IEEE-754
f16/f32/f64 conversion; predicates offer byte/halfword/word/doubleword views.

`set_rounding` changes only MXCSR bits 14:13 or FPCR bits 23:22. Nearest-even,
toward negative infinity, toward positive infinity and toward zero map to their
architectural encodings; x86 and AArch64 swap the two directed encodings.
Other controls and status survive. Instructions with explicit rounding and x87
state are unaffected. Reset restores nearest-even and the zeroed SIMD bank.
The desktop uses JavaScript's standard IEEE-754 conversion for typed float inputs,
independent of guest rounding. Signed zero is preserved; raw hex is required for
exact NaN payloads. Encoding overflow is rejected instead of silently becoming
infinity; explicit Infinity is accepted. Subnormal rounding/underflow follows
`DataView`, with the encoded bits shown before submission.

GPR and flag edits preserve interrupted REP continuation and instruction accounting.
Writing PC, **even to its current value**, abandons continuation and rearms address
breakpoints. A paused session becomes an ordinary requested pause; ready stays ready.
The next dispatched instruction is counted as a new start. PC writes do not decode
bytes, require an executable mapping or consume budget: fetch faults and explicit
completion are observed only when execution resumes. Reset restores the image entry
and initial flags.

Memory writes accept 1–65,536 bytes in one mapped region, including RX and guard
pages. They do not change mapping permissions or require guest write access.
Debugger writes clear the bounded ORC cache; writes to executable mappings also
clear pending REP continuation. Each dispatch re-translates using
current CPU state and bytes before reusing compiled code, so guest-written code
cannot reuse a block solely by PC. Broader self-modifying-code patterns still
require explicit acceptance.

Writes preserve counters, completion policy, generation and breakpoints. Reset
restores original bytes and initial registers. Validation failures leave the
machine intact. A native write or invalidation failure may have partial effects:
the session becomes Crashed, cannot resume or reset, and must be loaded again.
There is no rollback or automatic mutation retry.

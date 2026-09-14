# Engine contract

The language, artifact, loading, and execution boundaries are distinct.
[Testing](testing.md) records verified behavior; [roadmap](roadmap.md) tracks the remaining product and release gates.

## Assembly

Oplab uses LLVM 23 MC and matching LLD to assemble complete source documents into bounded,
relocated artifacts. Assembly support, decoder recognition, and emulator execution
are separate capabilities. The worker exposes assembly, decoding, and bounded execution; source debugging remains under implementation.

### Language

Use LLVM's GNU-style assembly language for the selected target. The initial
profiles are x86_64 with Intel operand syntax and little-endian AArch64. This is a
specific toolchain contract, not a universal assembly standard or a promise of
NASM/MASM compatibility. Source is passed unchanged to LLVM; there is no dialect
translation, custom macro preprocessor, or per-instruction fallback.

For portable x86 examples, declare the operand syntax explicitly:

```asm
.intel_syntax noprefix
.text
start:
    mov rax, 42
    lea rbx, [rip + value]
    ret
.p2align 3
value:
    .quad 42
```

AArch64 uses its native LLVM/GNU operand and directive syntax:

```asm
.text
start:
    adr x1, value
    ldr x0, [x1]
    ret
.p2align 3
value:
    .quad 42
```

Both examples require an eight-byte-aligned artifact base because of `.p2align 3`.
They illustrate assembly layout, not runnable function experiments: execution
also requires a validated stack, return policy, permissions, and instruction support.

GNU Intel mode retains GNU directives; use `.byte`, `.quad`, `.macro`/`.endm`, and
`.rept`/`.endr` rather than silently translating another assembler's spelling.
Standard labels, expressions, macros, and pseudo-instructions are resolved in one
translation unit. Prefer `.balign N` for byte alignment or `.p2align N` for a power
of two. GNU `.align` has target-dependent conventions. See the official
[syntax comparison](https://sourceware.org/binutils/docs/as/i386_002dVariations.html)
and [alignment rules](https://sourceware.org/binutils/docs/as/Align.html).

### Origin and sections

The request's base is the load address of `.text`. Initial section alignment is
one byte for x86_64 and four bytes for AArch64; source directives and target literal
pools can increase it. The base must satisfy the final section alignment. For
example, `nop` can start at x86 address `0x1001`, but a document requiring 16-byte
section alignment cannot. Oplab reports invalid input rather than adding hidden
bytes, rewriting source, or silently changing the requested address.

`.org 16, 0x42` advances the current section offset to 16 and fills the gap with
`0x42`; it does not set the experiment's load address. At base `0x1000`, the next
byte is at `0x1010`. Backward or otherwise invalid origins fail according to LLVM's
parser and layout rules. This follows the GNU
[section-relative origin model](https://sourceware.org/binutils/docs/as/Org.html).

Each build retains the complete relocatable ELF object and final executable ELF
image. The original object preserves sections, symbol attributes, relocations, and
debugging information. LLD resolves references and places additional sections using
its [standard orphan-section rules](https://lld.llvm.org/ELF/linker_script.html); Oplab does not flatten, rename, or discard them.
Data-only translation units are valid. BSS and TLS zero-fill retain their standard
representation rather than becoming fabricated file bytes. A linked image does not
by itself establish a runnable entry point or supported runtime capabilities.

`assembly::compile` produces an object independently of a base address.
`assembly::link` can link that immutable object repeatedly at different bases.
The worker build operation composes both stages. [Machine loading](engine.md#loading)
uses ELF program headers, validates page geometry and permissions, initializes
zero-fill, and rejects unsupported runtime requirements. The link layout aligns
storage following `.text` to the target's maximum page boundary; it does not change
source alignment or the requested code address. General imported object loading
needs its own validation gate.
ELF is an internal portable assembly representation, independent of the host OS.
Guest bytes are never executed as native host code.

MC writes a relocatable object; LLD resolves its references at the requested base.
Oplab checks ELF kind, architecture, endianness, section geometry, and load-segment
file/memory ranges before returning an artifact. The `.text` anchor is checked
without assigning every other section the same base. Symbol attributes and
relocation records are read from ELF rather than copied into a reduced protocol type. Undefined
symbols and unrepresentable relocations fail. An end-exclusive range may equal
`2^64`, but no emitted byte may wrap past the last address.

### Encoding and diagnostics

There are no instruction optimization passes. LLVM still chooses legal encodings,
expands aliases and pseudo-instructions, and relaxes assembler branches to fit the
final layout. Optional LLD relaxation is disabled. `mov rax, 42` preserves its
64-bit operand; use `movabs rax, 42` when the 64-bit immediate encoding is intended.
For existing exact bytes, import the bytes directly instead of expecting a textual
round trip to select the original encoding.

Warnings fail the build. Diagnostics carry stable categories and, when available,
an original UTF-8 source offset. Raw source messages, paths, and macro stacks are
not published. That position is not a full source map. Instruction/data ranges,
expansion provenance, and editor source breakpoints are not advertised yet.

### Job capabilities and limits

Accepting syntax does not grant host capabilities. The assembler uses an empty
virtual filesystem: `.include` and `.incbin` cannot access host files. `.print` is
rejected through LLVM's directive dispatch because it writes host stdout, which
carries the worker protocol. Quoted spellings receive the same treatment. These
restrictions are explicit job boundaries; source text is not filtered with regular
expressions. LLVM dependent-library metadata is retained in the original object,
but [LLD automatic library loading](https://github.com/llvm/llvm-project/blob/llvmorg-23.1.1/lld/ELF/InputFiles.cpp)
is disabled with `--no-dependent-libraries`.
Guest source cannot use `.deplibs` to acquire implicit host filesystem access.

Source is limited to 256 KiB, total allocated section size to 64 KiB, and each
complete object/image file to 1 MiB. Total load-segment memory size, including
segment padding and zero-fill, is bounded independently to 1 MiB. Transport splits
each ELF file into binary chunks of at most 64 KiB. Section size and
alignment are validated after normal object finalization. LLVM finalizes once;
layout is not a read-only preflight. Output storage bounds do not limit LLVM
internal allocations or emission work. These are not a complete CPU/memory sandbox:
the desktop supervisor adds time/RSS cutoffs and worker recovery, while adversarial expansion, OS quotas, and distribution acceptance remain release requirements. See [engine acceptance](testing.md).

## Loading

`LoadPlan::from_elf` validates an image before native guest memory is allocated.
`Machine::from_elf` queries Unicorn's page size, creates that plan, maps its exact
permissions, copies initial bytes, and sets the architecture's program counter.
Both operations belong to `oplab-engine`; desktop code uses the isolated worker.
Loading and [execution sessions](engine.md#execution) are available through the library
and framed worker. The desktop supervisor owns the worker and its lifecycle.

### Supported images

The initial profile accepts little-endian ELF64 `ET_EXEC` images for x86_64 or
AArch64. [Program headers](https://gabi.xinuos.com/elf/07-pheader.html) determine
memory contents and permissions. Section names, section headers, symbols, and
debugging information are unnecessary for loading; a sectionless executable is
valid. Extended program-header counts use the ELF-defined section-zero metadata.
There is no implicit relocation, load bias, stack, return address, or OS environment.

The entry must be aligned for the target and its minimum instruction width must
fit an executable `PT_LOAD` range. Page padding alone is not a valid entry. This
checks the initial fetch range, not instruction validity or eventual termination.
The execution controller must separately validate completion and instruction budgets.

Dynamic images, interpreters, dynamic linking, TLS, RELRO, GNU processor properties,
nonzero processor flags, and unknown program-header types are rejected until their
runtime policies are implemented. Non-executable `PT_GNU_STACK` with no requested
allocation is accepted without creating a stack. Notes, program-header metadata,
and unwind indexes do not create additional mappings. This is a static experiment
loader, not an operating-system process loader or a general executable importer.

### Mapping invariants

`PT_LOAD` entries must be ordered by virtual address. File bytes must fit both
the file and the segment's memory extent. Nonempty virtual extents cannot overlap
or wrap; their exclusive end may equal `2^64`. Alignment values of zero and one
impose no additional segment alignment. Larger alignments must be powers of two.
File offset and virtual address must be congruent modulo both segment alignment
and the selected backend page size.

Each mapping covers whole backend pages. File bytes initialize only their declared
segment addresses; BSS tails and unused page bytes are zero. A segment with no
file bytes needs no read from its conceptual file offset. Unrelated bytes following
`p_filesz` never leak into guest padding. Zero tails remain implicit in the retained
initial image instead of consuming another full host buffer.

Adjacent or shared pages with identical permissions coalesce. Different segment
permissions sharing a page are rejected; permissions are never combined to make
the image fit. ELF segment permissions remain authoritative. The LLVM link layout
aligns storage following `.text` to the target's maximum page boundary. Other
sections retain LLD's orphan placement rules; a requested base or unusual section
layout can still be incompatible with non-overlapping, separately protected pages.
Link success and load success are distinct results.

Input is bounded to 1 MiB and 256 program headers. Page-rounded mappings must fit
64 regions and 64 MiB in total. All segment ranges and the complete allocation
budget are checked before allocating initial-content buffers. These limits are
separate from the tighter assembly emission budget and from native process limits.

### Native ownership and observations

The machine owns one [Unicorn instance](https://docs.rs/unicorn-engine/latest/unicorn_engine/struct.Unicorn.html).
It is neither `Send`, `Sync`, nor cloneable. Construct it inside its eventual
execution thread. Dropping the owner releases the handle and partially initialized
memory after a failed load. Native library types remain private to the adapter.

Host initialization uses `mem_write`; it does not grant guest write access to RX
pages. Bounded debugger reads may inspect mapped execute-only or guard pages without
granting the guest additional permissions. Reads currently fit a single mapping
and are capped at 64 KiB. Unmapped memory remains an error, distinct from mapped zeros.

## Execution

`oplab-engine::session::Session` binds one validated static ELF image to a native
machine and explicit execution policy. It implements synchronous control for an
owning worker thread: start, advance, step, pause, cancel, reset, address breakpoints,
and integer/memory observations. The machine remains neither `Send` nor `Sync`.
Construct the session in its owner rather than moving a native handle into a thread.

The [worker protocol](protocol.md#sessions-and-observations) connects this library
to a dedicated execution thread with bounded commands, session identities, and
ordered observations with optional coalesced subscriptions. Assembly has an independent bounded scheduler.
The desktop workbench connects this path through a process supervisor. Initial register inputs, patching, ABI setup, source debugging, and full project persistence remain required. Worker negotiation advertises the implemented session path.

### Control and completion

`start` enters Running without blocking on a complete experiment. The owner calls
`advance` between command polls. Each call executes at most 1,024 native dispatches,
with a cooperative two-millisecond target checked before admitting an instruction.
Native translation or one dispatch can exceed that interval; it is not a hard
deadline. The desktop supervisor applies separate request/run deadlines and sampled RSS limits.

`step` starts a step and executes one bounded dispatch. It can remain Running after
a cooperative yield or while a repeated string instruction is in progress. The
owner continues calling `advance` until Paused or Terminated. `pause` and `cancel`
apply between slices. A pause preserves effects already made; it does not roll back
an interrupted REP operation. The worker must continue polling commands during steps.

Completion means reaching the declared address before fetch, including an address
immediately outside a mapped code page. A completed final instruction may exactly
consume the instruction budget. The resulting outcome is Completed, not Budget.
An unmapped fetch elsewhere, guest fault, environment request, or unexpected native
exit never implies success. Terminated sessions require reset before execution resumes.

Execution uses native dispatch limits and a code hook for cooperative yields.
An asynchronous native timer can stop after a pre-execution hook but before the
instruction takes effect, so it cannot establish exact step or instruction-count
semantics. The [Unicorn APIs](https://docs.rs/unicorn-engine/2.1.5/unicorn_engine/struct.Unicorn.html)
and the bundled engine's hook behavior are verified with actual guest effects.

The adapter configures native exits at the completion address and executable
mapping ends. This stops translation read-ahead from faulting before a valid final
instruction executes. A slice starting at a mapping end removes that auxiliary exit
so the next requested fetch is attempted normally. Changes to the exit set flush
translated blocks because Unicorn embeds these checks during translation.

When the last allowed dispatch completes a branch, its next fetch may fail before
Unicorn's counter hook can stop execution. Only this post-limit fetch is deferred
to the next slice. A step can therefore pause at an unmapped destination; the next
step faults without starting another instruction. Data faults in the admitted
instruction and fetch failures before admission remain immediate faults.

### Instruction accounting and breakpoints

The instruction budget bounds observed instruction starts. A started instruction
can still fault; the counter is not a retired-instruction count. Fetch/decode failure
can precede an observed start. A separate dispatch counter records native work and
uses checked arithmetic. Native dispatch limits prevent one long REP operation from
monopolizing the owner between command polls.

REP iterations at the same address and with the same instruction bytes belong to
one instruction. They do not consume another instruction budget or re-trigger its
breakpoint. Address and byte identity matter: the controller does not assume that
every consecutive visit to one address is a repeat. Ordinary self-looping branches
are separate instruction starts. The decoder is used only to classify the control
behavior; Unicorn remains responsible for actual instruction effects.

Address breakpoints stop before the selected instruction's effects. Resuming bypasses
that stop until the instruction is admitted, preserving the bypass across a zero-work
yield. It re-arms on a subsequent architectural visit. At most 256 addresses are
retained, with AArch64 instruction alignment enforced. Unmapped or non-boundary
addresses cannot become breakpoints by inventing an instruction or changing memory.
Source resolution and watchpoint timing are separate pending capabilities.

### Observations, faults, and reset

Integer observations expose the complete canonical x86_64 GPR bank plus RIP/RFLAGS,
or X0–X30 plus SP/PC/NZCV on AArch64. Array order follows the architecture and is
documented in `oplab-core::registers`. Subregister instruction writes are observed
through canonical storage. Debugger alias patching still needs its own validation
and acceptance cases. Raw flags do not imply that every bit is architecturally defined.

Faults distinguish unmapped, prohibited, and unaligned reads/writes/fetches, invalid
instructions, and processor exceptions. Memory-hook reports retain the access address
and width when available. The reported PC is an observation, not an inferred source
location. Partial effects remain observable after a guest fault. Infrastructure errors
invalidate the session as Crashed instead of being hidden by another stop request.

Syscall/sysenter, software environment requests, port I/O, and wait/halt instructions
do not acquire host services. They terminate with UnsupportedEnvironment. Exception
vectors follow the selected backend's architecture mapping. This behavior does not
establish support for arbitrary privileged instructions or an operating-system ABI.

Reset constructs a fresh native machine from the retained initial image. It restores
initial memory and processor state, clears progress/fault/step state, and retains
address breakpoints. The old machine remains intact if replacement initialization
fails. Generation advances only after successful replacement; exhaustion is an error.
The worker assigns a distinct connection-scoped identity to each successful load
and rejects commands for obsolete generations. Integer observations are not complete
CPU snapshots or replay records.

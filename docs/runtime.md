# QEMU → LLVM runtime

The runtime uses QEMU **11.1.1** CPU translators and helpers, Rust TCG
lowering and LLVM **23** ORC. LLVM MC/LLD builds standard ELF; Intel XED
**v2026.08.23** and LLVM MC provide independent static inspection.

## Ownership

```text
Guest bytes + QEMU CPU state
  → QEMU target translator
  → owned TCG operation records
  → Rust scalar/vector/helper lowering
  → verified LLVM IR → O2 → ORC code
  → QEMU state, mapped memory and explicit exit
```

`crates/runtime/native` owns QEMU layouts, vCPU work queues, memory regions and
exceptions. Within `crates/runtime`, `src/qemu.rs` loads the private SDK;
`src/jit.rs` and `src/jit/` own LLVM compilation and TCG lowering. Bindgen generates only the private Oplab ABI;
Rust never mirrors `CPUArchState`, TCG structs or helper type encodings. Each guest
uses a private shared library exporting only `oplab_qemu`; libraries remain loaded
for the worker lifetime because QEMU owns process hooks and native threads.
Translation transfers owned temporary/operation arrays to Rust; `Block::drop` releases
them through the SDK. Operation names and helper addresses have SDK lifetime.

Each runtime owns an LLVM JIT and a bounded cache of 128 compiled instructions.
Scalar, vector and native-call lowering have separate modules. Addressable local
temporaries are promoted by LLVM's standard pipeline; CPU globals remain explicit
loads/stores. Native QEMU SoftFloat and memory helpers retain their semantics.
Unsupported TCG operations return an explicit fault.
Lowering currently requires a 64-bit little-endian host. Both Rust and the native
adapter reject other layouts at compile time. Vector broadcasts use LLVM shuffles;
scalar shifts mask counts so TCG's unspecified results do not become LLVM poison.

The approach follows [rev.ng's lifting model](https://docs.rev.ng/developer-manual/qemu-helpers/),
but retains QEMU CPU state and native helpers for execution. Generated IR includes
calls to native helper addresses, so it is process-local executable IR, not a
self-contained bitcode export. rev.ng is not a dependency; helper bitcode/inlining
remains future optimization work.

## Native lifecycle and faults

The adapter uses QOM type registration and instance initialization/finalization,
non-migrating RAM, native vCPU threads and `run_on_cpu`. A per-library lock serializes
QEMU operations. The vCPU owns instruction execution; QEMU's BQL and RCU protect
internal readers and retirement. Host callers use call-scoped RCU registration so
Rust thread teardown cannot leave QEMU registry entries pointing at released TLS.
`adapter.c` owns this lifecycle and the ABI table; `translate.c` owns TCG export,
dispatch and exception recovery. Memory helpers and architectural state each have
their own source file. The export uses QEMU's operation count and checks ABI array
capacities against its headers at compile time.

CPU teardown unregisters the x86 machine-init notifier and clears strong memory
links before owner reclamation. The guests execute unprivileged snippets through
QEMU system translators; they do not create APIC devices or an SMP machine. Native
QEMU APIs and CPU properties provide this behavior without upstream lifecycle or
instruction patches.

Guest memory uses exact 4-KiB mappings, permissions and bounded buffers. Native
MMU helpers retain endianness, access width, alignment and atomic operations.
The private SDK uses R=1, W=2, X=4; the engine converts ELF permission bits at
the boundary. Debugger access is independent of guest permissions. C `sigsetjmp`
and QEMU exit boundaries contain native exceptions below Rust frames;
instruction-start metadata restores faulting CPU state. This experiment memory
model does not implement an OS page table, firmware or syscall environment.

`invalid_instruction` preserves QEMU's invalid/undefined-instruction outcome;
it does not prove malformed bytes, because CPU features and execution state can
also make an encoding unavailable. `unsupported_instruction` instead means that
translation produced an operation Oplab cannot lower. Neither category is inferred
from static decoder recognition.

Single-instruction blocks preserve debugger boundaries. QEMU's single-step
translation bounds each REP iteration; its native resume state identifies pending
work. Breakpoints, completion and instruction budgets remain engine policy.
Cache identity includes PC, code-segment base, translation flags, size and exact
instruction bytes. Every dispatch translates current state before cache lookup;
this reuses LLVM compilation, not QEMU translation. Debugger writes clear the cache.

See [QEMU's TCG operations](https://www.qemu.org/docs/master/devel/tcg-ops.html),
[QOM ownership](https://www.qemu.org/docs/master/devel/qom.html),
[RCU lifecycle](https://www.qemu.org/docs/master/devel/rcu.html) and
[LLVM IR semantics](https://llvm.org/docs/LangRef.html). ORC's thread-safe module
and resource trackers own compiled code and its context. Selected QEMU sources and
headers are authoritative; this is an internal integration, not a stable upstream
embedding API.

## SDK construction

`cargo xtask sdk` delegates to QEMU configure/Meson/Ninja and Intel mbuild in owned,
release-tag checkouts. Integration adds a shared-library target and enables the
ARM GICv5 CPU-interface dependency; translators, helpers and SoftFloat stay unchanged.
[Development](development.md#qemu-and-xed-sdk) owns setup and rebuild instructions.
The private ABI stays at version 1; rebuild libraries, worker and desktop together.

## Coverage and limits

Each architecture selects one fixed MAX runtime, bounded by QEMU's TCG features,
Oplab's implemented lowering and verified behavior. QEMU 11.1.1 TCG has AVX2,
but does not provide AVX-512 execution. AArch64 enables FP/NEON and SVE access
using native architectural controls. Broader instruction samples must be verified,
not inferred from decoder recognition or feature names.

Tests cover integer/flags, ELF/raw loading, memory faults, REP interruption,
self-modifying code, live edits, reset, floating-point rounding/status, AVX2 and
predicated SVE lanes. Bundled programs independently check saturated RGBA bytes
and their checksum. Full YMM/Z and P/FFR reads and edits are checked through
independent guest stores, alias preservation and reset. The adapter uses QEMU
vector storage and effective SVE-length helpers directly. The
[roadmap](roadmap.md#verification) records acceptance and platform limits.

## Distribution design

Planned release linkage (not yet independently installed and verified):

| Component                              | Intended delivery                                     |
| -------------------------------------- | ----------------------------------------------------- |
| Rust, LLVM MC/LLD/ORC and XED          | One statically linked worker                          |
| QEMU guest adapters                    | Two private shared libraries, one exported entry each |
| Non-system dynamic dependencies        | Private bundled dependency closure                    |
| System runtime, frameworks and WebView | Declared platform prerequisites                       |

Current staging copies the worker and QEMU libraries using Tauri's
[sidecar](https://v2.tauri.app/develop/sidecar/) and
[resource](https://v2.tauri.app/develop/resources/) lifecycles. The shell resolves
its resource directory and passes it to the worker, independently of the working
directory or inherited SDK override. It does not link LLVM itself.

Development builds still use shared LLVM and SDK-installed GLib dependencies.
Release-static LLVM, complete dependency relocation, signatures/JIT entitlements,
Linux baseline and Windows DLL lookup require installed-app acceptance. A local
debug bundle does not establish self-contained distribution. LLVM bindings and
C++ adapters share llvm-sys discovery and linkage; LLD must match that LLVM release.
[Development](development.md#discovery-and-overrides) owns the environment settings.

The [distribution license review](development.md#distribution-licensing) must cover
the compiled dependency graph, notices and source/build obligations. Dynamic
linking alone does not establish compatibility.

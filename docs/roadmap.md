# Roadmap

Oplab supports desktop assembly, execution and inspection. It is not yet a complete
debugger or an independently distributable application. This inventory separates
implemented scope from planned work; [testing](testing.md) defines acceptance and
[development](development.md) owns setup.

## Runtime status

QEMU 11.1.1 supplies translation and CPU semantics; Rust lowers TCG to LLVM 23 ORC.
Intel XED v2026.08.23 and LLVM MC provide independent static inspection. Each guest
uses fixed MAX state. AVX2 and SVE/SVE2 samples execute; AVX-512 is unavailable.
Full YMM/Z/P/FFR observation and editing, low aliases and native SVE lengths are
implemented. Register coverage does not establish complete instruction coverage.

SDK construction and worker/QEMU-library staging are implemented. Release-static
LLVM, transitive dependency relocation and installed distribution remain open.
See [runtime](runtime.md) for boundaries and [development](development.md) for setup.

## Implemented

| Area          | Available behavior                                                                                                                                                         |
| ------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Assembly      | LLVM 23 MC/LLD through CXX/C++23; unchanged GNU-style source, separate compile/link APIs and complete ELF object/image output                                              |
| Loading       | Validated static ELF64 program headers, entry, permissions, BSS and padding; raw-code loading in desktop/worker/CLI                                                        |
| Execution     | Both guests; step/run/pause/stop/reset, explicit completion and budgets, managed address breakpoints, integer/SIMD/flags/memory observations and fault outcomes            |
| Initial state | Canonical GPRs and extra zero-filled mappings in desktop/worker/CLI; shared validation, failed-load preservation and reset to retained setup                               |
| Live editing  | Ready/paused GPR/alias/PC/flag writes, full YMM/Z/P/FFR and lane edits, FP rounding and memory patches (4 KiB desktop, 64 KiB engine/worker); reset restores initial state |
| Inspection    | Bounded raw/ELF-segment/observed-memory disassembly and on-demand instruction metadata in desktop/worker/CLI; static recognition remains distinct from execution support   |
| Editor        | GNU-aware CodeMirror/Lezer highlighting and completion, block folding, literal-label navigation, search/history/comments and build-scoped Unicode diagnostics              |
| Workbench     | Separate document/artifact/session state, stale-result rejection, blank first launch, scratch recovery, configuration, focus mode and collapsible/adjustable panels        |
| Files         | Native UTF-8 source and exact-byte import/export, complete ELF exports, bounded I/O and atomic replacement with paths kept native                                          |
| CLI           | Source/ELF/raw execution, explicit completion and instruction/time limits, final JSON registers/faults and optional bounded memory                                         |
| Delivery      | Bounded framing, correlated requests, assembly coalescing/cancellation, reserved output, full/delta observations and orderly shutdown                                      |
| Supervision   | View leases, deadlines/RSS monitoring, uncertain outcomes, bounded WebView delivery, kill/reap and explicit worker recovery                                                |
| Presentation  | English/Simplified Chinese, self-hosted/system monospace, native CSS, Bits UI, Lucide and application icon assets                                                          |
| Tooling       | Workspace dependencies/lints, clap CLI/xtask, native build lifecycles, strict checks, contract generation and isolated behavior/property tests                             |

## Next product work

1. **Source provenance:** derive verified MC/DWARF source/instruction relationships,
   including macros, duplicate locations, Unicode, data and padding. Diagnostic
   points are implemented; they are not instruction provenance.
2. **Execution coverage:** extend the tested guest/extension matrix beyond static
   recognition. Full AVX2/SVE register inspection/editing and directed rounding
   are implemented; broader instruction, exception and vector-length-transition
   coverage remains. AVX-512, x87 and SME matrix views are outside current scope.
3. **Workbench expansion:** multiple source documents, coordinated source/instruction
   focus, accessible draggable separators and large-data views as needed. Source-only
   label navigation and GNU block folding are implemented.

Files use standard assembly text, raw bytes and ELF; no saved-experiment container
is planned. Local scratch/settings recovery remains. Importing a file never
implicitly loads or patches a machine.

## Later capabilities

- [ ] Source breakpoints, watchpoints and explicit before/after timing.
- [ ] SysV AMD64, Windows x64 and AAPCS64 integer/buffer function experiments with
      declared stack and return policies.
- [ ] Assertions and bounded traces with visible truncation; static effects remain
      distinct from observed execution.
- [ ] Broader undefined-flag, self-modifying-code and instruction coverage.
- [ ] Complete snapshots/replay and optional static performance analysis, each
      with explicit semantics and acceptance gates.

## Release gates

- [ ] Complete-workflow state-machine properties, parser fuzzing and adversarial
      native resource/expansion tests.
- [ ] Measured assembly, execution, IPC, rendering, startup and memory workloads.
- [ ] Windows/Linux/macOS CI, both guests, coherent native toolchains and declared
      host/WebView minimums; static/cross/universal builds where supported.
- [ ] Native WebDriver workbench flows with real IPC and worker, including failure,
      stale edits, reset and source/binary import/export. Automation stays out of production.
- [ ] Broader keyboard/screen-reader, zoom and real-WebView acceptance using native
      semantics and library primitives.
- [ ] Complete dependency-license compatibility review, including QEMU, XED and
      LLVM/LLD; packaged dependency/font licenses and notices.
- [ ] Transitive native-library bundling/discovery, signing/JIT policy, installation
      and sidecar startup on each supported host.
- [ ] Production CSP/capabilities, privacy and failure-artifact review.

Add dependencies and modules when implementing a capability, not as placeholders.

## Verification

Recorded on **Apple Silicon macOS, 2026-09-17**. Evidence below comes from distinct
runs; browser fixtures, native tests and desktop acceptance establish different guarantees.

| Verification               | Recorded result                                                                                                                                 |
| -------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| SDK                        | QEMU 11.1.1, XED/mbuild v2026.08.23 and GNU C23 adapter built with LLVM/LLD 23.1.1                                                              |
| Workspace checks           | `cargo xtask check`: Rust/C/C++ and frontend lints, types, 49 generated declarations and formatting passed                                      |
| Automated tests            | 142 Rust tests (including doctests) and 120 frontend tests passed in separate workspace/frontend runs                                           |
| Native execution           | Both guests: loading, flags, memory faults, REP, cache invalidation, edits/reset, analysis, CLI and worker transport                            |
| SIMD                       | Full YMM/Z/P/FFR, highest lanes, alias/inactive-bit preservation, rounding, independent guest stores and full-bank delivery                     |
| Contract/interaction tests | Mixed decode boundaries, overlapping edits, all SVE view lengths, half-precision conversion, retained baselines and duplicate-submit prevention |

Documentation verification on the same date passed warnings-denied workspace rustdoc,
both runnable doctests, SDK rebuild and `cargo xtask check`. All 79 local Markdown
links and heading anchors resolved. This comments/documentation pass did not rerun
the full behavior suites or desktop acceptance.

Both bundled programs match scalar RGBA results and checksum 4814 at two link
addresses and after reset. The SVE2 loop also passed 1, 3, 7, 8, 63, 64, 65, 127
and 129 pixels at both addresses with MAX's 256-byte vector length. It does not
require SVE2.1. Synthetic length properties do not verify guest-driven length transitions.

Fresh static debug WKWebView acceptance covered both examples, full SIMD views,
Float16 writes, FFR's highest bit, reset and architecture changes in both languages.
Search/replacement, undo, focus mode, appearance and register disclosure were also
verified. Earlier native acceptance covered analysis, memory/PC inspection, patches,
source/binary I/O and keyboard assembly/step/run.

Browser visual checks covered 1440×900, 880×600 and 414px widths, both languages,
long-vector previews and matching search/line-navigation overlays. These sizes
were not all repeated for the expanded SIMD bank; fixture rendering establishes
layout, not native execution.

AddressSanitizer verified x86 notifier-lifetime and APIC-index fixes; 44 runtime/engine
tests passed with the instrumented SDK. Process-lifetime QEMU globals were excluded
from leak detection. Unmodified QEMU's coroutine suite reproduced macOS
`__asan_handle_no_return` warnings while all 13 tests passed: its `sigaltstack`
backend lacks ASan fiber-switch notifications. Coroutine-stack sanitizer evidence
is therefore limited. The latest SIMD changes have not repeated that run.

The current Vitest/Vite combination warns that the mocks interceptor's
`configureServer` hook is ignored. Explicit test ports pass without it; the warning
remains visible. Recheck compatibility before depending on that facility.

Windows/Linux, release-static/cross/universal builds and signed, independently
installed packages remain unverified. No desktop WebDriver harness is installed.
[Testing](testing.md) defines acceptance; the release gates above remain open.

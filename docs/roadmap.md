# Roadmap

Oplab supports desktop assembly, execution and inspection. It is not yet a complete
debugger or an independently distributable application. This inventory separates
implemented scope from planned work; [testing](testing.md) defines acceptance and
[development](development.md) owns setup.

## Implemented

| Area          | Available behavior                                                                                                                                                       |
| ------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Assembly      | LLVM 23 MC/LLD through CXX/C++23; unchanged GNU-style source, separate compile/link APIs and complete ELF object/image output                                            |
| Loading       | Validated static ELF64 program headers, entry, permissions, BSS and padding; raw-code loading in desktop/worker/CLI                                                      |
| Execution     | Both guests; step/run/pause/stop/reset, explicit completion and budgets, managed address breakpoints, integer/flags/memory observations and fault outcomes               |
| Initial state | Canonical GPRs and extra zero-filled mappings in desktop/worker/CLI; shared validation, failed-load preservation and reset to retained setup                             |
| Live editing  | Ready/paused canonical GPR writes and memory patches (4 KiB desktop, 64 KiB engine/worker), executable cache invalidation, unchanged permissions and reset               |
| Inspection    | Bounded raw/ELF-segment/observed-memory disassembly and on-demand instruction metadata in desktop/worker/CLI; static recognition remains distinct from execution support |
| Editor        | CodeMirror/Lezer lexical assistance, completion, search/history/comments and build-scoped diagnostics with verified Unicode positions                                    |
| Workbench     | Separate document/artifact/session state, stale-result rejection, local scratch recovery, configuration, focus mode and adjustable panels                                |
| Files         | Native UTF-8 source and exact-byte import/export, complete ELF exports, bounded I/O and atomic replacement with paths kept native                                        |
| CLI           | Source/ELF/raw execution, explicit completion and instruction/time limits, final JSON registers/faults and optional bounded memory                                       |
| Delivery      | Bounded framing, correlated requests, assembly coalescing/cancellation, reserved output, full/delta observations and orderly shutdown                                    |
| Supervision   | View leases, deadlines/RSS monitoring, uncertain outcomes, bounded WebView delivery, kill/reap and explicit worker recovery                                              |
| Presentation  | English/Simplified Chinese, self-hosted/system monospace, native CSS, Bits UI, Lucide and application icon assets                                                        |
| Tooling       | Workspace dependencies/lints, clap CLI/xtask, native build lifecycles, strict checks, contract generation and isolated behavior/property tests                           |

## Next product work

1. **Source provenance:** derive verified MC/DWARF source/instruction relationships,
   including macros, duplicate locations, Unicode, data and padding. Diagnostic
   points are implemented; they are not instruction provenance.
2. **Execution coverage:** extend the tested guest/extension matrix beyond static
   recognition; CPU selection, SIMD observations and precise unsupported behavior
   need explicit contracts.
3. **Extended machine editing:** register aliases with architectural width/zero-extension
   rules and an explicit PC/flags write policy. SIMD state belongs with execution coverage.
4. **Workbench expansion:** multiple source documents, coordinated source/instruction
   focus, accessible draggable separators and large-data views as needed.

Files use standard assembly text, raw bytes and ELF; no saved-experiment container
is planned. Local scratch/settings recovery remains. Importing a file never
implicitly loads or patches a machine.

## Later capabilities

- [ ] Source breakpoints, watchpoints and explicit before/after timing.
- [ ] SysV AMD64, Windows x64 and AAPCS64 integer/buffer function experiments with
      declared stack and return policies.
- [ ] Assertions and bounded traces with visible truncation; static effects remain
      distinct from observed execution.
- [ ] Verified undefined flags, self-modifying code and broader instruction coverage.
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
- [ ] Transitive native-library bundling/discovery, dependency/font notices,
      signing/JIT policy, installation and sidecar startup on each supported host.
- [ ] Production CSP/capabilities, privacy and failure-artifact review.

Add dependencies and modules when implementing a capability, not as placeholders.

## Verification

Evidence is limited to Apple Silicon macOS development. Windows/Linux builds and
signed, independently installed distributions remain unverified.

Recorded verification through **2026-09-16**:

| Evidence                   | Result                                                                                                                                                                                                       |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Workspace checks           | `cargo xtask check`; strict Rust/C++/frontend checks, formatting and 46 generated TypeScript declarations verified                                                                                           |
| Automated tests            | `cargo xtask test`: 109 Rust unit/integration tests, 2 doctests and 72 frontend tests, including 42 Chromium cases                                                                                           |
| Native bridge/build        | LLVM/LLD 23.1.1 and CXX 1.0.202: parallel C++ builds, standalone bridge header, both guests, arbitrary ELF data preservation, paths with spaces and target-specific overrides verified                       |
| Rust documentation         | Rustdoc passed with warnings denied                                                                                                                                                                          |
| Live-editing acceptance    | Both guests in the fresh macOS static bundle: GPR write, code patch, execution to 42 and reset to original values/bytes; English and Simplified Chinese, keyboard submission and native hex parsing verified |
| Memory inspection          | Fresh macOS static bundle: invalid/unmapped addresses and empty/oversized lengths preserved captured memory; corrected input cleared errors. Both languages, Enter and Inspect at PC verified                |
| Native setup and execution | Both guests consumed full-width GPR inputs and explicit mappings; reset restored initial values, and invalid replacement preserved the existing machine                                                      |
| Raw-code desktop           | Both guests imported through native dialogs, stopped before effects, resumed/stepped to 42 and reset; failed entry preserved the machine; observed-memory PC and breakpoint toggles verified                 |
| Desktop bundle             | Normal Tauri build hooks; both guests assembled, loaded, stepped and stored 42; reset, failed assembly and bounded pause/stop flows exercised                                                                |
| Bundled examples           | Both signed-array sorting programs ran in the static desktop bundle; native tests verified sorted output, sum 42, unchanged input and reset at two link addresses                                            |
| Files and diagnostics      | Unicode diagnostic navigation, exact source/raw-byte transfers and a 126,980-byte ELF segment exported and decoded through its end                                                                           |
| Interface                  | Compact command header, 1440×900 default and 40% observation panel; responsive layouts at 1280, 880, 800 and 414px; both languages and native breakpoint spacing verified                                    |
| Icon conversion            | All 10 ICNS representations and 6 ICO layers matched PNG references on 2026-09-14                                                                                                                            |

These are recorded results, not a substitute for checks after later changes.
[Testing](testing.md) defines coverage, acceptance procedures and the known Vitest/Vite
mock-hook warning. No desktop WebDriver harness is installed.

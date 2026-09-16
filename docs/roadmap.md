# Roadmap

Oplab supports desktop assembly, execution and inspection. It is not yet a complete
debugger or an independently distributable application. This inventory separates
implemented scope from planned work; [testing](testing.md) defines acceptance and
[development](development.md) owns setup.

## Implemented

| Area          | Available behavior                                                                                                                                                               |
| ------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Assembly      | LLVM 23 MC/LLD through CXX/C++23; unchanged GNU-style source, separate compile/link APIs and complete ELF object/image output                                                    |
| Loading       | Validated static ELF64 program headers, entry, permissions, BSS and padding; raw-code loading in desktop/worker/CLI                                                              |
| Execution     | Both guests; step/run/pause/stop/reset, explicit completion and budgets, managed address breakpoints, integer/SIMD/flags/memory observations and fault outcomes                  |
| Initial state | CPU profiles, canonical GPRs and extra zero-filled mappings in desktop/worker/CLI; shared validation, failed-load preservation and reset to retained setup                       |
| Live editing  | Ready/paused GPR and subregister writes, RIP/PC redirection, individual application flags and memory patches (4 KiB desktop, 64 KiB engine/worker); reset restores initial state |
| Inspection    | Bounded raw/ELF-segment/observed-memory disassembly and on-demand instruction metadata in desktop/worker/CLI; static recognition remains distinct from execution support         |
| Editor        | CodeMirror/Lezer lexical assistance, completion, search/history/comments and build-scoped diagnostics with verified Unicode positions                                            |
| Workbench     | Separate document/artifact/session state, stale-result rejection, local scratch recovery, configuration, focus mode and adjustable panels                                        |
| Files         | Native UTF-8 source and exact-byte import/export, complete ELF exports, bounded I/O and atomic replacement with paths kept native                                                |
| CLI           | Source/ELF/raw execution, explicit completion and instruction/time limits, final JSON registers/faults and optional bounded memory                                               |
| Delivery      | Bounded framing, correlated requests, assembly coalescing/cancellation, reserved output, full/delta observations and orderly shutdown                                            |
| Supervision   | View leases, deadlines/RSS monitoring, uncertain outcomes, bounded WebView delivery, kill/reap and explicit worker recovery                                                      |
| Presentation  | English/Simplified Chinese, self-hosted/system monospace, native CSS, Bits UI, Lucide and application icon assets                                                                |
| Tooling       | Workspace dependencies/lints, clap CLI/xtask, native build lifecycles, strict checks, contract generation and isolated behavior/property tests                                   |

## Next product work

1. **Source provenance:** derive verified MC/DWARF source/instruction relationships,
   including macros, duplicate locations, Unicode, data and padding. Diagnostic
   points are implemented; they are not instruction provenance.
2. **Execution coverage:** extend the tested guest/extension matrix beyond static
   recognition; CPU profiles and 128-bit SIMD observations are implemented. Broader
   extension coverage, vector editing and precise unsupported behavior remain.
3. **Workbench expansion:** multiple source documents, coordinated source/instruction
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
- [ ] Complete dependency-license compatibility review, including Unicorn and
      LLVM/LLD; packaged dependency/font licenses and notices.
- [ ] Transitive native-library bundling/discovery, signing/JIT policy, installation
      and sidecar startup on each supported host.
- [ ] Production CSP/capabilities, privacy and failure-artifact review.

Add dependencies and modules when implementing a capability, not as placeholders.

## Verification

Recorded through **2026-09-16** on Apple Silicon macOS. These results describe
development builds; Windows/Linux and signed, independently installed distributions
remain unverified.

| Area                   | Evidence                                                                                                                                                                                                                                                       |
| ---------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Workspace              | `cargo xtask check`: strict Rust/C++/frontend checks, formatting and 48 generated TypeScript declarations; Rustdoc passed with warnings denied                                                                                                                 |
| Tests                  | `cargo xtask test`: 125 Rust unit/integration tests, 2 doctests and 82 frontend tests, including 48 Chromium cases                                                                                                                                             |
| CPU and SIMD           | All four profiles: CPU identity, every XMM/V register, packed arithmetic, rounding, initial controls, guest stores and reset; exact wire values, stream replacement/clearing and lane properties                                                               |
| Native SIMD views      | Fresh macOS bundle: Haswell/Cortex-A53 packed additions and stored 42s, both guests' f32 special values, AArch64 f64 lanes, lane numbering, retained format and reset                                                                                          |
| Build and bridge       | LLVM/LLD 23.1.1 and CXX 1.0.202: parallel builds, standalone bridge header, both guests, arbitrary ELF data, paths with spaces and target-specific overrides; normal Tauri bundle hooks                                                                        |
| Loading and execution  | Native desktop source/raw loading, explicit setup, breakpoints, step/run/pause/stop and failed-load preservation; both sorting examples return 42 with sorted memory and unchanged input; native tests also cover two link addresses                           |
| Live editing           | Both guests: native alias/MOV equivalence, flags, PC/REP/breakpoint restart and stale-write rejection. Static desktop: GPR writes, code patches and reset on both guests; x86 AH/EAX, ZF/SETZ and PC redirection                                               |
| Files and inspection   | Exact source/raw-byte transfers, Unicode diagnostic navigation and a 126,980-byte ELF segment decoded through its end; invalid memory inputs preserve prior captures, corrected inputs recover                                                                 |
| Interface and recovery | Responsive layouts at 1280, 880, 800 and 414px, including bilingual browser checks; native keyboard tabs, sticky tab strip, retained input/format and popover placement. Clean native startup after cache/storage reset, both bundled guests and AArch64 reset |
| Icon assets            | All 10 ICNS representations and 6 ICO layers matched PNG references on 2026-09-14                                                                                                                                                                              |

Interactive AArch64 subregister, flag and PC-editing acceptance is still incomplete;
full-width writes, bundled-program flags and reset have passed in the native app.
No desktop WebDriver harness is installed. [Testing](testing.md) defines acceptance
procedures and records the known Vitest/Vite mock-hook warning. Rerun the relevant
checks when behavior changes; this record does not replace them.

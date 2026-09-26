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

| Area             | Available behavior                                                                                                                                                                        |
| ---------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Assembly         | LLVM 23 MC/LLD through CXX/C++23; unchanged GNU-style source, separate compile/link APIs and complete ELF object/image output                                                             |
| Loading          | Validated static ELF64 program headers, entry, permissions, BSS and padding; raw-code loading in desktop/worker/CLI                                                                       |
| Execution        | Both guests; step/run/pause/stop/reset, explicit completion and budgets, managed address/data breakpoints, temporary run targets, call step-over, bounded opt-in trace and fault outcomes |
| Initial state    | Canonical GPRs and extra zero-filled mappings in desktop/worker/CLI; shared validation, failed-load preservation and reset to retained setup                                              |
| Live editing     | Ready/paused GPR/alias/PC/flag writes, full YMM/Z/P/FFR and lane edits, FP rounding and memory patches (4 KiB desktop, 64 KiB engine/worker); reset restores initial state                |
| Inspection       | Bounded raw/ELF-segment/observed-memory disassembly and on-demand instruction metadata in desktop/worker/CLI; static recognition remains distinct from execution support                  |
| Editor           | GNU-aware CodeMirror/Lezer highlighting and completion, block folding, literal-label navigation, search/history/comments and build-scoped Unicode diagnostics                             |
| Source debugging | Build-scoped DWARF points, bidirectional source/instruction navigation, current execution line, atomic grouped source breakpoints and run-to-cursor                                       |
| Workbench        | Source tabs/list with independent editor/build state, quota-aware draft recovery, explicit machine provenance, draggable/keyboard-adjustable panels and focus mode                        |
| Files            | Native UTF-8 source and exact-byte import/export, complete ELF exports, bounded I/O and atomic replacement with paths kept native                                                         |
| CLI              | Source/ELF/raw execution, explicit completion and instruction/time limits, final JSON registers/faults and optional bounded memory                                                        |
| Delivery         | Bounded framing, correlated requests, assembly coalescing/cancellation, reserved output, full/delta observations and orderly shutdown                                                     |
| Supervision      | View leases, deadlines/RSS monitoring, uncertain outcomes, bounded WebView delivery, kill/reap and explicit worker recovery                                                               |
| Presentation     | English/Simplified Chinese, self-hosted/system monospace, native CSS, Bits UI, Lucide and application icon assets                                                                         |
| Tooling          | Workspace dependencies/lints, clap CLI/xtask, native build lifecycles, strict checks, contract generation and isolated behavior/property tests                                            |

Data watchpoints are implemented through QEMU's native matching, with post-dispatch
pauses, REP continuation, reset retention, desktop controls and CLI stop reports.
AArch64 flat RAM now has explicit Normal-memory semantics.

## Next product work

1. **Debugging:** define frame/unwind policy before step-out, and source provenance before supporting
   user-authored DWARF or mappings for modified code.
2. **Execution coverage:** extend the verified guest/extension matrix, including
   exceptions, vector-length transitions, undefined flags and self-modifying code.
   AVX-512 execution, x87 views and SME matrix views remain outside current scope.
3. **Workbench:** expand large-data views when measured workloads justify it;
   keep the source/document/loaded-machine boundary explicit as workflows grow.

Files use standard assembly text, raw bytes and ELF; no saved-experiment container
is planned. Local document/settings recovery remains. Importing a file never
implicitly loads or patches a machine.

## Later capabilities

- [ ] SysV AMD64, Windows x64 and AAPCS64 integer/buffer function experiments with
      declared stack and return policies.
- [ ] Assertions over observed execution, with explicit failure reporting.
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

Evidence below was recorded on Apple Silicon macOS. Automated suites, browser
fixtures, desktop acceptance and sanitizer runs establish different guarantees;
they were not one complete matrix.

### Current baseline — 2026-09-26

| Scope   | Recorded result                                                                                                                                              |
| ------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| SDK     | Both QEMU guest adapters rebuilt with LLVM/LLD 23.1.2                                                                                                        |
| Checks  | `cargo xtask check`, including native linting and 56 generated declarations, passed                                                                          |
| Tests   | **170 Rust tests**, including doctests, and **161 frontend tests** passed; the frontend suite was rerun with localhost access after a sandbox startup denial |
| Desktop | Fresh macOS debug bundle built through normal Tauri hooks; both guests exercised through real IPC                                                            |

Data-watchpoint coverage includes cross-page and overlapping ranges, access filtering,
same-value stores, REP continuation, AVX2/SVE and atomic stores, inactive SVE lanes,
fault priority, fetch exclusion and reset retention. Range properties cover
wraparound; fixed cases verify the 32/33-input boundary. Native tests verify complete
affected bytes and exact non-hit completion. AArch64 regressions distinguish ordinary
unaligned Normal-memory accesses from instructions requiring alignment.

Worker/CLI tests verify atomic rejection, hit delivery and resulting memory. Stream
properties vary range, length and access independently, including replacement,
inheritance and clearing. Browser fixtures verify pending keyboard edits,
rejection/retry and removal without implementing backend behavior in the fixture.

Desktop acceptance verified AVX2 and SVE2 watchpoint creation, post-store pauses,
continuation to checksum 4814, x86 reset retention, replacement on load and bilingual
controls. The AVX2 flow also exercised trace inspection. Drafts and locale were
preserved, and the app and worker exited after acceptance. Built-in browser checks
covered search, line navigation and 880×600 / 414px layouts; the desktop pass covered
native select alignment.

The subsequent repository audit rebuilt the SDK and debug bundle, reran workspace
checks and tests, and removed duplicate observation-cache/delivery copies, buffered
pipe writes with per-frame flushes, and eliminated temporary address-format strings
during wire decoding. Properties verify memory replacement/inheritance and that
later deltas leave previously delivered snapshots unchanged. This audit did not
repeat desktop interaction or sanitizer acceptance; performance gains have not
been benchmarked.

### Documents and delivery — 2026-09-20

Native checks/tests passed after an SDK rebuild with LLVM/LLD 23.1.1. Attachment
regressions reject obsolete replies, failures and acknowledgements after
reconnect/detach, preserve current failures and release stale leases.

Workbench coverage verifies independent text/target/history/search/builds,
late outcomes for inactive or closed sources, import conflicts, recovery quotas,
per-tab closure and a fresh editor after closing the last source. Background closure
preserves active edits and undo; closing any source preserves the loaded machine.

Built-in browser acceptance covered divider dragging, long names, selected-tab
visibility, source-list navigation and upper-right search/line panels. At 880×600
and 414px, controls remained reachable; the narrow layout hid dividers without
horizontal overflow. Fresh macOS debug bundles verified bilingual interaction,
native titles, renaming, keyboard navigation/resizing and confirmation cancellation.
Separate runs executed both examples to checksum 4814 while switching documents.
The final tab-only acceptance reused native binaries and did not repeat guest execution.

An earlier register-form keyboard submission failed, then passed in isolation and
subsequent full runs without assertion changes; its root cause remains unresolved.

### Native baseline — 2026-09-17

| Scope            | Recorded result                                                                                                 |
| ---------------- | --------------------------------------------------------------------------------------------------------------- |
| SDK              | QEMU 11.1.1, XED/mbuild v2026.08.23 and GNU C23 adapter built with LLVM/LLD 23.1.1                              |
| Workspace checks | `cargo xtask check`: Rust/C/C++ and frontend lints, types, 53 generated declarations and formatting passed      |
| Automated tests  | `cargo xtask test`: 158 Rust tests, including doctests, and 136 frontend tests passed                           |
| Guest execution  | Both guests: loading, flags, memory faults, REP, cache invalidation, live edits/reset, CLI and worker transport |
| SIMD             | Full YMM/Z/P/FFR, highest lanes, aliases/inactive bits, rounding, independent guest stores and bank delivery    |

Source-debugging tests cover relocated DWARF, macros/repetition, Unicode, gaps,
foreign files and exact metadata limits. Truncation retains complete line groups.
Both guests exercise generated repetition counts and link addresses; real worker
checks verify map delivery, atomic breakpoint updates and reset. Chromium covers
navigation, partial groups, rejection/retry, `F9`, edit invalidation and concurrent
decode outcomes in both orders.

Temporary-goal tests cover direct/indirect and recursive calls, user-breakpoint
interruption, current-PC targets and budgets. Trace properties compare recording
and clear operations with independent history, including fixed 511/512/513-start
boundaries. REP records one start; data faults and failed fetches remain distinct.
Worker tests verify replies without observation-sequence changes, terminal clear
and stale-generation rejection. Chromium covers late replies after reset/reconnect,
consecutive steps, hidden-panel reads and source links. Live decoding covers patches,
mapping edges and x86 instructions spanning adjacent mappings.

Both bundled programs match scalar RGBA results and checksum 4814 at two link
addresses and after reset. SVE2 additionally passed 1, 3, 7, 8, 63, 64, 65, 127 and
129 pixels at MAX's 256-byte vector length. Synthetic length properties do not
verify guest-driven length transitions.

Packaged WKWebView checks covered both examples, SIMD/Float16 edits, FFR's highest
bit, reset, architecture/locale changes, search/replacement, undo, focus and appearance.
Separate runs covered analysis, memory/PC edits, patches and source/binary I/O.
AArch64 debugging acceptance included source/instruction navigation, breakpoints,
stepping, address/cursor targets, trace recording/clear and completion with checksum 4814. Direct/indirect and recursive calls were verified in native tests, not that UI pass.

Visual checks covered register/memory/instruction contrast, matching search/line
panels, half-screen layout, 1440×900, 880×600 and 414px widths. Not every size/locale
was repeated for the expanded SIMD bank or source debugger in WKWebView.

### Sanitizers and remaining limits

AddressSanitizer verified x86 notifier-lifetime and APIC-index fixes; 44 runtime/engine
tests passed with the instrumented SDK. Process-lifetime QEMU globals were excluded
from leak detection. Unmodified QEMU's coroutine suite reproduced macOS
`__asan_handle_no_return` warnings while all 13 tests passed: its `sigaltstack`
backend lacks ASan fiber-switch notifications. Coroutine-stack evidence is limited,
and subsequent native changes have not repeated that run.

The current Vitest/Vite combination warns that the mocks interceptor's
`configureServer` hook is ignored. Tests pass without relying on it; recheck
compatibility before using that facility.

Windows/Linux, release-static/cross/universal builds and signed, independently
installed packages remain unverified. No desktop WebDriver harness is installed.
[Testing](testing.md) defines acceptance; the release gates above remain open.

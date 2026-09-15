# Roadmap

The desktop assembly/execution loop works. Oplab is not yet a complete debugger or
an independently distributable application. Checked items describe implementation;
[testing](testing.md) defines evidence and [development](development.md) owns setup.

## Implemented

- [x] Rust core, engine and desktop boundaries; workspace dependencies/lints, stable
      toolchain, clap CLI/xtask and native Cargo/Vite/Tauri build ownership.
- [x] Strict Rust/C++/TypeScript/Svelte/CSS checks, formatting, dependency analysis
      and Rust-owned TypeScript contract verification.
- [x] LLVM 23 MC/LLD through CXX/C++23, unchanged GNU-style source, complete standard
      ELF object/image output and separate compile/link operations.
- [x] Validated addresses, permissions, program-header loading, BSS/padding and RX protection.
- [x] Both guests, bounded execution slices, step/run/pause/stop/reset, address
      breakpoints in the engine, integer/memory observations and explicit stop/fault outcomes.
- [x] Bounded framing, correlated requests, build coalescing/cancellation, output
      reservations, coherent full/delta subscriptions and orderly shutdown.
- [x] Desktop leases, deadlines/RSS monitoring, unknown outcomes, bounded delivery,
      kill/reap and explicit worker recovery.
- [x] Source/machine/observation interface, stale-build handling, scratch recovery,
      configuration, appearance, focus mode, adjustable panel proportions and layout reset.
- [x] CodeMirror/Lezer lexical assistance, completion, comment/search/history commands,
      bracket matching and multiple selections; dynamically imported editor.
- [x] Build-scoped diagnostics, exact UTF-8 to UTF-16 positioning, CodeMirror point
      markers and navigation; stale results cannot annotate newer source.
- [x] Native UTF-8 source and raw-byte import/export, complete ELF exports, bounded
      reads and temporary-file replacement; paths stay outside the frontend.
- [x] Instruction inspection of raw bytes or individual file-backed ELF segments,
      explicit byte offsets, bounded pages and stale-result rejection.
- [x] On-demand analysis in the workbench, worker and CLI: register/memory metadata,
      x86 control/flags/CPUID and AArch64 groups/writeback, with explicit metadata limits.
- [x] Batch CLI execution from unchanged source or standard ELF, explicit completion
      and instruction/time budgets, final registers/faults and optional bounded memory.
- [x] English/Simplified Chinese, self-hosted JetBrains Mono/system monospace,
      font size/wrap preferences, native CSS, Bits UI primitives and Lucide icons.
- [x] Tests outside production directories, with independent Proptest/fast-check
      oracles, generated guest programs and observation histories, scoped concurrency
      probes and Chromium new-headless workbench flows.
- [x] One application SVG master and PNG/ICO/ICNS assets with documented conversion.

## Next product work

1. **Source provenance:** derive verified MC/DWARF source/instruction relationships,
   including macros, duplicate locations, Unicode, data and padding. Diagnostic
   points are implemented; they are not instruction provenance.
2. **Execution coverage:** extend the tested guest/extension matrix beyond static
   recognition; CPU selection, SIMD observations and precise unsupported behavior
   need explicit contracts.
3. **Machine setup:** validated initial registers/mappings and explicit raw-code
   loading, alias writes, executable patches and translated-code invalidation.
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

Verified on **2026-09-15**:

- `cargo xtask check` and `cargo xtask test`: 84 Rust unit/integration tests,
  2 doctests and 60 frontend tests, including 36 Chromium cases. Rustdoc also
  passed with warnings denied; generated TypeScript matches the Rust contracts.
- Batch CLI source/ELF runs on both guests preserved arbitrary full-width arithmetic
  and memory results, including precision/overflow boundaries. Tests covered exact
  stdin limits, sectionless ELF entry handling, absolute/ambiguous symbols, rejected
  TLS offsets, partial faults, environment stops, instruction limits and cooperative timeout.
- Static frontend and Tauri debug builds through the normal hooks. Both guests
  assembled, loaded, stepped and stored 42; reset restored state, failed assembly
  preserved the machine, and a bounded loop paused/stopped.
- Native Unicode diagnostic navigation, a byte-exact AArch64 source/code workflow,
  and a 126,980-byte ELF segment exported unchanged and decoded through its end.
  Static analysis preserved loaded-machine state and survived locale changes.
- Browser and native layout review covered both languages, font/wrap preferences,
  focus mode, layout reset, narrow instruction navigation and the larger default
  window. Fonts and the dynamic editor loaded under the app origin/CSP.

All 10 ICNS representations and 6 ICO layers matched PNG references on
**2026-09-14**. Test design, acceptance procedures and the known Vitest/Vite mock-hook
warning are maintained in [testing](testing.md).

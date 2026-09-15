# Roadmap

The desktop assembly/execution loop works. Oplab is not yet a complete debugger or
an independently distributable application. Checked items describe implementation;
[testing](testing.md) defines evidence and [development](development.md) owns setup.

## Implemented

- [x] Concise workspace directories, named Rust module entries, editor/machine
      feature groups and corresponding tests; framework entry points retain defaults.
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
      configuration, appearance, focus mode and adjustable panel proportions.
- [x] CodeMirror/Lezer lexical assistance, completion, comment/search/history commands,
      bracket matching and multiple selections; dynamically imported editor.
- [x] English/Simplified Chinese, self-hosted JetBrains Mono/system monospace,
      font size/wrap preferences, native CSS, Bits UI primitives and Lucide icons.
- [x] Standard component event composition and native disabled controls; no custom
      focus-management layer. Broader accessibility acceptance remains open.
- [x] Tests outside production directories, with independent Proptest/fast-check
      oracles, generated guest programs and observation histories, scoped concurrency
      probes and Chromium new-headless workbench flows.
- [x] One application SVG master and PNG/ICO/ICNS assets with documented conversion.

## Next product work

1. **Diagnostics and provenance:** map UTF-8 diagnostic offsets to UTF-16 editor
   positions, then derive verified MC/DWARF source/instruction relationships, including
   macros, duplicate locations, Unicode, data and padding.
2. **Instruction inspection:** bounded machine-code import, disassembly/static effects
   and a tested target/extension capability matrix.
3. **Machine setup:** validated initial registers/mappings, alias writes, executable
   patches and translated-code invalidation.
4. **Saved experiments:** versioned setup/source/policy format, atomic save/reopen,
   conflicts and recovery; CLI execution using the same model.
5. **Workbench expansion:** multiple documents, coordinated source/instruction focus,
   accessible draggable separators and large-data views when those capabilities exist.

## Later capabilities

- [ ] Source breakpoints, watchpoints and explicit before/after timing.
- [ ] SysV AMD64, Windows x64 and AAPCS64 integer/buffer function experiments with
      declared stack and return policies.
- [ ] Assertions and bounded traces with visible truncation; static effects remain
      distinct from observed execution.
- [ ] Verified undefined flags, self-modifying code and broader instruction coverage.
- [ ] Complete snapshots/replay, selected import/export formats, SIMD and optional
      static performance analysis, each with its own semantics and acceptance gates.

## Release gates

- [ ] Complete-workflow state-machine properties, parser fuzzing and adversarial
      native resource/expansion tests.
- [ ] Measured assembly, execution, IPC, rendering, startup and memory workloads.
- [ ] Windows/Linux/macOS CI, both guests, coherent native toolchains and declared
      host/WebView minimums; static/cross/universal builds where supported.
- [ ] Native WebDriver workbench flows with real IPC and worker, including failure,
      stale edits, reset and eventual save/reopen. Automation stays out of production.
- [ ] Broader keyboard/screen-reader, zoom and real-WebView acceptance using native
      semantics and library primitives.
- [ ] Transitive native-library bundling/discovery, dependency/font notices,
      signing/JIT policy, installation and sidecar startup on each supported host.
- [ ] Production CSP/capabilities, privacy and failure-artifact review.

Do not install speculative dependencies or create placeholder modules for pending
work. Browser fixtures and local debug builds do not satisfy distribution gates.

## Verification

Current evidence is limited to local Apple Silicon macOS development. It does not
establish Windows/Linux support or signed, independently installed distributions.

| Scope             | Latest verified result                                                                                                                                                                                                                                                                           |
| ----------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Workspace         | `cargo xtask check` and `cargo xtask test` passed on 2026-09-15: 62 Rust unit/integration tests, 2 doctests and 27 frontend tests, including 8 Chromium component cases. Native tests ran with the host access described in the testing guide.                                                   |
| API documentation | Rustdoc with warnings denied and both executable documentation examples passed on 2026-09-15; generated TypeScript comments match the Rust contracts.                                                                                                                                            |
| Repository layout | Migrated paths, imports, build configuration and documentation were checked on 2026-09-15; no files were lost and Cargo package/binary names are unchanged.                                                                                                                                      |
| Frontend          | Static production build passed on 2026-09-15. Visual review covered desktop, minimum-window and narrow layouts, both languages, fonts, wrapping and focus mode.                                                                                                                                  |
| Desktop           | A fresh debug app built through Tauri's default hooks on 2026-09-15. Both guests assembled, loaded, stepped and stored 42; reset restored state, failed assembly preserved the machine, and a bounded loop paused/stopped. Fonts and the dynamic editor loaded under the application origin/CSP. |
| Icons             | All 10 ICNS representations and 6 ICO layers matched their PNG references on 2026-09-14.                                                                                                                                                                                                         |

The Vitest/Vite mock-hook warning and remaining keyboard/focus acceptance are
recorded in [testing](testing.md#browser-coverage). The product work and release
gates above remain open.

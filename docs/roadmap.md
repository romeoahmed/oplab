# Roadmap

The basic desktop execution loop exists. The complete debugging product does not.
Checked items are implemented; acceptance limits are explicit in [testing](testing.md).
[Architecture](architecture.md) and [engine](engine.md) define the intended boundaries.

## Implemented

- [x] Three product Rust packages, Cargo xtask, stable Rust, inherited dependencies/lints, lockfiles, and native framework build hooks.
- [x] Strict Rust/C++/TypeScript/Svelte/CSS checks, formatters, dependency analysis, and generated contract verification.
- [x] LLVM 23 MC/LLD CXX bridge with C++23, standard source, complete relocatable/executable ELF, independent compile/link operations, and native compilation metadata.
- [x] Validated addresses, non-wrapping ranges, memory permissions, ELF program-header loading, BSS/padding, and native RX protection.
- [x] Both guest execution owners, bounded slices, step/run/pause/cancel/reset, integer observations, address breakpoints, and explicit completion/budget/fault outcomes.
- [x] Bounded binary protocol, correlated requests, assembly coalescing/cancellation, reserved output capacity, coherent full/delta subscriptions, and orderly shutdown.
- [x] Desktop supervision, connection/view leases, unknown outcomes, deadlines, RSS monitoring, bounded diagnostics, and kill/reap recovery.
- [x] Editor, artifact loading, controls, registers, memory, stale-document handling, and bounded scratch recovery.
- [x] English and Simplified Chinese, native locale selection, retained editor history/search, semantic controls, native CSS, and Lucide interface icons.
- [x] Declarative clap CLI/xtask commands; tests outside production directories; property models for pending edits, fragments, Unicode recovery, and memory windows.
- [x] One application SVG master and verified PNG/ICO/ICNS assets; documented conversion through ImageMagick and Tauri.

## Next product work

- [ ] Integrate UTF-8 diagnostic positions with UTF-16 editor ranges; derive structured source provenance from MC/DWARF, including macros, duplicates, data, padding, and Unicode.
- [ ] Add bounded machine-code import and instruction views, static effects, and a tested target/extension capability matrix.
- [ ] Add validated initial registers and mappings, debugger aliases, executable patches, and translated-code invalidation.
- [ ] Implement versioned experiments, atomic save/reopen, conflict detection, complete recovery, and CLI execution of the same experiment model.
- [ ] Add multiple documents, deliberate focus transitions, keyboard resizing, and accessible large-data inspection.
- [ ] Automate complete native workbench flows, including failures, stale edits, reset, and eventual save/reopen.

## Professional debugging

- [ ] Source breakpoints and watchpoint timing with before/after semantics.
- [ ] SysV AMD64, Windows x64, and AAPCS64 integer/buffer function experiments with explicit stack and return policies.
- [ ] Assertions, bounded trace with visible truncation, and separation of static versus observed effects.
- [ ] Verified alias writes, undefined flags, self-modifying code, and broader independent instruction coverage.
- [ ] Measured assembly, execution, IPC, rendering, startup, and memory workloads.

## Extensions

- [ ] Complete machine snapshots and deterministic replay with explicit external-state limits.
- [ ] Selected portable import/export formats with separate acceptance gates.
- [ ] SIMD with tested storage, views, and execution semantics.
- [ ] Optional static performance analysis with declared microarchitecture assumptions.

## Release gates

- [ ] State-machine properties for complete workflows, isolated fuzzing, adversarial native resource tests, and representative benchmarks.
- [ ] Windows, Linux, and macOS CI exercising both guests and coherent native toolchains.
- [ ] WebdriverIO/Tauri native automation, with embedded automation excluded from production.
- [ ] Declared host/WebView minimums and verified native, static, cross, and universal build paths where supported.
- [ ] Native library discovery/bundling, licensing, signing/JIT policy, installation, and sidecar discovery on each host.
- [ ] Production CSP, command capabilities, dependency/license audit, privacy, failure artifacts, and full save/reopen acceptance.

Do not install dependencies or add empty modules/configurations for unchecked work.
A capability is complete only when its actual behavior and failure boundaries are
verified; browser fixtures and local debug builds do not establish release readiness.

## Verification record

Verified on 2026-09-14:

- `cargo xtask check`: strict Rust/C++/frontend analysis, Knip, 34 generated declarations, and all formatters passed.
- Rust workspace: 62 passing tests, including 14 core, 46 engine, and 2 desktop tests.
- Frontend: 18 passing tests, including 13 logic/property tests and 5 Chromium new-headless browser cases.
- Application icon: all 10 ICNS representations and 6 ICO layers match corresponding PNG references pixel-for-pixel after extraction.

- `pnpm tauri build --debug --bundles app`: default hooks built a matching worker and static macOS application bundle.
- The fresh bundle ran from `tauri://localhost`: both guests assembled, loaded, completed, and stored 42. AArch64 single-step, reset, cross-target replacement, and locale changes preserved the documented state boundaries.
- Documentation links and local-path hygiene passed. The bundle contains the verified application icon.

These are local debug-build results. Installed/signed distributions, other hosts,
WebDriver automation, full experiment persistence, and the product gates above are
still open. The known Vitest/Vite mock-hook warning is recorded in [testing](testing.md).

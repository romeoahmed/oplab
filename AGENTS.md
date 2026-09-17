# AGENTS.md

Oplab is a Tauri 2 / Svelte 5 assembly workbench for x86_64 and AArch64.
Read [roadmap](docs/roadmap.md) before choosing work and
[development](docs/development.md) before native builds. Distinguish implemented
behavior, planned capabilities and verified platforms.

## Commands

Run from the repository root. Install with `pnpm install --frozen-lockfile`.
Native work requires stable Rust, GNU C23, C++23, LLVM 23 with matching LLD development
files, the QEMU/XED SDK and [platform prerequisites](docs/development.md#native-toolchain).
Use installed tool discovery; do not invent paths or patch generated headers.

| Command                                                     | Purpose                                                   |
| ----------------------------------------------------------- | --------------------------------------------------------- |
| `pnpm dev`                                                  | Frontend HMR; no assembly or execution in browser preview |
| `pnpm tauri dev`                                            | Stage the worker and start the desktop app with Vite      |
| `pnpm check` / `pnpm lint`                                  | Catalog/types; ESLint, Stylelint and Knip                 |
| `pnpm test --project logic` / `pnpm test --project browser` | Focused frontend suites                                   |
| `cargo xtask check`                                         | Workspace lints, types, contract drift and formatting     |
| `cargo xtask test [--release]`                              | Rust workspace and frontend tests                         |
| `cargo xtask fmt [--check]`                                 | Rust, web, documentation and C/C++ formatting             |
| `cargo xtask codegen [--check]`                             | Export/verify Rust-owned TypeScript DTOs without LLVM     |
| `cargo xtask sdk`                                           | Build the versioned QEMU/XED SDK                          |
| `cargo xtask sidecar [--release]`                           | Rebuild and stage the worker                              |

Use one Vite server per checkout; stop it before Tauri dev or a static build.
Native changes need worker staging and an app restart; Tauri's watcher does not
rebuild the worker. Rebuild the SDK after QEMU adapter changes. Frontend recipes
belong in package.json; cross-tool tasks belong in xtask. Preserve native tool lifecycles.

## Ownership and invariants

Follow the [repository layout](docs/architecture.md#repository-layout):

- `crates/core`: pure domain policy and authoritative wire DTOs.
- `crates/toolchain`: LLVM MC/LLD and XED/MC static inspection.
- `crates/runtime`: QEMU CPU state, TCG lowering and LLVM ORC execution.
- `crates/engine`: validated loading and execution sessions.
- `crates/runner`: worker and CLI; `src-tauri`: supervision, IPC and selected-file I/O.
- `src/lib/desktop`: the only frontend module importing Tauri APIs.
- `src/lib/protocol`: generated declarations and pure wire transformations.
- `src/lib/workbench`: document state and presentation; `load` owns editable load
  inputs, `machine` owns live views. Keep editor/instruction helpers and CSS with their features.

Pass assembly to LLVM unchanged and preserve standard ELF semantics. Protocol
metadata describes artifacts; it does not redefine their layout or relocations.
Keep document, artifact and session identities separate. Reject stale results;
never replay uncertain mutations. Native handles stay on their owning threads,
and queues, allocations and payloads stay bounded. llvm-sys owns LLVM discovery
and linkage; CXX consumes its Cargo metadata.

Edit DTOs and exported comments in Rust, then run codegen. Never hand-edit
`src/lib/protocol/generated`. The private protocol stays at version 1 during
development: evolve one schema and rebuild clients/workers together. Paraglide,
SvelteKit, Tauri-generated output and staged worker/runtime binaries remain ignored.

## Editing

- Keep framework entry points and root configurations conventional. Components use
  PascalCase; TS/CSS modules use descriptive lowercase names; rune modules use
  `.svelte.ts`. Rust modules use snake_case; integration targets use kebab-case.
  Keep workspace directories concise and Cargo package/executable names prefixed `oplab-`.
- Inherit Rust dependencies/lints from the workspace. Use compatible minor-version
  requirements and preserve lockfiles unless dependencies change. Keep MPL-2.0,
  the unchanged official LICENSE and package metadata aligned; no Exhibit B opt-out.
- Clippy all/pedantic/nursery run with warnings denied. Fix findings or use a narrow,
  reasoned `#[expect]`, never `#[allow]`. Retain scoped visibility and disabled
  `redundant_pub_crate`.
- Prefer typed enums/newtypes, pure transformations and explicit effect owners.
  Avoid redundant state, unchecked casts and speculative abstractions.
- Share `examples/` unchanged through Vite raw imports and Rust execution tests.
  Editor workflow tests use independent small documents.
- Svelte runes own presentation; derive computed state and use attachments for widget
  lifetime/cleanup. Reconfigure CodeMirror compartments without replacing history.
- Use semantic HTML, native CSS/inputs/forms and Bits UI composite controls. Preserve
  native validation/disabled behavior and library event/attachment composition.
  Avoid custom focus patches; Newly Baseline features need actual WebView acceptance.
- Update both language catalogs with aligned keys/placeholders and concise natural
  wording. Keep source, identifiers and standard units untranslated. Ordinary UI
  excludes host paths, raw backend diagnostics and internal delivery counters.
- Comments explain constraints: units, ownership, invariants and failures. Use concise
  TSDoc and Rust item/module docs; retain meaningful error sections and FFI safety
  explanations. [Documentation guidance](docs/development.md#api-documentation) owns details.
- Keep secrets, host paths and local logs out of tracked files. `static/icon.svg` is
  the icon master; follow [conversion and visual checks](docs/development.md#configuration-and-assets).

## Verification and delivery

Keep tests under root or Rust package `tests/`, mirroring feature ownership.
Private Rust tests use `#[cfg(test)]` path modules without widening production APIs.
Test observable behavior, invariants and independent architectural facts; avoid
incidental DOM structure, internal implementation and translation wording. Use
Proptest/fast-check where useful and retain minimized regressions. Use Unicode
escapes for encoding fixtures, events/promises for coordination and bounded deadlines.

Use Playwright Chromium in new headless mode only:
`pnpm exec playwright install --with-deps --no-shell chromium`.
UI acceptance covers both languages, keyboard flows, realistic sizes and retained
editor state. Native changes need real worker/guest evidence; relevant UI/native
changes also need a fresh static desktop bundle. Follow [testing](docs/testing.md).

Run checks proportionate to the change: actual xtask flows for orchestration,
codegen verification for contracts, workspace checks/tests for broad changes.
Report what ran and what remains unverified. Local debug success does not establish
distribution readiness. Keep setup in development docs, contracts in their owning
references and dated evidence in the roadmap.

Inspect staged and unstaged changes; preserve user work. Do not stage, commit,
reset or discard changes unless requested.

# AGENTS.md

Oplab is a Tauri 2 / Svelte 5 assembly workbench for x86_64 and AArch64.
Read [the roadmap](docs/roadmap.md) before choosing work and
[development](docs/development.md) before native builds. Keep implemented behavior,
planned capabilities and verified platforms distinct.

## Commands

Run from the repository root. Install dependencies with `pnpm install --frozen-lockfile`.
Native work needs stable Rust, C++23, LLVM 23 with matching LLD development files,
libclang and the [remaining build prerequisites](docs/development.md#native-toolchain).
Do not invent tool paths or patch generated headers to bypass missing dependencies.

| Command                                                     | Purpose                                                         |
| ----------------------------------------------------------- | --------------------------------------------------------------- |
| `pnpm dev`                                                  | Frontend HMR; browser preview cannot assemble or execute        |
| `pnpm tauri dev`                                            | Stage the worker and start the desktop app plus Vite            |
| `pnpm check` / `pnpm lint`                                  | Catalog/type checks; ESLint, Stylelint and Knip                 |
| `pnpm test --project logic` / `pnpm test --project browser` | Focused frontend suites                                         |
| `cargo xtask check`                                         | Workspace lints, types, generated-contract drift and formatting |
| `cargo xtask test [--release]`                              | Rust workspace and frontend tests                               |
| `cargo xtask fmt [--check]`                                 | Rust, web, documentation and C++ formatting                     |
| `cargo xtask codegen [--check]`                             | Export or verify TypeScript DTOs without LLVM                   |
| `cargo xtask sidecar [--release]`                           | Rebuild and stage the worker                                    |

Use one Vite server per checkout; stop standalone Vite before starting Tauri dev.
Engine changes require worker staging and an app restart; Tauri's watcher does not
rebuild the worker. Frontend recipes belong in package.json, cross-tool tasks in
xtask. Preserve native Cargo/Vite/Tauri lifecycles instead of adding wrappers.

## Boundaries

Follow the [repository layout](docs/architecture.md#repository-layout).

- `crates/core` owns pure domain policy and authoritative wire DTOs.
- `crates/engine` owns LLVM MC/LLD, decoding, ELF loading, Unicorn, worker and CLI.
- `src-tauri` owns supervision, bounded IPC and user-selected native file I/O.
- `src/lib/desktop` is the only frontend module allowed to import Tauri APIs.
- `src/lib/protocol` owns generated declarations and pure wire transformations.
- `src/lib/workbench` owns presentation and document state; group editor, machine
  and instruction code with related CSS and pure helpers.

Pass assembly to LLVM unchanged. Preserve standard ELF semantics; protocol metadata
must describe artifacts rather than replace their layout or relocation rules.
Keep document, artifact and session identities separate. Reject stale results and
never replay an uncertain mutation. Native handles stay on their owning threads;
queues, allocations and payloads stay bounded.

Edit DTOs and exported comments in Rust, then run codegen. Never hand-edit
`src/lib/protocol/generated`. The private protocol remains version 1 during
development: evolve one schema and rebuild clients/workers together, without
historical compatibility branches. Paraglide, SvelteKit, Tauri-generated output
and staged worker binaries remain ignored.

## Editing

- Keep framework entry points and root configurations in conventional locations.
  Components use PascalCase; TypeScript/CSS modules use descriptive lowercase names.
  Use `.svelte.ts` for modules with runes. Rust modules use snake_case named entry
  files; integration targets use kebab-case. Keep workspace directory names concise
  and retain `oplab-` in Cargo package and executable names.
- Inherit Rust dependencies and lints from workspace Cargo.toml. Use compatible
  minor-version requirements and preserve lockfiles unless dependencies change.
  Keep `MPL-2.0`, the unmodified official `LICENSE` and package metadata aligned.
  Preserve secondary-license compatibility; do not apply the Exhibit B opt-out.
- Clippy all/pedantic/nursery run with warnings denied. Fix findings; use a narrow,
  reasoned `#[expect]` only when justified, never `#[allow]`. Keep
  `redundant_pub_crate` disabled and retain explicit scoped visibility.
- Prefer typed enums/newtypes, pure transformations and explicit effect owners.
  Avoid redundant state, unchecked casts and speculative abstractions.
- Keep bundled programs in `examples/`, shared unchanged by Vite raw imports and
  Rust execution tests. Use independent small documents for editor workflow tests.
- Svelte runes own presentation; derive computed values and use attachments for
  external widgets and cleanup. Reconfigure CodeMirror compartments without
  replacing editor history.
- Use semantic HTML, native CSS/inputs and Bits UI for composite interactions.
  Preserve library event/attachment composition and native disabled behavior;
  use form submission and constraint validation for related actions. Avoid custom
  focus patches. Newly Baseline features need real WebView acceptance.
- Update both `messages/en.json` and `messages/zh-CN.json`. Keep keys/placeholders
  aligned and wording concise and natural. Do not translate source or expose host
  paths, raw backend diagnostics or internal delivery counters in ordinary UI.
- Document units, ownership, invariants and failure behavior. Use concise TSDoc and
  Rust item/module docs; retain useful error sections and FFI safety explanations.
  Comments explain constraints, not signatures or obvious implementation steps.
- Keep secrets, machine-specific paths and local logs out of tracked files.
  `static/icon.svg` is the icon master; follow the documented
  [conversion and visual checks](docs/development.md#configuration-and-assets).

## Verification and delivery

Keep tests under root or Rust package `tests/`, mirroring feature ownership.
Private Rust tests use `#[cfg(test)]` path modules without widening production APIs.
Test behavior, invariants and independent architectural facts. Avoid assertions tied
to incidental DOM structure, internal implementation or translation wording. Use
Proptest/fast-check for meaningful invariants and retain minimized regressions.
Coordinate async tests with events/promises and bound failures with deadlines.

Use Playwright Chromium in new headless mode only. Install it with
`pnpm exec playwright install --with-deps --no-shell chromium`.
Browser fixtures establish component behavior; native changes need real worker/guest
evidence. UI changes need both languages, keyboard flows, realistic window sizes,
retained editor state and a fresh static desktop bundle when relevant. Follow
[testing and acceptance](docs/testing.md).

Run checks proportionate to the change: actual xtask flows for orchestration,
codegen verification for contracts, workspace checks/tests for broad changes.
Report what ran and what remains unverified; a local debug bundle does not prove
distribution readiness. Update the owning document and roadmap when scope changes.

Inspect staged and unstaged changes and preserve user work. Do not stage, commit,
reset or discard changes unless requested. Keep build recipes in development docs,
contracts in their owning reference, and dated evidence in the roadmap.

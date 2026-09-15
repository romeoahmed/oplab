# AGENTS.md

## Project and boundaries

Oplab is a Tauri 2 / Svelte 5 desktop assembly workbench for x86_64 and AArch64.
Read [the roadmap](docs/roadmap.md) for implemented scope and
[the development guide](docs/development.md) before native build work.
Keep planned capabilities distinct from shipped behavior.

| Location                   | Owner                                                                  |
| -------------------------- | ---------------------------------------------------------------------- |
| `crates/core`              | Pure domain types, policies and authoritative wire DTOs                |
| `crates/engine`            | LLVM MC/LLD C++23 bridge, ELF loader, Unicorn sessions, worker and CLI |
| `src-tauri`                | Tauri commands, worker supervision and bounded IPC delivery            |
| `src/lib/workbench`        | Editor, document controller, machine/memory views and native CSS       |
| `src/lib/desktop`          | Only frontend module allowed to import Tauri APIs                      |
| `src/lib/protocol`         | Generated declarations plus pure wire/scalar/observation logic         |
| `src/lib/i18n`, `messages` | Locale selection and bilingual catalogs                                |
| `xtask`                    | Cross-tool checks, formatting, code generation and sidecar staging     |

Assembly source reaches LLVM unchanged. Preserve standard ELF; protocol metadata
must describe it, not replace its semantics. Document, artifact and loaded session
have separate identities. Reject stale results and never replay an uncertain mutation.
Native handles stay on their owning threads; queues and payloads stay bounded.

## Setup and commands

- Install JavaScript dependencies with `pnpm install --frozen-lockfile`.
- `pnpm dev` starts frontend HMR; preview cannot assemble or execute.
- `pnpm tauri dev` stages the worker and starts the desktop app plus Vite. Reuse one
  Vite server per checkout; stop a standalone server before starting Tauri dev.
- Native builds require stable Rust, C++23, LLVM 23 with matching LLD development
  files, libclang, CMake/build tools, pkg-config and the platform SDK. Follow
  `docs/development.md`; do not invent paths or patch generated headers to bypass setup.
- `pnpm check`: catalogs and Svelte/TypeScript checks.
- `pnpm lint`: ESLint, Stylelint and Knip.
- `pnpm test [--project logic|browser]`: frontend tests.
- `cargo xtask check`: complete workspace checks, including generated contract drift.
- `cargo xtask test [--release]`: Rust workspace plus frontend tests.
- `cargo xtask fmt [--check]`: rustfmt, Oxfmt and clang-format.
- `cargo xtask codegen [--check]`: regenerate/verify TypeScript DTOs without LLVM.
- `cargo xtask sidecar [--release]`: rebuild/stage the worker; engine changes also
  need an app restart. Tauri's application watcher does not rebuild the worker.

Keep frontend recipes in package.json and call them from xtask. Use native Cargo,
Vite and Tauri lifecycles instead of adding duplicate wrappers or ad hoc scripts.

## Editing conventions

- Keep SvelteKit/Tauri/Cargo entry points and root tool configurations in their
  conventional locations. Follow the [repository layout](docs/architecture.md#repository-layout).
  Group workbench editor and machine code with their related CSS and pure helpers;
  mirror feature paths under `tests/`, with shared DTOs in `tests/fixtures`.
- Components use PascalCase; TypeScript/CSS modules use lowercase descriptive names.
  Rust modules use snake_case named entry files; integration targets use kebab-case.
  Keep Cargo package/binary names distinct from concise workspace directory names.
- Rust dependencies and lints inherit from workspace Cargo.toml. Use compatible
  minor-version requirements; preserve lockfiles unless changing dependencies.
  Project licensing is `MIT OR Apache-2.0`, inherited by every Rust package and
  mirrored in package.json; keep both root license files and README aligned.
- Clippy all/pedantic/nursery run with warnings denied. Fix findings; use a narrow,
  reasoned `#[expect]` only when retaining the code is justified. Do not add `#[allow]`.
  `redundant_pub_crate` is deliberately disabled; retain explicit scoped visibility.
- Use typed enums/newtypes, pure transformations and explicit effect owners. Avoid
  redundant derived state, unvalidated casts and speculative abstraction layers.
- Svelte runes own presentation. `$derived` computes state; attachments own external
  widgets and cleanup. Reconfigure CodeMirror compartments without replacing history.
- Use semantic HTML, native CSS and native inputs. Bits UI owns composite keyboard
  interactions; preserve its event/attachment composition and native disabled controls.
  Avoid custom focus patches. Newly Baseline features still need WebView acceptance.
- Update `messages/en.json` and `messages/zh-CN.json` together. Keep keys/placeholders
  aligned, source text untouched and product copy concise and natural. Do not expose
  raw host paths, source diagnostics or internal transport counters in ordinary UI.
- Edit Rust DTOs, then run codegen; do not hand-edit `src/lib/protocol/generated`.
  Paraglide, SvelteKit, Tauri schemas/permissions and staged binaries are ignored output.
- Document contracts, units, ownership and failure behavior; ordinary comments explain
  non-obvious constraints. Use TSDoc summaries with tags only where useful, and Rust
  module/item docs with error sections and checked links. Preserve FFI safety reasons;
  generated contract comments come from Rust. Do not restate signatures or add filler.
- Keep tests under root `tests/` or Rust package `tests/`; private Rust tests use
  `#[cfg(test)]` path modules without widening production APIs.
- Keep machine-specific paths, secrets and local logs out of tracked files. The icon
  master is `static/icon.svg`; follow the documented conversion and visual verification.

## Verification and delivery

Test behavior, invariants and independent architectural facts, not incidental DOM
structure or internal implementation. Use Proptest/fast-check where they model a
real invariant; keep minimized regressions. Coordinate async tests with events or
promises and bound failures with deadlines.

Use only Playwright Chromium with new headless mode. Install it with
`pnpm exec playwright install --with-deps --no-shell chromium`. Browser fixtures
prove component behavior, not native IPC or guest execution. Test UI changes with
both languages, keyboard navigation, realistic window sizes and retained editor state.
Native changes need real worker/guest tests; desktop UI changes also need a fresh
static-bundle check when relevant. See `docs/testing.md` for the acceptance flows.

Run checks proportionate to the change. Command/orchestration changes need the
actual xtask flows; contracts need codegen verification; broad changes need workspace
checks/tests. Report exactly what ran and any blocked or unverified platform work.
Do not claim distribution readiness from a local debug bundle.

Inspect both staged and unstaged changes and preserve user work. Do not stage,
commit, reset or discard changes unless requested. Update the owning documentation
and roadmap when behavior or progress changes; avoid duplicate setup instructions.

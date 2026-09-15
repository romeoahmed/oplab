# Architecture

Oplab is a desktop assembly workbench for x86_64 and little-endian AArch64.
Its working loop is source → standard ELF → loaded machine → observable effects.
[Roadmap](roadmap.md) separates implemented capabilities from the planned debugger.

## Ownership

```text
Svelte workbench → Tauri supervisor → isolated worker
                                      ├─ assembly owner: LLVM MC → ELF object → LLD → ELF image
                                      └─ execution owner: ELF loader → Unicorn → observations
```

| Location                      | Responsibility                                                                             |
| ----------------------------- | ------------------------------------------------------------------------------------------ |
| `crates/core`                 | Validated domain types, memory/execution policy and wire contracts; no native dependencies |
| `crates/engine`               | Assembly, linking, decoding, loading, sessions, worker and CLI                             |
| `src-tauri`                   | Window-scoped commands, worker supervision, bounded delivery and user-selected file I/O    |
| `src/lib/workbench`           | Document controller, editor, machine, memory and instruction presentation                  |
| `src/lib/desktop`             | Sole frontend entry to Tauri; no native execution in browser preview                       |
| `src/lib/protocol`            | Rust-generated declarations and pure framing/scalar/observation logic                      |
| `src/lib/i18n`, `messages`    | Locale selection and English/Simplified Chinese catalogs                                   |
| `tests`, Rust package `tests` | Behavior and invariant tests outside production directories                                |
| `xtask`                       | Cross-tool verification, formatting, contract export and sidecar staging                   |

Keep abstractions with the state and effects they own. Native types stay in engine
adapters; desktop dependencies stay out of core. Add modules or crates for actual
boundaries, not empty service/repository/shared layers. Cargo, Vite, SvelteKit and
Tauri retain their normal build lifecycles; [development](development.md) owns commands.

## Repository layout

Keep framework entry points in their default locations: SvelteKit owns `src/routes`,
`src/app.html` and `static`; Tauri owns `src-tauri`; Cargo owns each package's
`src`, `tests` and `build.rs`. Root tool configuration stays discoverable without
custom config paths. See the official [SvelteKit](https://svelte.dev/docs/kit/project-structure),
[Tauri](https://tauri.app/start/project-structure/) and
[Cargo](https://doc.rust-lang.org/cargo/guide/project-layout.html) layouts.

```text
crates/
  core/                       # Package: oplab-core
    src/                      # Domain model and wire contracts
    tests/                    # Public domain/protocol tests
  engine/                     # Package: oplab-engine
    build/                    # LLVM discovery and C++ build support
    native/                   # CXX-facing C++ sources
    src/                      # Assembly, machine, session and worker modules
    tests/                    # Public integration targets
      common/                 # Process fixtures
      unit/                   # Private concurrency tests
src/
  lib/
    desktop/                  # Tauri IPC boundary
    i18n/                     # Locale selection
    protocol/                 # Wire operations and generated DTOs
    styles/                   # Application theme
    workbench/                # Composition, controller, scratch and preferences
      editor/                 # CodeMirror component, CSS and language support
      machine/                # Machine/register/memory views and initial viewport
      instructions/           # Static byte windows and bounded disassembly
  routes/                     # SvelteKit entry points
src-tauri/                    # Native application and supervisor
static/                       # Application icon master
messages/                     # English and Simplified Chinese catalogs
project.inlang/               # Localization project configuration
tests/                        # Frontend behavior and invariant tests
  fixtures/                   # Shared DTO fixtures
  workbench/                  # Mirrors editor/machine/instruction ownership
xtask/                        # Repository tooling
```

Use `PascalCase.svelte` for components, lowercase TypeScript/CSS module names, and
`.svelte.ts` only for modules using Svelte runes. Name files for their responsibility:
`scratch.ts` validates draft recovery, `editor/language.ts` supplies lexical assistance,
and `machine/memory.ts` selects a memory viewport. `ToolbarAction.svelte` identifies
its toolbar role. Avoid generic `utils`, `shared` or `components` buckets.

Rust modules use `snake_case.rs` with child modules in a sibling directory; module
entry points have explicit names such as `worker.rs` and `supervisor.rs`. Integration
targets use kebab-case, such as `worker-session.rs`. The test helper `common/mod.rs`
remains a module rather than a Cargo integration target. Directory names omit the
redundant project prefix; Cargo package names and executable names retain `oplab-`.

## Standards and state

LLVM owns GNU-style source parsing, macros, fixups, relaxation and ELF generation.
LLD resolves relocations and layout. Oplab passes complete source unchanged and
reads standard object/program metadata. Protocol fields describe artifacts and
operations; they do not redefine assembly, relocations or ELF layout. The
[engine contract](engine.md) specifies the language and supported runtime subset.

Three independent lifetimes prevent accidental state changes:

- **Document:** editable source, target and human inputs, including incomplete fields.
- **Artifact:** immutable output identified by document, revision, target, link base
  and assembler identity. A late result cannot replace a newer document's build.
- **Session:** immutable initial image and mutable machine state. Loading replaces
  it explicitly; reset advances its generation only after replacement succeeds.

Editing does not patch the machine. Reattaching a view does not invent an association
between retained machine state and current source. Missing memory differs from zero;
instruction starts differ from retired instructions; raw flags do not imply definedness.
Human addresses use ordinary hexadecimal input. Exact 64-bit JSON scalars use the
canonical representation defined in [protocol](protocol.md).

Use a functional core with explicit effect owners. Rust newtypes and tagged enums
enforce domain constraints. TypeScript uses strict unions, exhaustive switches,
`unknown` validation and pure transformations. Derive values rather than storing
parallel copies; avoid casts that bypass validation or a generic framework around
one operation. See the [Rust reference](https://doc.rust-lang.org/stable/reference/)
and [TypeScript's functional guidance](https://www.typescriptlang.org/docs/handbook/typescript-in-5-minutes-func.html).

## Technology decisions

| Technology                                  | Role and limit                                                                              |
| ------------------------------------------- | ------------------------------------------------------------------------------------------- |
| Tauri 2, Svelte 5, SvelteKit static adapter | Native desktop lifecycle and reactive static SPA; no server in the bundle                   |
| LLVM 23 MC/LLD, CXX, C++23                  | Whole-document standard assembly/linking behind a small ownership boundary                  |
| Unicorn                                     | One emulator API for both guests, with behavior independently verified                      |
| iced-x86, Capstone AArch64                  | Decoding and instruction metadata; recognition does not guarantee emulation support         |
| `object`                                    | ELF inspection; explicit project policy still validates loading                             |
| CodeMirror 6, Lezer                         | Transactional editing, lexical syntax assistance and completion; LLVM remains the assembler |
| Bits UI                                     | Composite toolbar, tooltip, tab, menu and popover behavior; native fields elsewhere         |
| Fontsource JetBrains Mono Variable          | Bundled offline WOFF2 with an optional system monospace preference                          |
| Paraglide                                   | Compiled bilingual messages and explicit locale selection                                   |
| Native CSS, Lucide                          | Platform layout and a tree-shaken interface icon family                                     |
| clap                                        | Typed CLI/xtask arguments and generated help                                                |
| Proptest, fast-check, Vitest Browser        | Domain invariants and real-browser component behavior                                       |

The direct MC boundary avoids reconstructing standard object/linker behavior around
an instruction emitter. Inkwell/llvm-sys primarily expose IR and LLVM's C API;
they do not replace MC's C++ interfaces. There is one assembly backend, without
Keystone or competing fallbacks. Dependency changes must remove real complexity,
fit the contract, and be checked against maintained official documentation and
resolved source. Root manifests own requirements; lockfiles own exact resolutions.

## Interface

The source area, right-hand machine panel and bottom observations form a stable
workbench. Assembly, loading and execution are separate actions. Configuration
states when each input applies: link address on build, stop position and instruction
limit on load. Appearance changes affect presentation only.

CodeMirror is dynamically imported through Svelte's
[await block](https://svelte.dev/docs/svelte/await); Vite owns chunking. One
[Svelte attachment](https://svelte.dev/docs/svelte/@attach) creates and destroys the
editor. [Compartments](https://codemirror.net/examples/config/) update language,
locale, accessibility and wrapping without replacing history. Svelte `$derived`
owns computed presentation and `$state.raw` holds immutable snapshots; effects
synchronize external state.

The target-specific `StreamLanguage` uses Lezer tags for highlighting. Completion
combines register/directive hints with deduplicated document words and is suppressed
inside comments/strings. Comment commands distinguish x86 `#` from AArch64 `//`.
This lexer is neither a complete grammar nor an instruction-validity database.
Build errors retain their complete identity. Valid UTF-8 diagnostic offsets become
UTF-16 point diagnostics through CodeMirror's lint extension, with gutter markers,
F8 navigation and a localized location button. The frontend accounts for BOM,
supplementary characters and CRLF/CR normalization without rewriting assembler
input. Editing invalidates the build; points are cleared rather than remapped onto
unverified source. Structural folding and source-to-instruction mapping remain open.
Tab leaves the editor. Indentation, search, multiple selections and bracket matching
use CodeMirror's native facilities.

Bits UI owns roving toolbar focus, tooltip dismissal, popover focus return, file
menu interactions and tab navigation. Buttons retain native `disabled` behavior.
File actions use `onSelect`; other handlers pass through the primitive to preserve
event composition. [Tabs](https://www.bits-ui.com/docs/components/tabs) retain panel
state and hide inactive content with `hidden`; CSS must preserve that behavior.
Native number inputs validate required byte offsets and integer bounds before
submission. Custom focus or form-validation layers are unnecessary.

Use semantic landmarks, labelled fields, tables and definition lists. Native CSS
uses Grid/Flexbox, logical properties, nesting, `oklch`, `color-mix` and dynamic
viewport units. Newly Baseline features are eligible subject to actual WebView
acceptance; Vite does not polyfill missing Web APIs. Follow
[HTML semantics](https://html.spec.whatwg.org/multipage/) and
[CSS specifications](https://www.w3.org/Style/CSS/Overview.en.html).

JetBrains Mono defaults to 14px with ligatures disabled; preferences allow system
monospace, 12–22px, wrapping and panel proportions. Settings do not enumerate local
fonts. Focus mode hides inspectors without unmounting the editor or disconnecting
the worker. Narrow layouts, zoom, visible focus and reduced motion require visual
and keyboard acceptance.

The observation area includes static instruction inspection. Planned expansion
adds multiple documents and coordinated instructions in the center, watches on the
right, and diagnostics/traces below. Introduce navigators only when collections
exist, and coordinate source/instruction selection through real provenance. Do not
add inert controls or empty views for planned capabilities. The layout draws on
[VS Code](https://code.visualstudio.com/docs/getstarted/userinterface) and
[Binary Ninja](https://docs.binary.ninja/guide/index.html) while keeping this workflow compact.

## Localization and recovery

English and Simplified Chinese ship together. Preference order is explicit stored
locale, supported system preference, then English. `Intl.Locale` validates tags
and distinguishes Hans/Hant; Traditional Chinese is not silently treated as Simplified.
[Paraglide](https://paraglidejs.com/strategy) messages, document language, editor
phrases and accessible names update without reloading or losing edits/search state.
Message calls receive the active locale explicitly. Source identifiers, assembly
syntax, register names, file extensions and standard units are not translated.
Application-supplied file-dialog titles follow the selected locale; system-owned
dialog controls follow the host's language settings.

Product copy names actions and observable state, distinguishes builds from sessions,
and gives useful recovery steps. Internal delivery counters, host paths and raw
backend diagnostics stay out of ordinary views. Both languages are edited for
natural, concise wording; keys and placeholders must agree.

Browser storage retains a bounded scratch document and validated display preferences.
Files use standard UTF-8 assembly, raw bytes and ELF; no saved-experiment container
is planned. Import replaces the current source only if it has not changed while the
picker was open. Source exports preserve BOM and newline bytes until editing;
CodeMirror edits use its normal LF representation. Binary imports are separate,
transient inspection inputs and do not replace the loaded machine.

The desktop file boundary uses Tauri's [dialog plugin](https://tauri.app/plugin/dialog/)
and bounded Rust I/O. Only the main window can invoke import/export. The chosen
path stays native; files are validated before publication, and exports use a
flushed temporary file in the destination directory followed by
[`NamedTempFile::persist`](https://docs.rs/tempfile/latest/tempfile/struct.NamedTempFile.html#method.persist).
This provides replacement atomicity, not a promise of crash durability for the
containing directory. Cancellation is a normal outcome.
No frontend filesystem scope or automatic write to an earlier path is granted.

## Execution, performance and containment

Construct each native session on its execution thread. A separate owner handles
assembly; bounded queues and independent pipe readers/writers keep controls usable.
Observations coalesce only after coherent capture. The supervisor kills and reaps
failed workers, reports uncertain outcomes and never retries an uncertain mutation.
[Protocol](protocol.md) defines the identities and delivery guarantees.

Measure assembly latency, execution throughput, IPC, rendering, startup and memory
before optimizing. Bound work and payloads first. Traces, snapshots, replay, SIMD
and static performance analysis each need explicit semantics and resource budgets.

Tauri capabilities restrict commands to the main window. The CSP permits necessary
CodeMirror style injection and self-hosted fonts, without remote scripts. Native
process isolation and sampled cutoffs are not a complete adversarial sandbox.
Installed dependency bundling, licensing, signing/JIT policy and host containment
remain [release gates](roadmap.md#release-gates).

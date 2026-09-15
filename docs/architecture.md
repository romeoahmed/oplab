# Architecture

Oplab is a desktop assembly workbench for x86_64 and little-endian AArch64.
Its working loop is source → standard ELF → loaded machine → observable effects.
Raw bytes can enter the same loading/execution path with explicit placement.
[Roadmap](roadmap.md) separates implemented capabilities from the planned debugger.

## Ownership

```text
Svelte workbench → Tauri supervisor → isolated worker
                                      ├─ assembly owner: LLVM MC → ELF object → LLD → ELF image
                                      └─ execution owner: validated image/setup → Unicorn → observations
```

| Location                      | Responsibility                                                                             |
| ----------------------------- | ------------------------------------------------------------------------------------------ |
| `crates/core`                 | Validated domain types, memory/execution policy and wire contracts; no native dependencies |
| `crates/engine`               | Assembly, linking, decoding/analysis, loading, sessions, worker and CLI                    |
| `src-tauri`                   | Window-scoped commands, worker supervision, bounded delivery and user-selected file I/O    |
| `src/lib/workbench`           | Document controller, editor, machine, memory and instruction presentation                  |
| `src/lib/desktop`             | Sole frontend entry to Tauri; no native execution in browser preview                       |
| `src/lib/protocol`            | Rust-generated declarations and pure framing/scalar/observation logic                      |
| `src/lib/i18n`, `messages`    | Locale selection and English/Simplified Chinese catalogs                                   |
| `tests`, Rust package `tests` | Behavior and invariant tests outside production directories                                |
| `examples`                    | Standard assembly shared by the workbench and native execution tests                       |
| `xtask`                       | Cross-tool verification, formatting, contract export and sidecar staging                   |

Keep state and effects with their owners, native types in engine adapters and desktop
dependencies out of core. Introduce modules for actual boundaries. Cargo, Vite,
SvelteKit and Tauri own their build lifecycles; [development](development.md) owns commands.

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
      bin/oplab-cli/          # CLI entry and batch execution
    tests/                    # Public integration targets
      common/                 # Process fixtures
      unit/                   # Private concurrency tests
examples/                     # x86_64.s and aarch64.s, loaded unchanged
src/
  lib/
    desktop/                  # Tauri IPC boundary
    i18n/                     # Locale selection
    protocol/                 # Wire operations and generated DTOs
    styles/                   # Application theme
    workbench/                # Composition, controller, scratch and preferences
      editor/                 # CodeMirror component, CSS and language support
      machine/                # Initial configuration, registers and memory views
      instructions/           # Byte windows, disassembly and static analysis
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

Keep feature code and its CSS together; root tests mirror those feature paths.
Private Rust tests use path modules under package `tests/unit`; `tests/common/mod.rs`
provides integration helpers without becoming a separate Cargo test target.
[AGENTS.md](../AGENTS.md) defines naming and editing conventions.

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

A functional core validates inputs and computes state transitions. Native and UI
owners perform effects. Rust newtypes and tagged enums enforce domain constraints;
TypeScript uses strict unions, exhaustive switches and validation of `unknown` inputs.

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

LLVM MC's C++ interface owns assembly; Oplab does not reconstruct object/linker
behavior around an instruction emitter. Dependency changes must remove real
complexity and fit the contract. Check official documentation and resolved source;
root manifests own requirements and lockfiles own exact resolutions.

## Interface

The source editor, right-hand machine panel and bottom observations remain visible
through the edit/build/execute loop. Configuration distinguishes build inputs from
load inputs: changing the link address requires assembly; completion, budget,
initial GPRs and extra mappings apply on load. Per-target setup survives target and
locale switches for the current window without mutating an artifact or session.

Raw-code configuration owns its target, base, entry and completion; per-target
GPR/mapping setup and the instruction budget are shared with source loading.
Each load captures its bytes and settings before awaiting IPC. Later edits remain
editable inputs and cannot alter the pending load or the machine it creates.
Code freshness compares the source build identity or raw bytes/target/base/entry.
Completion, budget and initial conditions apply on load; freshness does not compare
those settings with the active session.
Configuration components require parent-owned bindings. Defaults live in the
workbench/controller; child components do not initialize competing state.

### Editor and diagnostics

CodeMirror loads through a [Svelte await block](https://svelte.dev/docs/svelte/await).
An [attachment](https://svelte.dev/docs/svelte/@attach) owns its lifetime;
[compartments](https://codemirror.net/examples/config/) reconfigure language, locale
and wrapping while preserving history. Svelte derives presentation from immutable
snapshots; effects synchronize external state.

A target-specific `StreamLanguage` and Lezer tags provide lexical highlighting.
Built-in programs live in `examples/`: Vite imports them with `?raw`, and Rust
execution tests use `include_str!`. No generated copy or custom asset loader is needed.
Completion combines register/directive hints and document words, excluding comments
and strings. Native CodeMirror commands handle search, history, comments, indentation,
bracket matching and multiple selections. Tab leaves the editor. LLVM alone validates
assembly; structural folding and source-to-instruction mapping remain planned.

Build-scoped UTF-8 offsets become validated UTF-16 point diagnostics, including
BOM, supplementary characters and newline normalization. The lint extension owns
markers and F8 navigation. Source edits clear diagnostics instead of remapping them
onto unverified text. The original assembly input remains unchanged.

### Controls and layout

Bits UI owns toolbar, tooltip, popover, menu and tab interactions. Native inputs
own constraint validation and submission; related shortcuts use `requestSubmit()`
to follow the same path. Buttons retain `disabled`, and library event/attachment
composition remains intact. Bits UI keeps inactive tab panels hidden without losing state.
Use semantic landmarks, labelled fields, tables and definition lists.

A compact application header combines files, execution and settings. Below 1200px
it moves execution controls to a second row. Window decorations and controls remain
native; macOS retains Tauri's default application menu. Tauri's
[window menu](https://tauri.app/learn/window-menu/) API provides menus, not a portable
native toolbar. Platform toolbar bridges and
[overlay titlebars](https://tauri.app/learn/window-customization/) are not implemented.

Native CSS uses Grid/Flexbox, logical properties, nesting, `oklch`, `color-mix`,
container queries and dynamic viewport units. Newly Baseline features require
actual WebView acceptance; Vite does not polyfill missing Web APIs.

The default window is 1440×900 logical pixels, fitted to the monitor work area by
Tauri's `preventOverflow`, with an 880×600 minimum. The machine panel starts at
28% width and the observation panel at 40% of viewport height. Minimum row sizes
protect the editor and observations. At 800px and below, panels stack vertically;
forms reflow with Grid and the tab strip scrolls without compressing its controls.
This also accommodates zoomed WebViews without JavaScript resize handlers. Raw-code
loading stays beside the panel heading, ahead of optional setup. Existing saved
panel sizes remain; Reset layout applies the defaults without changing other preferences.
Focus mode hides inspectors without unmounting the editor or disconnecting the worker.

JetBrains Mono defaults to 14px with ligatures disabled. Preferences offer system
monospace, 12–22px text and wrapping; the app does not enumerate installed fonts.
Visual and keyboard acceptance covers narrow layouts, zoom, both locales and long
values. The layout takes cues from
[VS Code](https://code.visualstudio.com/docs/getstarted/userinterface) and
[Binary Ninja](https://docs.binary.ninja/guide/index.html).

### Instruction inspection

A derived input identity invalidates pages and selection when bytes, target, base
or connection changes, including reversions. Unrelated prop or locale changes
retain results. Selection/retry events request analysis; a keyed await block owns
pending, error and result presentation. Re-decoding clears the selection.

Lists and analysis scroll independently. Below 760px of content width, a CSS
container query gives analysis the panel; Close restores the retained list without
another decode. Static analysis remains separate from live machine observations.

Observed memory is a separate instruction source with its captured target, address
and PC. Captures belong to a connection, session and generation, and disappear on
replacement or reset. Identical bytes retain decoded rows and selection across
observations; control replies without memory do not erase the last capture. The PC
marker belongs to that capture, not an inferred source location. The memory table
uses the same retained capture. Manual inspection and Inspect at PC share native
form validation for the desktop's 1–4,096-byte window; the engine still validates
that the requested range lies in one mapping.

Address breakpoint controls use the latest authoritative observation. They are
editable only in ready/paused states, retain the engine's reset semantics and never
optimistically publish a change. Static artifact/imported rows cannot change session
breakpoints; only rows from the bound memory capture expose those controls.

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
transient raw inputs; only the explicit load action replaces the machine.

The desktop file boundary uses Tauri dialogs and bounded Rust I/O. Paths stay
native; cancellation is normal. Exports validate contents before same-directory
temporary-file replacement. This provides atomic replacement, not directory crash
durability. No frontend filesystem scope or implicit write-back path is granted.
The [file contract](protocol.md#desktop-file-boundary) owns formats and limits.

## Execution, performance and containment

The CLI owns a session on its calling thread; the worker separates assembly,
execution and blocking pipe I/O. Both share loading and execution policy. Native
sessions are constructed on their owning threads; bounded queues keep controls
independent of assembly. Coherent observations coalesce before delivery.

The supervisor kills and reaps failed workers, reports uncertain outcomes and never
retries uncertain mutations. [Protocol](protocol.md) defines identities, deadlines,
resource cutoffs and delivery guarantees. These measures are not a complete
adversarial sandbox.

Tauri capabilities restrict commands to the main window. The CSP permits necessary
CodeMirror style injection and self-hosted fonts without remote scripts. Measure
assembly, execution, IPC, rendering, startup and memory before optimizing.
Native dependency bundling, licensing, signing/JIT policy and host containment remain
[release gates](roadmap.md#release-gates).

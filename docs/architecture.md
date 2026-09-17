# Architecture

Oplab separates editable source, immutable ELF artifacts and a mutable loaded
machine. The [engine contract](engine.md) defines their semantics;
[roadmap](roadmap.md) distinguishes implemented and planned capabilities.

## Ownership

```text
Svelte workbench → Tauri supervisor → isolated worker
                                      ├─ assembly: LLVM MC → object → LLD → image
                                      └─ execution: validated setup → QEMU/LLVM → observations
```

| Owner                      | Responsibility                                                       |
| -------------------------- | -------------------------------------------------------------------- |
| `crates/core`              | Pure domain validation, memory/execution policy and wire contracts   |
| `crates/toolchain`         | LLVM MC/LLD assembly and XED/MC static inspection                    |
| `crates/runtime`           | QEMU state, TCG lowering, LLVM ORC and compiled-code cache           |
| `crates/engine`            | Validated loading, machine policy and sessions                       |
| `crates/runner`            | Worker concurrency, delivery and CLI                                 |
| `src-tauri`                | Window-scoped supervision, bounded IPC and selected-file I/O         |
| `src/lib/workbench`        | Document state, editor, machine and instruction presentation         |
| `src/lib/desktop`          | Sole frontend entry to Tauri APIs                                    |
| `src/lib/protocol`         | Generated DTOs and pure framing/scalar/observation logic             |
| `src/lib/i18n`, `messages` | Locale selection and bilingual catalogs                              |
| `xtask`                    | SDK construction, cross-tool checks, formatting, codegen and staging |

Native types stay in their adapters. Core has no native dependencies; browser
preview does not simulate execution. llvm-sys owns LLVM discovery and linkage for
ORC and CXX through Cargo metadata. [Runtime](runtime.md) owns native lifecycle
and distribution design; [development](development.md) owns build instructions.

## Repository layout

Use conventional [SvelteKit](https://svelte.dev/docs/kit/project-structure),
[Tauri](https://tauri.app/start/project-structure/) and
[Cargo](https://doc.rust-lang.org/cargo/guide/project-layout.html) entry points:

```text
crates/
  core/                 # Domain and authoritative wire DTOs
  toolchain/            # Safe APIs, CXX adapters and build metadata
  runtime/              # QEMU adapter, CPU state and LLVM execution
  engine/               # Loading, machine policy and sessions
  runner/               # Worker and CLI
examples/               # Assembly shared unchanged by UI and native tests
src/
  lib/
    desktop/            # Tauri boundary
    i18n/               # Locale selection
    protocol/           # Wire transformations and generated DTOs
    styles/             # Application theme
    workbench/          # Composition, controller, recovery and preferences
      editor/           # CodeMirror, language support and CSS
      load/             # Raw-code and initial-condition inputs
      machine/          # Live registers, memory and breakpoints
      instructions/     # Disassembly and static analysis
  routes/               # SvelteKit entry points
src-tauri/              # Shell, supervisor, capabilities and icons
static/                 # Application icon master
messages/               # English and Simplified Chinese
project.inlang/         # Localization configuration
tests/                  # Frontend tests mirroring feature ownership
xtask/                  # Repository tooling
```

Runtime `qemu.rs` owns the SDK boundary; `jit.rs` and `jit/` own LLVM lowering.
Runner `worker/dispatch.rs` owns scheduling; `worker/execution.rs` owns the session.
Toolchain build support separates discovery (`build/sdk.rs`) from compiler setup
(`build/cpp.rs`). xtask separates SDK, sidecar and Clang tooling.

Keep components, CSS and pure helpers beside their feature. Rust public tests use
package `tests/`; private path modules use `tests/unit`. `tests/common/mod.rs`
shares fixtures without creating another integration target. Naming and editing
rules live in [AGENTS.md](../AGENTS.md).

## Standards and state

LLVM owns source parsing, macros, fixups, relaxation and ELF generation; LLD owns
relocations and layout. Source passes unchanged. Protocol metadata describes
standard ELF rather than redefining it.

| Lifetime | Identity and behavior                                                                |
| -------- | ------------------------------------------------------------------------------------ |
| Document | Source, target and human inputs, including unfinished fields                         |
| Artifact | Immutable build keyed by document, revision, target, base and assembler identity     |
| Session  | Retained initial image/setup plus mutable machine state; load replaces it explicitly |

A source edit cannot patch the machine. Loading captures inputs before awaiting IPC;
later edits cannot change that request. Reset advances generation only after
replacement succeeds. Reattachment cannot infer which source produced a retained
session. Code freshness compares build identity or raw bytes/target/base/entry,
independently of live edits and load settings.

Pure transformations own validation; explicit owners perform effects. Rust uses
newtypes and tagged enums; TypeScript uses validated inputs and exhaustive unions.
[Wire scalars](protocol.md#negotiation-and-identity) preserve exact values. Missing
memory is distinct from zero, and uncertain mutations are never replayed.

## Technology decisions

| Technology                                  | Role                                                               |
| ------------------------------------------- | ------------------------------------------------------------------ |
| Tauri 2, Svelte 5, SvelteKit static adapter | Desktop lifecycle and reactive SPA without a bundled server        |
| LLVM 23 MC/LLD, CXX, C++23                  | Whole-document assembly/linking with call-scoped FFI borrows       |
| QEMU, llvm-sys                              | CPU semantics, Rust TCG lowering and LLVM ORC execution            |
| Intel XED, LLVM MC AArch64                  | Independent decoding and conservative static metadata              |
| `object`                                    | ELF inspection; the engine validates the supported runtime subset  |
| CodeMirror 6, Lezer                         | Editor transactions, lexical assistance, completion and navigation |
| Bits UI                                     | Composite interactions; native HTML fields/forms elsewhere         |
| Paraglide                                   | Compiled English/Simplified Chinese messages                       |
| Native CSS, Lucide, Fontsource              | Layout, icons and offline JetBrains Mono                           |
| clap                                        | CLI/xtask arguments and help                                       |
| Proptest, fast-check, Vitest Browser        | Invariants and real-browser behavior                               |

Manifests own requirements; lockfiles own exact resolutions. SDK tags live in
xtask. Add dependencies when they remove concrete complexity. Static recognition
and editor suggestions do not imply execution support.

## Interface

### Editor and diagnostics

A Svelte await block loads CodeMirror; an [attachment](https://svelte.dev/docs/svelte/@attach)
owns cleanup. [Compartments](https://codemirror.net/examples/config/) reconfigure
language, locale and wrapping without replacing history. Documents start blank or
restore their last valid draft, including an intentionally empty one. Examples load
explicitly through Vite raw imports and Rust `include_str!`.

Target-specific StreamLanguage tokens and Lezer tags distinguish GNU symbols,
numeric references, macro parameters, directives, operands and SIMD arrangements.
Completion combines target hints with literal labels. A syntax-tree index provides
GNU block folding and F12 label navigation. Explicit navigation may request a
bounded full parse; ambiguous or incomplete input does not produce a guessed jump.
Macro expansion and instruction provenance remain LLVM responsibilities.

CodeMirror owns search, replacement, line syntax, history, comments, indentation,
bracket matching and selections. Its panel lifecycle mounts Svelte search forms;
Bits UI owns query toggles and replacement disclosure. Function bindings update
CodeMirror's query. Search and line navigation share an upper-right overlay without
shifting source. Locale changes preserve drafts and refresh labels. Toolbar actions
return focus to the editor; Tab leaves it. Assembly shortcuts do not insert text.

Diagnostics belong to one build. Original UTF-8 offsets become validated UTF-16
points, accounting for BOM, supplementary characters and normalized newlines.
CodeMirror lint owns markers/navigation. Edits clear diagnostics; absent or invalid
offsets never become fabricated locations. Points are not source maps.

### Controls and layout

Bits UI owns toolbars, tooltips, tabs, menus and popovers. Native forms own validation
and submission, including shortcuts through `requestSubmit()`. Preserve native
`disabled`, library attachments and event composition. Hidden panels retain state;
semantic fields, tables and definition lists describe content.

The default window is 1440×900 logical pixels with an 880×600 minimum and Tauri
`preventOverflow`. Decorations remain native; macOS retains the default application
menu. The machine panel starts at 28% width (320px minimum); bottom observations
start closed and open on assembly or binary import, at 32% height (180px minimum).
Closing or Mod-J toggling retains contents. Focus mode hides inspectors without
unmounting the editor or disconnecting the worker.

Grid, Flexbox, logical properties and container queries handle layout. Forms wrap;
panels stack at 800px and below. Main actions use 32px targets, panel tools 28px
and search options 24px. Familiar secondary actions use icons with tooltips;
ambiguous memory actions retain text. JetBrains Mono defaults to 14px without
ligatures; preferences offer system monospace, 12–22px text, wrapping and panel
proportions. Layout reset changes only proportions. Actual WebView acceptance is
required for Newly Baseline features.

### Inspection and editing

Disassembly identity includes bytes, target, base and connection. Changes invalidate
rows and selection even when reverted; locale changes retain results. Selection
requests static analysis. Narrow views return from analysis without decoding again.

Captured memory retains its session, generation, address and register snapshot.
Replies without bytes preserve it; reset/replacement clears it. Identical bytes
retain decoded rows. The PC marker belongs to the capture, not source provenance.
Only captured-memory rows can toggle breakpoints or set the next instruction.

SIMD views project active SVE lengths from maximum-width storage. `BigInt` and
`DataView` interpret exact integers and f16/f32/f64; native `details` lazily creates
expanded lanes. Predicate views select the significant bits for `.b/.h/.s/.d`.
Architecture changes reset bank selection; ordinary observations preserve view
choices and drafts. These banks are not complete CPU snapshots.

Ready/paused edits await authoritative replies. Rust validates widths, alignment,
state and native merges; observations do not overwrite a typed SIMD draft.
Successful memory writes invalidate captured bytes and request a fresh window.
Edits leave source, artifacts and reset inputs unchanged; [engine](engine.md#live-editing)
owns exact write semantics.

## Localization and recovery

Locale order is stored choice, supported system preference, then English.
`Intl.Locale` distinguishes Hans/Hant. Paraglide, document language, CodeMirror
phrases and accessible names update without reload. Source, identifiers, file
extensions and standard units remain untranslated. App dialog titles use the
selected locale; native controls follow the host.

Browser storage retains validated, bounded scratch and display preferences.
Invalid/oversized drafts preserve the last recoverable copy while current text stays
editable. Import replaces source only if it has not changed while the picker was
open. BOM/newlines survive export until editing; CodeMirror edits use LF. Binary
import never loads implicitly. Files use standard text, raw bytes and ELF; no
saved-experiment container is planned.

Native dialogs select paths; paths never reach the frontend. Bounded I/O validates
exports before atomic same-directory replacement. There is no implicit write-back
or frontend filesystem scope. [Protocol](protocol.md#desktop-file-boundary) owns
file limits and failure guarantees.

## Execution, performance and containment

The CLI owns a session on its calling thread. The worker separates assembly,
execution and blocking I/O through bounded queues and complete observations.
The supervisor kills/reaps failed workers and reports uncertain outcomes.
[Protocol](protocol.md) defines delivery, resource cutoffs and recovery.

Tauri capabilities restrict commands to the main window. CSP permits CodeMirror
style injection and self-hosted assets without remote scripts. These controls do
not form a complete adversarial sandbox. Performance measurement, host containment
and independently installed distribution remain [release gates](roadmap.md#release-gates).

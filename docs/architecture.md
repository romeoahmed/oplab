# Architecture

Oplab is an assembly workbench for x86_64 and little-endian AArch64. Its purpose is
an explicit, inspectable loop: edit source, assemble a standard artifact, configure
an experiment, execute it, and inspect effects. The current desktop loop includes
assembly, ELF loading, execution controls, integer registers, memory, and English
and Simplified Chinese interfaces. [Roadmap](roadmap.md) separates that implemented
subset from the complete debugging product.

## Boundaries

```text
Svelte workbench → Tauri commands → process supervisor → worker
                                                        ├─ LLVM MC → ELF object → LLD → ELF image
                                                        └─ ELF loader → Unicorn session
```

| Owner                      | Responsibility                                                                           |
| -------------------------- | ---------------------------------------------------------------------------------------- |
| `crates/oplab-core`        | Validated addresses, memory and execution rules, wire contracts; no native dependencies. |
| `crates/oplab-engine`      | Assembly, linking, decoding, loading, execution, worker and CLI entry points.            |
| `src-tauri`                | Window-scoped commands, worker lifetime, deadlines, connection leases, IPC delivery.     |
| `src/lib/workbench`        | Editor and inspector presentation; one document and one loaded session.                  |
| `src/lib/desktop`          | The only frontend access to Tauri. Browser preview exposes no native execution.          |
| `src/lib/protocol`         | Generated declarations and pure framing, scalar, and observation functions.              |
| `src/lib/i18n`, `messages` | Locale selection and source catalogs.                                                    |
| `tests`, `*/tests`         | Frontend, core, engine, and desktop tests, outside production source directories.        |
| `xtask`                    | Cross-tool verification, formatting, contract generation, and sidecar staging.           |

Keep each abstraction with its owner. Add crates or frontend modules when an actual
boundary warrants them; do not create empty service, repository, adapter, or shared
layers. Native types stay inside engine adapters. Desktop dependencies never enter
core. Cargo, SvelteKit, Vite, and Tauri retain their ordinary build responsibilities.

## Standards and state

Assembly syntax is LLVM's GNU-style language: Intel operands initially on x86_64,
native LLVM/GNU syntax on AArch64. This is a toolchain contract, not universal
NASM/MASM compatibility. Pass whole source documents unchanged. LLVM owns parsing,
macros, fixups, relaxation, and ELF generation; LLD owns final relocation and layout.
The [engine contract](engine.md) defines the accepted language and runtime subset.

Keep standard objects intact. Protocol metadata describes artifacts and operations;
it never replaces ELF sections, relocations, symbols, or program headers. Human
inputs accept ordinary hexadecimal addresses. Exact fixed-width scalar formatting
belongs at the JSON boundary. Code and data are interpreted using guest addresses
and target byte order, independently of the host.

Distinguish three lifetimes:

- A document contains editable source and human inputs, including incomplete fields.
- An artifact captures document identity, revision, target, base, and assembler identity.
- A session owns an immutable initial image and mutable machine state with a generation.

A late build can never silently replace a newer document. Editing source does not
patch a running machine. Reset replaces native state from the initial image only
after replacement succeeds. Reconnection cannot invent a source association for a
retained session. A future saved experiment will retain source, setup, policies,
assertions, and backend identity; live registers alone are not a complete snapshot.

## Implementation style

Use a functional core with explicit effect owners. Rust newtypes and enums enforce
ranges, transitions, and protocol variants; borrowed slices express read-only data.
TypeScript uses strict structural types, tagged unions, exhaustive switches,
`unknown` validation, and pure transformations. Avoid parallel copies of derived
state, opaque generic frameworks, and casts that bypass boundary validation.
See [TypeScript for functional programmers](https://www.typescriptlang.org/docs/handbook/typescript-in-5-minutes-func.html)
and the [Rust reference](https://doc.rust-lang.org/stable/reference/).

Svelte runes own reactive presentation. Use `$derived` for derivable values,
`$state.raw` for immutable snapshots, and effects for external synchronization.
The CodeMirror attachment owns one editor and its cleanup; compartments update
locale and accessibility without destroying history. The workbench controller owns
request identity, subscription lifetime, and scratch persistence. Separate state
only when its lifetime or invariants differ. Follow [Svelte attachments](https://svelte.dev/docs/svelte/@attach)
and [runes](https://svelte.dev/docs/svelte/what-are-runes).

Native sessions are constructed on their owning execution thread. Assembly uses a
separate owner; blocking pipe I/O and monitoring have explicit lifetimes. Bounded
queues preserve control replies, reserve native result capacity, and coalesce
observations. The supervisor kills and reaps failed workers, reports uncertain
outcomes, and never retries an uncertain mutation. [Protocol](protocol.md) defines
ordering, flow control, and identity checks.

## Technology choices

| Choice                                        | Reason and boundary                                                                                                                                                                                       |
| --------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Tauri 2 + Svelte 5 + SvelteKit static adapter | Native desktop lifecycle with a compact reactive interface and Vite HMR; no frontend server in a bundle.                                                                                                  |
| CodeMirror 6                                  | Composable editor state, transactions, history, search, and locale compartments. Current work does not need Monaco's language-service and worker integration. Revisit only for a concrete capability gap. |
| LLVM 23 MC + matching LLD through CXX         | Full-document assembly and complete ELF artifacts with standard relocation. A small C++23 adapter exposes the MC facilities that LLVM's C API does not.                                                   |
| Unicorn                                       | Both guest architectures under one execution API. Feature flags limit the native build; supported semantics still require independent tests.                                                              |
| iced-x86 / Capstone AArch64                   | Target-appropriate decoding without a second emulator or assembler. Decoder recognition is separate from execution support.                                                                               |
| `object`                                      | Standard ELF reading and inspection; loader policy remains explicit.                                                                                                                                      |
| Paraglide                                     | Compiled, typed bilingual messages and explicit locale selection.                                                                                                                                         |
| Native CSS + `@lucide/svelte`                 | Platform layout and controls with a consistent, tree-shaken icon family.                                                                                                                                  |
| clap                                          | Declarative, typed CLI and xtask commands with generated help and usage errors.                                                                                                                           |
| Proptest / fast-check / Vitest Browser        | Invariant testing and real Chromium component interactions.                                                                                                                                               |

Inkwell and llvm-sys primarily expose LLVM IR/the C API; they do not remove the MC
C++ boundary. Nyxstone is a useful runtime assembly wrapper, but preserving complete
standard object/linker behavior is the decisive requirement here. asm-rs and JIT
emitters would require implementing or restricting more of the language and
relocation surface. Oplab therefore has one direct MC/LLD backend, without legacy
fallbacks. Prefer maintained dependencies that remove real complexity, assessed by
API fit, release activity, documentation, licenses, and verified behavior rather
than popularity alone. Reevaluate when those facts change.

Dependency requirements live in root manifests; lockfiles record exact resolved
versions. Rust requirements use minor notation with Cargo's compatible-version
semantics, not an upper bound on that minor. The stable channel selects Rust;
`rust-version` declares the minimum. LLVM's C++ API requires a deliberately selected
major and matching LLD, with semantic and packaging verification on upgrades.

## Interface and localization

The editor is the primary work area; nearby controls separate assembly from loading
and execution. State, faults, stale artifacts, and unavailable actions must be
visible. Register and memory views use tabular numerals, bounded windows, and stable
layout. Distinguish missing memory from zero, raw flags from defined flags, and
static instruction effects from observed effects. Do not advertise unwritten features.

Use semantic buttons, labelled inputs, native selects, tables, headings, and regions.
Reserve navigation for navigation. Icons accompany clear labels; decorative SVGs
are hidden from assistive technology. Maintain focus visibility, keyboard access,
comfortable targets, reduced-motion behavior, and text/zoom resilience. Future
split panes must support keyboard resizing; large data views need measured,
accessible virtualization. [HTML semantics](https://html.spec.whatwg.org/multipage/)
and [CSS specifications](https://www.w3.org/Style/CSS/Overview.en.html) are the baseline.

Use native logical properties, Grid/Flexbox, nesting, modern color functions, and
other Newly Baseline capabilities where the selected system WebViews support them.
Vite transforms syntax, not missing Web APIs. Declare and test host minimums before
release; add a compatibility dependency only for a demonstrated requirement.
Application styles live in CSS. CodeMirror still injects its own structural styles.

English and Simplified Chinese ship together. Locale preference follows explicit
storage, supported system preference, then English. `Intl.Locale` validates tags
and distinguishes Hans from Hant; Traditional Chinese is not silently treated as
Simplified Chinese. Paraglide messages, document language, editor phrases, and
accessible names update without reload. Source, history, selection, open search,
and machine state survive locale changes. Assembly syntax and identifiers remain
unchanged. Catalog keys and placeholders must agree; localization failures must
not discard work. See [Paraglide strategies](https://paraglidejs.com/strategy).

## Persistence, performance, and delivery

Current recovery stores a bounded scratch document in browser storage. Full project
persistence needs a versioned format, complete validation, atomic replacement,
conflict detection, save/reopen tests, and an explicit recovery policy. Avoid a
custom container when standard source, ELF, and structured metadata suffice.

Measure representative assembly latency, cancellation, execution throughput,
observation delivery, rendering, memory, startup, and artifact size. Bound work and
payloads before optimizing copies. Add trace, snapshots, replay, SIMD, or static
performance analysis only with explicit semantics and measured resource budgets.
No performance claim follows merely from choosing Rust or a native library.

Tauri command capabilities and connection leases restrict access. The CSP permits
CodeMirror's required style injection, not arbitrary scripts. Diagnostics omit host
paths and source excerpts. Shareable artifacts require deliberate handling of
source, symbols, memory, and failure data. Release gates include native dependency
bundling, signing, licenses, installed workflows, actual system WebViews, and host
resource containment. See [testing](testing.md) and [roadmap](roadmap.md).

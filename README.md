<p align="center">
  <img src="static/icon.svg" width="112" height="112" alt="Oplab icon">
</p>

# Oplab

**Write assembly. Run it. Inspect the machine.**

Oplab is a desktop workbench for **x86_64** and **AArch64**. Edit GNU-style assembly,
build standard ELF with LLVM, and explore execution through registers and memory.
The interface is available in English and Simplified Chinese.

[Build and develop](docs/development.md) · [Architecture](docs/architecture.md) · [Roadmap](docs/roadmap.md)

## What works

- **Edit:** CodeMirror highlighting, register/directive and document-word completion,
  search, undo, comment commands, bracket matching and multiple selections.
  Assembly diagnostics mark and reveal verified source positions.
- **Build:** Whole-document LLVM MC assembly and LLD linking, preserving standard
  sections, symbols and relocations in complete ELF artifacts.
- **Execute:** Load, step, run, pause, stop and reset an isolated Unicorn session.
  Source edits stay separate from the loaded program.
- **Inspect:** Integer registers, flags, memory and artifact metadata; bounded
  disassembly of imported machine code or individual ELF segments. Select an
  instruction for static register/memory effects, control flow and architecture metadata.
- **Files:** Import/export UTF-8 assembly and exact machine bytes; export complete
  ELF objects and images through native dialogs.
- **Adjust:** Self-hosted JetBrains Mono or system monospace, font size, wrapping,
  panel proportions and focus mode; local draft/settings recovery.

Source-to-instruction mapping, editable machine setup, multiple documents and CLI
execution remain planned. Imported raw bytes support inspection only; execution
loads an assembled ELF image. See the [current scope](docs/roadmap.md).

## Get started

For **frontend development**, install Node satisfying `package.json` and pnpm:

```sh
pnpm install --frozen-lockfile
pnpm dev
```

Open Vite's local URL. Browser preview supports editing and interface development;
assembly and execution require the desktop app.

For **desktop development**, follow the [native build setup](docs/development.md)
for Rust, LLVM/LLD 23, C++23 and Tauri's platform prerequisites. Stop the standalone
Vite server, then run:

```sh
pnpm tauri dev
```

Tauri hooks build and stage the worker and start the frontend automatically.
Both guests have been exercised in an Apple Silicon macOS debug app. Windows/Linux
and independently installed/signed distributions remain release gates; a local
bundle may still depend on installed LLVM/LLD shared libraries.

## Try an experiment

1. Choose an architecture and **Load example**.
2. **Assemble**, then **Load artifact**.
3. **Step** (`F10`) or **Run** (`F5`). The example stores 42 in memory.
4. Inspect registers and memory; **Reset** restores the loaded initial state.

Use `Ctrl+Enter` / `⌘+Enter` to assemble. Configuration sets the link address, stop
address/symbol and instruction limit. A link-address change needs a new build;
stop position and limit apply when loading. The [engine contract](docs/engine.md)
explains GNU syntax, alignment and execution boundaries.

## Contribute

Read the [development guide](docs/development.md) for setup and focused commands.
Repository-wide verification uses Cargo xtask:

```sh
pnpm exec playwright install --with-deps --no-shell chromium
cargo xtask check
cargo xtask test
```

`cargo xtask fmt` formats source and documentation. Tests should verify observable
behavior and independent invariants. Include relevant validation and update both
language catalogs and the documentation when behavior changes.

## Project guide

| Document                             | Covers                                                            |
| ------------------------------------ | ----------------------------------------------------------------- |
| [Development](docs/development.md)   | Build requirements, native discovery, commands, outputs and icons |
| [Architecture](docs/architecture.md) | Repository layout, ownership, technology and interface            |
| [Engine](docs/engine.md)             | Assembly, instruction analysis, ELF loading and execution         |
| [Protocol](docs/protocol.md)         | Framing, sessions, subscriptions, supervision and CLI             |
| [Testing](docs/testing.md)           | Test design, browser/native acceptance and evidence limits        |
| [Roadmap](docs/roadmap.md)           | Implemented work, next capabilities and release gates             |

Built with Tauri, Svelte, LLVM/LLD, Unicorn, CodeMirror/Lezer and Bits UI. Inspired
by [cemu](https://github.com/hugsy/cemu), with a newly designed architecture and interface.

## Author and license

Created by [Romeo Ahmed](https://github.com/romeoahmed). Copyright © 2026 Romeo Ahmed.

Licensed under either the [MIT license](LICENSE-MIT) or the
[Apache License, Version 2.0](LICENSE-APACHE), at your option (`MIT OR Apache-2.0`).
Unless explicitly stated otherwise, contributions are provided under the same terms.
Dependencies and bundled fonts retain their own licenses and notices.

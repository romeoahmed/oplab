<p align="center">
  <img src="static/icon.svg" width="112" height="112" alt="Oplab icon">
</p>

# Oplab

**Write assembly. Run it. Inspect the machine.**

A desktop assembly workbench for **x86_64** and **AArch64**, available in English
and Simplified Chinese. Build standard ELF with LLVM, execute it in Unicorn, and
explore registers, memory and instruction effects.

**In development; not production-ready.** Native workflows have been verified on
Apple Silicon macOS. Windows/Linux and independently installed distributions
remain [release gates](docs/roadmap.md#release-gates).

[Build and develop](docs/development.md) · [Engine guide](docs/engine.md) · [Roadmap](docs/roadmap.md)

## Explore an experiment

1. Choose an architecture and **Load example**.
2. **Assemble**, then **Load artifact**.
3. **Step** (`F10`) or **Run** (`F5`). The example stores 42 in memory.
4. Inspect the result. **Reset** restores the loaded initial state.

Use `Ctrl+Enter` / `⌘+Enter` to assemble. Configure the link address, completion
address or symbol, instruction limit, initial registers and additional memory.
Assembly, loading and execution are explicit actions; editing source leaves the
loaded machine intact.

## Capabilities

- **Edit and build:** CodeMirror highlighting, completion, search, history and
  source diagnostics; unchanged GNU-style source assembled and linked by LLVM MC/LLD.
- **Execute and inspect:** step, run, pause, stop and reset; integer registers,
  flags and memory; bounded disassembly and static instruction analysis.
- **Work with files:** import/export UTF-8 assembly and exact machine bytes;
  export complete ELF objects and images through native dialogs.
- **Automate:** run source, static ELF or raw code from stdin with explicit
  completion and execution limits; receive final state as JSON.
- **Make it comfortable:** self-hosted JetBrains Mono or system monospace,
  font size, wrapping, panel proportions, focus mode and local draft recovery.

Desktop execution loads ELF artifacts; imported raw bytes are for inspection.
Raw-code execution is available in the CLI. Verified source-to-instruction mapping,
live register/memory editing and multiple documents are [planned](docs/roadmap.md#next-product-work).
Instruction recognition does not guarantee emulator support for every extension.

## Build from source

Install Node satisfying [package.json](package.json), pnpm and the
[native toolchain](docs/development.md#native-toolchain): stable Rust, C++23,
LLVM 23 with matching LLD development files, libclang, build tools and Tauri's
platform prerequisites. Then run:

```sh
pnpm install --frozen-lockfile
pnpm tauri dev
```

Tauri builds and stages the worker and starts Vite automatically. Engine changes
require rebuilding the worker and restarting the app; see the
[development workflow](docs/development.md#commands-and-ownership).

For frontend-only work, use `pnpm dev` instead. Browser preview supports editing
and interface development but cannot assemble or execute. Use one Vite server
per checkout.

For a terminal experiment, save the [x86_64 example](docs/engine.md#language-and-layout)
as `experiment.s`, then run:

```sh
cargo run --locked -p oplab-engine --bin oplab-cli -- run source x86_64 0x1000 --until-symbol done --budget 100 < experiment.s
```

The [CLI reference](docs/protocol.md#cli) covers ELF/raw inputs, initial state,
memory output and exit codes. No stack or operating-system services are supplied
implicitly.

## Development

After installing the native toolchain and JavaScript dependencies:

```sh
pnpm exec playwright install --with-deps --no-shell chromium
cargo xtask check
cargo xtask test
```

Use `cargo xtask fmt` for formatting. Keep tests focused on behavior and invariants,
update both language catalogs, and document changes to public behavior.

| Guide                                | Contents                                                       |
| ------------------------------------ | -------------------------------------------------------------- |
| [Development](docs/development.md)   | Requirements, toolchain discovery, commands, builds and icons  |
| [Architecture](docs/architecture.md) | Repository layout, ownership and technology decisions          |
| [Engine](docs/engine.md)             | Assembly syntax, ELF loading, analysis and execution semantics |
| [Protocol and CLI](docs/protocol.md) | Framing, sessions, supervision, files and batch execution      |
| [Testing](docs/testing.md)           | Test design and browser/native acceptance                      |
| [Roadmap](docs/roadmap.md)           | Implemented scope, next work and verification limits           |

Built with Tauri, Svelte, LLVM/LLD, Unicorn, CodeMirror/Lezer and Bits UI.
Inspired by [cemu](https://github.com/hugsy/cemu).

## Author and license

Created by [Romeo Ahmed](https://github.com/romeoahmed). Copyright © 2026 Romeo Ahmed.

Licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
Unless explicitly stated otherwise, contributions use the same terms. Dependencies
and bundled fonts retain their own licenses and notices.

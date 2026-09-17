<p align="center">
  <img src="static/icon.svg" width="112" height="112" alt="Oplab icon">
</p>

# Oplab

**Write assembly. Run it. Inspect the machine.**

Oplab is a desktop assembly workbench for **x86_64** and **AArch64**, in English
and Simplified Chinese. Assemble standard ELF, run it in an isolated worker, and
inspect or edit registers, memory and instruction state.

**In development; not production-ready.** Native workflows are verified on Apple
Silicon macOS. Windows, Linux and independently installed packages remain
[release gates](docs/roadmap.md#release-gates).

[Build and develop](docs/development.md) · [Engine guide](docs/engine.md) · [Roadmap](docs/roadmap.md)

## Try a program

The editor starts blank and restores your last draft on reopening.

1. Choose an architecture and **Load example**.
2. **Assemble** (`Ctrl+Enter` / `⌘+Enter`), then **Load artifact**.
3. **Step** (`F10`) or **Run** (`F5`). Both examples brighten eight RGBA pixels,
   preserving alpha, using AVX2 or a vector-length-agnostic SVE2 loop.
4. Inspect `output` in **Memory** and checksum **4814** (`0x12ce`) in RAX or X0.
5. **Reset** restores the loaded program and initial state, retaining breakpoints.

Assembly, loading and execution are explicit actions. Editing source leaves the
loaded machine intact. Programs have an explicit completion address or symbol
and instruction budget; stacks and operating-system services are not implicit.

## What you can do

- **Edit GNU-style assembly:** CodeMirror highlighting, completion, folding,
  label navigation, search/replacement, history and source diagnostics.
- **Run ELF or raw code:** step, run, pause, stop, reset and set address breakpoints.
  Configure initial GPRs and additional memory before loading.
- **Inspect effects:** integer registers, flags, memory, disassembly and on-demand
  instruction analysis. SIMD views include YMM/Z, low XMM/V aliases and SVE P/FFR,
  with exact bits, integer lanes and f16/f32/f64 interpretation.
- **Change live state:** edit registers, aliases, PC, flags, SIMD values/lanes and
  floating-point rounding while ready or paused; patch up to 4 KiB of data or code.
- **Use standard files:** import/export UTF-8 assembly and raw machine bytes;
  export complete ELF objects and executables through native dialogs.
- **Work comfortably:** local draft recovery, offline JetBrains Mono or system
  monospace, adjustable font size, wrapping, panel proportions and focus mode.
- **Automate experiments:** run source, ELF or raw code from stdin and receive
  final state as JSON through the [CLI](docs/protocol.md#cli).

For raw execution, import bytes from **Files**, set placement in **Raw code**, then
load explicitly. For live disassembly, inspect memory and choose **Captured memory**
in **Instructions**.

Both guests use a fixed QEMU MAX runtime. AVX2 and SVE/SVE2 samples execute;
recognizing an instruction does not guarantee execution support. AVX-512 execution
is unavailable; x87 and SME matrix state are not exposed. Complete extension
coverage, source mapping and multiple documents remain [planned](docs/roadmap.md#next-product-work).

## Build from source

Install Node satisfying [package.json](package.json), pnpm and the
[native toolchain](docs/development.md#native-toolchain): stable Rust, GNU C23,
C++23, LLVM 23 with matching LLD development files, build tools and Tauri's platform
prerequisites. Follow the guide to build the QEMU/XED SDK and configure discovery,
then run from the repository root:

```sh
pnpm install --frozen-lockfile
pnpm tauri dev
```

Tauri stages the worker and starts Vite. Native changes require rebuilding the
worker and restarting the app. For frontend-only HMR, use `pnpm dev`; browser
preview cannot assemble or execute.

With the same native environment, run the bundled example from the terminal:

```sh
cargo run --locked -p oplab-runner --bin oplab-cli -- run source x86_64 0x1000 --until-symbol done --budget 10000 < examples/x86_64.s
```

## Contributing

Start with the [roadmap](docs/roadmap.md) and [development guide](docs/development.md).
Keep tests focused on observable behavior and invariants, update both language
catalogs, and document changes to contracts. With the native toolchain configured:

```sh
pnpm exec playwright install --with-deps --no-shell chromium
cargo xtask check
cargo xtask test
```

Use `cargo xtask fmt` for formatting. When reporting a failure, include the host,
tool versions, a minimal program and expected versus actual behavior.

| Guide                                | Purpose                                                   |
| ------------------------------------ | --------------------------------------------------------- |
| [Development](docs/development.md)   | Prerequisites, commands, tool discovery, builds and icons |
| [Architecture](docs/architecture.md) | Repository layout, ownership and interface design         |
| [Runtime](docs/runtime.md)           | QEMU/LLVM integration and distribution design             |
| [Engine](docs/engine.md)             | Assembly, loading, analysis and execution contracts       |
| [Protocol and CLI](docs/protocol.md) | Worker transport, desktop boundary and batch execution    |
| [Testing](docs/testing.md)           | Test design and browser/native acceptance                 |
| [Roadmap](docs/roadmap.md)           | Implemented scope, priorities and verification limits     |

Built with Tauri, Svelte, LLVM/LLD, QEMU, Intel XED, CodeMirror/Lezer and Bits UI.
Inspired by [cemu](https://github.com/hugsy/cemu).

## Author and license

Created by [Romeo Ahmed](https://github.com/romeoahmed). Copyright © 2026 Romeo Ahmed.

Licensed under [MPL-2.0](LICENSE). Unless stated otherwise, contributions use the
same terms. Dependencies and fonts retain their own licenses and notices;
[native distribution licensing](docs/development.md#distribution-licensing) remains
under review.

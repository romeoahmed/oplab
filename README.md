# Oplab

A desktop assembly workbench for x86_64 and AArch64 using Rust, Tauri 2, Svelte 5,
LLVM MC/LLD, and Unicorn.

Edit standard GNU-style source, assemble and load ELF, step/run/reset a session,
and inspect coherent integer registers and memory in English or Simplified Chinese.
Source edits remain separate from the loaded machine. Source debugging, full
project persistence, ABI experiments, and installed cross-platform distribution
remain in the [roadmap](docs/roadmap.md).

## Development

Install stable Rust, a maintained Node release satisfying `package.json`, pnpm,
[Tauri prerequisites](https://tauri.app/start/prerequisites/), and the
[native toolchain](docs/development.md#native-toolchain).

```sh
pnpm install --frozen-lockfile
pnpm exec playwright install --with-deps --no-shell chromium
pnpm tauri dev
```

Use `pnpm dev` for frontend-only HMR and browser preview. Use one development server
per checkout. Repository-wide commands are `cargo xtask check`, `cargo xtask test`,
and `cargo xtask fmt`; `cargo xtask --help` describes the command interface.

## Documentation

- [Architecture](docs/architecture.md): scope, ownership, technology choices, UI and localization.
- [Engine](docs/engine.md): assembly, ELF loading, and execution semantics.
- [Protocol](docs/protocol.md): worker/desktop framing, identities, flow control, and CLI.
- [Development](docs/development.md): setup, commands, build ownership, configuration, icons.
- [Testing](docs/testing.md): evidence, meaningful tests, browser/native acceptance, and limits.
- [Roadmap](docs/roadmap.md): implemented work, remaining capabilities, and verification status.

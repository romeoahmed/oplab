# Development

Use stable Rust with the components in `rust-toolchain.toml`, a maintained Node
release satisfying `package.json`, pnpm, and the platform's
[Tauri prerequisites](https://tauri.app/start/prerequisites/). Native engine builds
also require LLVM 23, matching LLD development libraries, a C++23 compiler, CMake,
pkg-config, and libclang for Unicorn's bindings. Keep local paths outside tracked
configuration and documentation.

## Commands

```sh
pnpm install --frozen-lockfile
pnpm exec playwright install --with-deps --no-shell chromium
pnpm tauri dev
```

| Command                           | Purpose                                                                |
| --------------------------------- | ---------------------------------------------------------------------- |
| `pnpm dev`                        | Vite HMR and browser preview. Native execution is unavailable.         |
| `pnpm tauri dev`                  | Stage the worker, start Vite, and run the desktop application.         |
| `pnpm build` / `pnpm preview`     | Build / preview the static frontend.                                   |
| `pnpm tauri build`                | Stage the matching worker and build/package the application.           |
| `cargo xtask check`               | Clippy, clang-tidy, web types/lints, Knip, contract drift, formatting. |
| `cargo xtask test [--release]`    | Rust workspace and Vitest logic/browser tests.                         |
| `cargo xtask fmt [--check]`       | Apply / verify rustfmt, Oxfmt, and clang-format.                       |
| `cargo xtask codegen [--check]`   | Export / verify TypeScript contracts without LLVM.                     |
| `cargo xtask sidecar [--release]` | Build and stage the worker without launching the app.                  |

`cargo xtask --help` and subcommand help describe options. Direct `pnpm check`,
`pnpm lint`, `pnpm test`, Vitest `--project logic|browser`, and ordinary Cargo test
filters remain available. Native execution tests need a host that permits Unicorn
JIT operations. Checks do not install dependencies, update lockfiles, stage files,
or rewrite source; installation, formatting, and generation are explicit actions.

Reuse one Vite server per checkout for browser visual and interaction acceptance.
Stop a standalone server before `pnpm tauri dev`, which starts its own. Components
and catalogs use HMR. Engine changes require rebuilding the sidecar and restarting
the app; Tauri's application watcher does not rebuild an independent worker.
Validate final behavior in a static application bundle as well as development.

## Native toolchain

The validated native release is LLVM/LLD 23.1.1. The build accepts LLVM 23 releases
without a patch pin and requires matching LLVM/LLD release identities and coherent
headers, libraries, and C++ ABI. See [LLD's package requirement](https://github.com/llvm/llvm-project/blob/llvmorg-23.1.1/lld/cmake/modules/LLDConfig.cmake.in).

| Variable         | Selection                                | Default                                                                   |
| ---------------- | ---------------------------------------- | ------------------------------------------------------------------------- |
| `LLVM_CONFIG`    | Host-runnable discovery executable       | `llvm-config` on `PATH`                                                   |
| `LLD_PREFIX`     | LLD headers, libraries, and `bin/ld.lld` | LLVM prefix                                                               |
| `LLVM_LINK_KIND` | `dylib` or `static`                      | Distribution's shared mode                                                |
| `LLD_LINK_KIND`  | `dylib` or `static`                      | Shared when LLVM is shared and both LLD libraries exist; otherwise static |

Override precedence is `NAME_<target-triple>`, `NAME_<target_with_underscores>`,
`HOST_NAME` or `TARGET_NAME`, then `NAME`, following
[cc-rs](https://docs.rs/cc/latest/cc/#external-configuration-via-environment-variables).
Empty overrides fail. Compiler, archiver, C++ standard library, and target flags
retain cc-rs semantics. On macOS explicit sysroot flags win; otherwise `SDKROOT`
or `xcrun` supplies the SDK. Selected environment variables and native identity
files participate in Cargo rebuild tracking.

`build.rs` orchestrates `build/llvm.rs` discovery and `build/cpp.rs` compilation.
[CXX](https://cxx.rs/build/cargo.html) supplies a `cc::Build`; `llvm-config` supplies
library filenames, paths, link mode, and system dependencies. Do not copy LLVM's
`--cxxflags`: they can disable bridge exceptions or select an older C++ standard.
Shared LLD with static LLVM is rejected to avoid duplicate runtime state. Static
LLD with shared LLVM is valid; fully static builds still require system dependencies.

Cross builds need host-runnable discovery tools describing target libraries,
matching LLD, target compiler/SDK, and Cargo linker settings. Discovery uses host
tools; library naming follows the target. The build never downloads a toolchain.
Windows, Linux, static, cross, and universal macOS paths still need platform proof.

Unicorn's upstream build probes pkg-config, otherwise builds bundled sources. Set
`UNICORN_NO_PKG_CONFIG=1` to select bundled-source verification. `LIBCLANG_PATH`
follows [bindgen requirements](https://rust-lang.github.io/rust-bindgen/requirements.html).
Only x86 and AArch64 guest features are enabled; the binding's AArch64 dependency
also compiles ARM internally without exposing another Oplab guest profile.

C++23 code uses RAII and borrowed spans with call-scoped lifetimes, following the
[C++ Core Guidelines](https://isocpp.github.io/CppCoreGuidelines/CppCoreGuidelines).
CXX translates exceptions; aborts and resource exhaustion require process
supervision. MC finalization runs once, including target literal pools, relaxation,
fixups, relocation recording, and DWARF emission. Do not call layout as a supposedly
read-only preflight. Review matching release source when changing the adapter.

## Build ownership

Tauri hooks stage the sidecar before invoking the frontend command. Target selection
uses `TAURI_ENV_TARGET_TRIPLE`, then `CARGO_BUILD_TARGET`, then host. Hook profiles
follow `TAURI_ENV_DEBUG`; standalone staging defaults to debug. Cargo JSON artifact
messages provide executable paths even with a custom target directory. Staging uses
[Tauri's external binary naming](https://tauri.app/develop/sidecar/).

Native build metadata stays in Cargo `OUT_DIR`. xtask obtains that directory from
Cargo messages and gives clang-tidy the actual `compile_commands.json`, including
CXX headers and cc-rs arguments. Formatting uses clang-format from the selected
`LLVM_CONFIG` tool directory and requires no native compilation.

Rust DTOs own committed TypeScript declarations. Generation formats a temporary
export and checks both content and file-set drift, independently of ambient ts-rs
export settings. Paraglide output, SvelteKit output, schemas, permissions, and staged
binaries are generated and ignored. Catalogs, capabilities, lockfiles, and minimized
property regressions stay tracked. Ignore rules cannot untrack an indexed file.

## Configuration

Keep one configuration per owning tool, adding only project requirements:

| Files                                       | Responsibility                                                                   |
| ------------------------------------------- | -------------------------------------------------------------------------------- |
| Root `Cargo.toml`, `.cargo/config.toml`     | Shared dependencies/features, metadata, profiles, lints; xtask alias.            |
| `vite.config.ts`                            | Application plugins, HMR, and both Vitest projects.                              |
| `svelte.config.ts`, `tsconfig.json`         | Static SPA output; strict additions to SvelteKit's generated TypeScript project. |
| `eslint.config.js`                          | Typed TS/Svelte linting, exhaustive switches, desktop import boundary.           |
| `stylelint.config.ts`, `.oxfmtrc.json`      | Native CSS analysis; web/config/document formatting.                             |
| `.clang-tidy`, `.clang-format`              | C++ analysis and formatting.                                                     |
| `knip.jsonc`                                | Explicit exceptions for Rust-task and inlang-owned entry points.                 |
| `src-tauri/tauri.conf.json`, `capabilities` | Desktop lifecycle, bundling, CSP, command access.                                |

ESLint's `.js` is native ESM under `type: module`; `.mjs` adds no behavior, while
TypeScript would need extra loader support in this ESLint release. Tools that
natively load TypeScript use `.ts`. Vitest intentionally lives in Vite config so
both projects inherit SvelteKit and Paraglide. Svelte has no `<style>` blocks;
Stylelint reads ordinary CSS without `postcss-html`.
See [ESLint](https://eslint.org/docs/latest/use/configure/configuration-files),
[typed linting](https://typescript-eslint.io/getting-started/typed-linting/),
[Stylelint](https://stylelint.io/user-guide/configure/), and [Vitest projects](https://vitest.dev/guide/projects).

Rust enables Clippy all, pedantic, and nursery with selected restriction rules and
`-D warnings`. `redundant_pub_crate` is intentionally disabled; explicit scoped
visibility is checked by `unreachable_pub`. Prefer fixes, then narrowly scoped
reasoned `#[expect]` when retaining the implementation is better. Stale expectations
fail. Avoid broad allowances and suppressing dependency warnings without evidence.
Native lint selections favor correctness, ownership, performance, and useful
modernization over purely stylistic churn.

## Application icon

`static/icon.svg` is the editable master. Keep colors and geometry self-contained;
filled paths preserve rounded outlines across SVG rasterizers. Review the SVG in a
browser first, including small-size legibility and transparency.

Rasterize with ImageMagick, then let the [Tauri icon command](https://tauri.app/develop/icons/)
package platform formats. Use a temporary output directory so unrelated mobile and
store assets do not enter this desktop repository:

```sh
icon_work=$(mktemp -d)
magick -background none -density 192 static/icon.svg -resize 1024x1024 -depth 8 "PNG32:$icon_work/master.png"
pnpm tauri icon "$icon_work/master.png" --output "$icon_work/icons"
cp "$icon_work/icons/32x32.png" "$icon_work/icons/128x128.png" "$icon_work/icons/128x128@2x.png" "$icon_work/icons/icon.ico" "$icon_work/icons/icon.icns" src-tauri/icons/
```

The snippet uses a POSIX shell; the same two tools accept native Windows paths.
Inspect PNG, every ICO layer, and extracted ICNS representations. Compare them with
Tauri's PNG output at the same dimensions; check silhouette, color, alpha, and
small-size legibility. A successful conversion alone is insufficient. Rebuild the
application bundle after replacement; an already-running application may retain
its old icon.

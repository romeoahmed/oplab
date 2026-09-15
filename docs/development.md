# Development and builds

This guide covers setup, commands, native discovery and build outputs. Manifests
and lockfiles define dependencies; [testing](testing.md) defines acceptance.
Build from the repository root. No toolchain is downloaded by `build.rs` or xtask.

## Requirements by task

| Task                                                | Requirements                                                                                                                                                    |
| --------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Frontend development, static build, types and lints | Node satisfying `package.json` (`>=24.21.0`), pnpm, installed JavaScript dependencies                                                                           |
| Frontend tests                                      | The above plus Playwright Chromium and its host dependencies                                                                                                    |
| Core tests and xtask compilation                    | Stable Rust satisfying workspace `rust-version` (`1.98` minimum)                                                                                                |
| Contract generation                                 | Rust, Node, pnpm and installed JavaScript dependencies; no LLVM                                                                                                 |
| Engine, CLI and worker                              | Rust, C/C++ toolchain with C++23 support, LLVM 23 development installation, matching LLD development installation, libclang, CMake, a build tool and pkg-config |
| Desktop and full workspace checks/tests             | All engine/frontend requirements plus the platform's Tauri prerequisites, rustfmt, Clippy, clang-format and clang-tidy                                          |
| Icon regeneration only                              | ImageMagick and the project's Tauri CLI                                                                                                                         |

`rust-toolchain.toml` selects stable and installs rustfmt/Clippy. Use a maintained
Node release satisfying the manifest; pnpm 12 is the verified package manager.
Keep both lockfiles. `pnpm install --frozen-lockfile` and Cargo's `--locked` reject
dependency drift. No global Tauri CLI or external `cargo-xtask` installation is needed.

Frontend-only work does not require Rust or native libraries. Browser preview can
edit source and settings, but cannot assemble or execute guest programs.

## Platform prerequisites

Follow [Tauri's platform setup](https://tauri.app/start/prerequisites/) before a desktop build:

| Host    | Platform components                                                                                                          | Project verification                                    |
| ------- | ---------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------- |
| macOS   | Xcode Command Line Tools or Xcode with an active macOS SDK; system WKWebView                                                 | Apple Silicon native debug app and both guests verified |
| Linux   | Distribution development tools and Tauri's WebKitGTK 4.1, GTK and related development packages                               | Build and installed-app acceptance pending              |
| Windows | MSVC Rust target, Visual Studio C++ Build Tools and Windows SDK, WebView2; compatible native LLVM/LLD libraries and compiler | Build and installed-app acceptance pending              |

Guest architecture is independent of host architecture. An AArch64 Mac can emulate
both supported guests without cross-compiling Oplab. Cross-compilation instead
means building the host application for a different Rust target.

Tauri's minimum platform requirements alone do not establish Oplab's supported
WebView minimums. Native CSS and Web APIs must also pass on the intended host.

## Native toolchain

The bridge accepts **LLVM 23**, with **the same LLVM/LLD release identity** and a
compatible C++ ABI. LLVM/LLD 23.1.1 is verified; the build does not pin that patch.
A compiler-only LLVM installer or a standalone `ld.lld` is insufficient. Supply:

- `llvm-config`, LLVM headers and libraries with the **X86 and AArch64** targets.
- LLD's `include/lld/Common/Driver.h`, `lib/lldELF` and `lib/lldCommon` libraries
  (platform naming applies), plus a host-runnable `bin/ld.lld` for version discovery.
- A C++23 compiler and compatible standard library. The bridge uses CXX; CXX does
  not provide LLVM, an SDK, or a C++ standard library.
- `libclang` for Unicorn's generated bindings. Select its directory with
  `LIBCLANG_PATH` when automatic discovery fails; see [bindgen requirements](https://rust-lang.github.io/rust-bindgen/requirements.html).
- CMake and Ninja or the platform's Make/build tool. Ninja is recommended and is
  used by Unicorn's MSVC path. Unix Unicorn configuration requires **pkg-config**
  even when building its bundled sources; pkgconf can provide that executable.
- `clang-format` and `clang-tidy` in the selected LLVM tool directory for full checks.

Unicorn's Rust build generates bindings, probes a system Unicorn installation,
then builds bundled sources if the probe fails. For the verified bundled path,
set `UNICORN_NO_PKG_CONFIG=1`. This disables the system-library probe; it does **not**
remove the bundled configure script's pkg-config requirement. The Cargo features
select x86 and AArch64; upstream also builds ARM internally for AArch64.

### macOS example

After installing Rust, Node, pnpm and the macOS SDK, a Homebrew setup is:

```sh
brew install llvm@23 lld@23 cmake ninja pkgconf
export LLVM_CONFIG="$(brew --prefix llvm@23)/bin/llvm-config"
export LLD_PREFIX="$(brew --prefix lld@23)"
export LIBCLANG_PATH="$(brew --prefix llvm@23)/lib"
export CC="$(brew --prefix llvm@23)/bin/clang"
export CXX="$(brew --prefix llvm@23)/bin/clang++"
export UNICORN_NO_PKG_CONFIG=1
```

Ensure Homebrew's executable directory is on `PATH`. These are shell settings,
not repository configuration. LLVM and LLD are separate
[Homebrew packages](https://formulae.brew.sh/formula/llvm); check the
[LLD package](https://formulae.brew.sh/formula/lld) remains on the matching release.
Keep the system SDK and C++ runtime unless a coherent alternative has been verified.

For Linux or Windows, use development packages meeting the inventory above; package
names vary. If unavailable, build a matching LLVM 23 release from the
[LLVM project](https://github.com/llvm/llvm-project/releases) using its
[CMake instructions](https://llvm.org/docs/CMake.html). Enable `clang`,
`clang-tools-extra` and `lld`, and targets `X86;AArch64`; install headers, libraries,
`llvm-config`, libclang and analysis tools, not just compiler/linker executables.
On supported Unix hosts, `LLVM_BUILD_LLVM_DYLIB` and `LLVM_LINK_LLVM_DYLIB` select a
shared LLVM build. Those options are not a portable Windows recipe.

### Discovery and overrides

| Variable         | Meaning                                       | Default                                                                        |
| ---------------- | --------------------------------------------- | ------------------------------------------------------------------------------ |
| `LLVM_CONFIG`    | Host-runnable LLVM discovery executable       | `llvm-config` on `PATH`                                                        |
| `LLD_PREFIX`     | Prefix containing LLD `include`, `lib`, `bin` | LLVM prefix                                                                    |
| `LLVM_LINK_KIND` | `dylib` or `static`                           | `llvm-config --shared-mode`                                                    |
| `LLD_LINK_KIND`  | `dylib` or `static`                           | Shared if LLVM is shared and both LLD shared libraries exist; otherwise static |

The bridge resolves these four overrides in order: `NAME_<target-triple>`,
`NAME_<target_with_underscores>`, `HOST_NAME` for a native build or `TARGET_NAME` for
a cross build, then `NAME`. Empty values fail. This follows
[cc-rs precedence](https://docs.rs/cc/latest/cc/#external-configuration-via-environment-variables).
`CC`, `CXX`, `AR`, `CXXFLAGS`, standard-library selection and target flags retain
cc-rs behavior. Libclang and pkg-config retain their upstream environment conventions.

On macOS, explicit compiler sysroot flags take precedence; otherwise `SDKROOT` or
`xcrun` supplies the SDK. `build.rs` tracks selected environment and identity files.
It uses [llvm-config](https://llvm.org/docs/CommandGuide/llvm-config.html) for library
names and dependencies, without copying `--cxxflags`, which may select an older
language standard or disable exceptions needed by the bridge.

Static LLD with shared LLVM is valid. Shared LLD with static LLVM is rejected to
avoid duplicate LLVM runtime state. Static linking still requires transitive system
libraries and does not make the whole application self-contained. Cross builds
need host-runnable discovery tools describing target libraries, target SDK/compiler,
and Cargo linker settings. Cross, static and universal macOS builds remain unverified.
`cargo xtask fmt` uses the unqualified `LLVM_CONFIG` to locate host clang-format;
compilation and clang-tidy use the build's resolved target configuration.

### Diagnose setup before changing code

With the variables above selected, run:

```sh
rustc --version
node --version
pnpm --version
"$LLVM_CONFIG" --version
"$LLVM_CONFIG" --targets-built
"$LLVM_CONFIG" --includedir --libdir --shared-mode
"$LLD_PREFIX/bin/ld.lld" --version
cmake --version
ninja --version
pkg-config --version
```

The shell examples are POSIX; use PowerShell's environment syntax and call operator
on Windows. A missing-header or missing-library error calls for a complete development
installation. A libclang error calls for bindgen discovery. If Unicorn configuration
fails, read the **first configure error**, not only the later generated-header errors.
After correcting a compiler or configure dependency, clean only its failed build:

```sh
cargo clean -p unicorn-engine-sys --target <host-triple>
```

Use the triple from the failed invocation. Keep compiler/SDK settings consistent
across commands; do not patch generated native headers to hide a failed configure.

## Commands and ownership

Install dependencies, then choose browser or desktop development:

```sh
pnpm install --frozen-lockfile
pnpm dev
# Or, after native setup and with the standalone Vite server stopped:
pnpm tauri dev
```

Install the test browser separately when running component tests:

```sh
pnpm exec playwright install --with-deps --no-shell chromium
```

| Owner        | Commands                                 | Scope                                                                                  |
| ------------ | ---------------------------------------- | -------------------------------------------------------------------------------------- |
| package.json | `pnpm dev`, `pnpm build`, `pnpm preview` | Vite HMR, static build, static preview                                                 |
| package.json | `pnpm check`                             | Compile catalogs, sync SvelteKit, check Svelte/TypeScript                              |
| package.json | `pnpm lint`                              | ESLint, native CSS Stylelint, Knip                                                     |
| package.json | `pnpm test`                              | Vitest logic and Chromium component tests                                              |
| Tauri CLI    | `pnpm tauri dev`, `pnpm tauri build`     | Worker staging hooks, frontend lifecycle, desktop build and bundling                   |
| xtask        | `cargo xtask check`                      | Sidecar, strict Clippy, clang-tidy, frontend check/lint, contract drift and formatting |
| xtask        | `cargo xtask test [--release]`           | Stage worker, test Rust workspace, delegate to `pnpm test`; release applies to Rust    |
| xtask        | `cargo xtask fmt [--check]`              | rustfmt, Oxfmt and clang-format                                                        |
| xtask        | `cargo xtask codegen [--check]`          | Export or verify Rust-owned TypeScript contracts                                       |
| xtask        | `cargo xtask sidecar [--release]`        | Build and stage the worker without starting the app                                    |

Frontend recipes live once in package.json; xtask calls them. Formatting spans
languages and belongs to xtask. Tauri owns desktop builds; there is no second xtask
build/dev command. Use `cargo xtask --help` and subcommand help for options. Focused
runs use native tool arguments, such as `pnpm test --project logic`,
`pnpm test --project browser`, or `cargo test -p oplab-core --locked`.

Checks may compile dependencies, refresh ignored generated files, and stage ignored
binaries. They do not update lockfiles, format tracked source, or change Git staging.
Use one Vite server per checkout. Frontend and catalog changes use HMR; engine
changes require worker staging and an app restart. Tauri's watcher does not rebuild
the independent worker automatically.

## API documentation

Comments describe contracts and non-obvious constraints: units, ownership, failure
behavior and invariants. Keep implementation explanations beside the relevant code.
TypeScript uses [TSDoc](https://tsdoc.org/) summaries, with `@remarks`, `@returns` or
`@throws` when they add useful information; do not repeat declared types in tags.
Rust uses [`//!` for modules and `///` for items](https://doc.rust-lang.org/reference/comments.html),
with linked API references, `# Errors` sections and runnable examples where useful.
Edit exported contract comments in Rust, then run `cargo xtask codegen`.

With the native toolchain configured, verify Rust documentation and its examples:

```sh
cargo doc --workspace --no-deps --locked
cargo test --workspace --doc --locked
```

## Outputs and distribution

Tauri hooks run sidecar staging before Vite. Worker target selection uses
`TAURI_ENV_TARGET_TRIPLE`, then `CARGO_BUILD_TARGET`, then the Rust host triple.
Hook profiles follow `TAURI_ENV_DEBUG`; standalone staging defaults to debug.
Cargo JSON messages supply actual executable paths, including custom target directories.
The staged name follows [Tauri sidecar naming](https://tauri.app/develop/sidecar/).

| Output                                                                            | Ownership                                                       |
| --------------------------------------------------------------------------------- | --------------------------------------------------------------- |
| `build/`                                                                          | SvelteKit static frontend; no Node server in the desktop bundle |
| Cargo target directory                                                            | Rust binaries, native objects and build metadata                |
| `src-tauri/binaries/`                                                             | Ignored target-suffixed worker copies for Tauri                 |
| Cargo bundle directory                                                            | Platform packages from `pnpm tauri build`                       |
| `src/lib/protocol/generated/`                                                     | Committed ts-rs contracts; regenerate with xtask                |
| `src/lib/paraglide/`, `.svelte-kit/`, `src-tauri/gen/`, autogenerated permissions | Ignored generated output                                        |

For the verified macOS debug bundle:

```sh
pnpm tauri build --debug --bundles app
```

`pnpm tauri build` selects release builds and configured platform bundle targets.
The worker's transitive LLVM/LLD shared libraries are **not automatically bundled
by declaring `externalBin`**. Local debug success can depend on installed native
libraries. Inspect native dependencies and implement target-specific bundling,
loader paths, licensing and signing/JIT policy before distributing an installer.
No signed or independently installed distribution is currently verified.

## Configuration and assets

Each tool owns one configuration. `vite.config.ts` contains SvelteKit, Paraglide and
both Vitest projects. `svelte.config.ts` produces a static SPA; `tsconfig.json`
extends SvelteKit's generated project. ESLint uses native ESM `.js`; Stylelint reads
native CSS. There are no Svelte `<style>` blocks or PostCSS adapters.

The native bridge's `OUT_DIR` holds compilation metadata. xtask obtains it from
Cargo and runs clang-tidy against the actual cc-rs/CXX compilation database.
C++ formatting uses the selected LLVM clang-format. Root Cargo dependencies and
lints are inherited throughout the workspace. Knip's two exceptions document
Oxfmt invoked by Rust tooling and the inlang-owned message-format plugin.

`static/icon.svg` is the application icon master and README logo. Review it in a
browser at small and large sizes. To regenerate desktop formats:

```sh
icon_work=$(mktemp -d)
magick -background none -density 192 static/icon.svg -resize 1024x1024 -depth 8 "PNG32:$icon_work/master.png"
pnpm tauri icon "$icon_work/master.png" --output "$icon_work/icons"
cp "$icon_work/icons/32x32.png" "$icon_work/icons/128x128.png" "$icon_work/icons/128x128@2x.png" "$icon_work/icons/icon.ico" "$icon_work/icons/icon.icns" src-tauri/icons/
```

Use ImageMagick to rasterize and [Tauri](https://tauri.app/develop/icons/) to package.
Keep temporary/mobile/store output outside the repository. Inspect every ICO/ICNS
representation against matching-size PNG output for shape, color and transparency,
then rebuild the bundle. Conversion success alone is not visual verification.

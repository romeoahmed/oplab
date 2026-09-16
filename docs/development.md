# Development and builds

Run commands from the repository root. Manifests and lockfiles define dependencies;
[testing](testing.md) defines acceptance. Install native tools before building;
`build.rs` and xtask do not download them.

## Requirements by task

| Task                                                | Requirements                                                                                                                                                    |
| --------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Frontend development, static build, types and lints | Node satisfying `package.json`, pnpm, installed JavaScript dependencies                                                                                         |
| Frontend tests                                      | The above plus Playwright Chromium and its host dependencies                                                                                                    |
| Core tests and xtask compilation                    | Stable Rust satisfying workspace `rust-version`                                                                                                                 |
| Contract generation                                 | Rust, Node, pnpm and installed JavaScript dependencies; no LLVM                                                                                                 |
| Engine, CLI and worker                              | Rust, C/C++ toolchain with C++23 support, LLVM 23 development installation, matching LLD development installation, libclang, CMake, a build tool and pkg-config |
| Desktop and full workspace checks/tests             | All engine/frontend requirements plus the platform's Tauri prerequisites, rustfmt, Clippy, clang-format and clang-tidy                                          |
| Icon regeneration only                              | ImageMagick and the project's Tauri CLI                                                                                                                         |

`rust-toolchain.toml` selects stable and installs rustfmt/Clippy. The manifests own
minimum Rust and Node versions; pnpm 12 is the verified package manager.
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
not repository configuration. LLVM and LLD are separate Homebrew packages; verify
their reported release identities match before building.
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
upstream behavior. CXX's `link-cplusplus` dependency selects the C++ runtime;
cc-rs selects the compiler and archiver. The bridge adds `/EHsc` for MSVC-compatible
compilers so CXX exception translation unwinds C++ owners. Libclang and pkg-config
retain their upstream environment conventions.

On macOS, explicit compiler sysroot flags take precedence; otherwise `SDKROOT` or
`xcrun` supplies the SDK. `build.rs` tracks selected environment and identity files.
Discovery rejects installations missing either guest target before compilation.
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

Use `pnpm dev` for frontend HMR or `pnpm tauri dev` after native setup. Stop
standalone Vite before starting Tauri dev; both use the same configured port.
Stop development servers before a static build in the same checkout. Both regenerate
SvelteKit/Paraglide output, which can invalidate the running HMR module graph.

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

package.json owns frontend recipes, xtask owns cross-tool verification and formatting,
and Tauri owns desktop builds. Use `cargo xtask --help` for options. Focused runs use
native arguments: `pnpm test --project logic`, `pnpm test --project browser` or
`cargo test -p oplab-core --locked`.

Check/test commands may compile dependencies, refresh ignored generated files
and copy worker binaries into `src-tauri/binaries/`. They preserve lockfiles,
tracked source and the Git index.
Frontend and catalog changes use HMR; engine changes require worker staging and
an app restart. Tauri's watcher does not rebuild the independent worker automatically.
Protocol changes require rebuilding clients and worker together: the private
[development contract](protocol.md#negotiation-and-identity) evolves in place at version 1.

## API documentation

TypeScript uses [TSDoc](https://tsdoc.org/) summaries, with `@remarks`, `@returns` or
`@throws` when they add useful information; do not repeat declared types in tags.
Rust uses [`//!` for modules and `///` for items](https://doc.rust-lang.org/reference/comments.html),
with linked API references, `# Errors` sections and runnable examples where useful.
Edit exported contract comments in Rust, then run `cargo xtask codegen`.

With the native toolchain configured, verify Rust documentation and its examples:

```sh
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
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
loader paths, licensing and signing/JIT policy before distribution. See the
[release gates](roadmap.md#release-gates).

### Distribution licensing

Oplab's source uses standard MPL-2.0. The root `LICENSE` contains the unchanged
[official text](https://www.mozilla.org/media/MPL/2.0/index.txt); the README names
the copyright holder. No Exhibit B opt-out applies: retaining that exhibit in the
standard text does not apply its notice to the project.

The locked `unicorn-engine` and `unicorn-engine-sys` packages declare `GPL-2.0`,
and [Unicorn identifies its license as GPLv2](https://www.unicorn-engine.org/).
MPL §§1.12 and 3.3 permit combination with GPLv2 through secondary licensing;
they do not relicense Unicorn under MPL or remove GPL obligations. When distributing
the linked worker or CLI, distribute the combined work under GPLv2 and also make
its MPL-covered source available under MPL, retaining existing notices. Supply
the GPL text and complete corresponding source, including required build scripts,
or another source-provision option permitted by GPLv2 §3. See
[Mozilla's compatibility FAQ](https://www.mozilla.org/en-US/MPL/2.0/FAQ/#q14-may-i-combine-mpl-licensed-code-and-lgpl-licensed-code-in-the-same-executable-program)
and [distribution guidance](https://www.mozilla.org/en-US/MPL/2.0/combining-mpl-and-gpl/).

Before distribution, review compatibility across the complete linked dependency
set, including LLVM/LLD, and package its licenses and notices. The MPL/GPL route
alone does not clear a release, and a sidecar boundary alone does not establish
independent works. Distribution remains a [release gate](roadmap.md#release-gates).

## Configuration and assets

Each tool owns one configuration. `vite.config.ts` contains SvelteKit, Paraglide and
both Vitest projects. The browser project uses
[Vite entry scanning](https://vite.dev/config/dep-optimization-options#optimizedeps-entries)
for tests and the dynamic editor, avoiding dependency reloads during cold runs.
`svelte.config.ts` produces a static SPA; `tsconfig.json` extends SvelteKit's
generated project. ESLint uses native ESM `.js`; TypeScript ESLint uses the project
service, and Stylelint reads native CSS.
No separate Vitest configuration, test TS project or PostCSS adapter is needed.
Only `.svelte` components disable `no-useless-default-assignment`: the rule treats
required [`$bindable()` declarations](https://svelte.dev/docs/svelte/$bindable) as
removable defaults. Ordinary TypeScript retains the rule.

Prefer tool defaults unless a project constraint requires an override. Oxfmt honors
`.gitignore` and its built-in lockfile exclusions; its only additional exclusion is
the committed Rust-generated contracts. Rustfmt records both edition and style edition
so standalone editor formatting agrees with Cargo. Tauri retains explicit application
identity, window geometry, build hooks and capabilities; bundle targets use platform defaults.

CXX exposes headers under the Cargo package name. The include
`oplab-engine/src/assembly/ffi.rs.h` refers to the generated bridge header under
`OUT_DIR/cxxbridge/include`, not a source-tree file. CXX also exposes handwritten
headers through its crate include directory; no custom include prefix is needed.
The bridge's three C++ translation units compile through cc-rs's `parallel`
feature, coordinated by Cargo's jobserver and `-j` limit.

The native bridge's `OUT_DIR` holds compilation metadata. xtask obtains it from
Cargo and runs clang-tidy against the cc-rs/CXX compiler arguments, carrying over
cc-rs's tool environment for SDK/MSVC headers. The JSON database uses argument
arrays without shell escaping; paths and arguments must be UTF-8.
C++ formatting inherits LLVM style with C++23 and a 100-column limit. clang-tidy
owns brace enforcement, correctness, modernization and direct-include checks;
formatting does not insert control-flow syntax. Both tools come from the selected
LLVM installation. Knip declares the dynamically imported Svelte editor as an
entry so its dependency tree remains checked. Its two dependency exceptions cover Oxfmt invoked
by Rust and the inlang-owned message-format plugin.

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

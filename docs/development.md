# Development and builds

Run commands from the repository root. Manifests and lockfiles define dependencies;
[testing](testing.md) defines acceptance. Install native tools before building;
`build.rs` does not download native dependencies. The explicit `cargo xtask sdk`
command obtains pinned SDK sources and delegates to upstream build tools.

## Requirements by task

| Task                                                | Requirements                                                                                                           |
| --------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| Frontend development, static build, types and lints | Node satisfying `package.json`, pnpm, installed JavaScript dependencies                                                |
| Frontend tests                                      | The above plus Playwright Chromium and its host dependencies                                                           |
| Core tests and xtask compilation                    | Stable Rust satisfying workspace `rust-version`                                                                        |
| Contract generation                                 | Rust, Node, pnpm and installed JavaScript dependencies; no LLVM                                                        |
| Native libraries, CLI and worker                    | Rust, GNU C23/C++23 toolchain, LLVM 23 development installation, matching LLD and QEMU/XED SDK                         |
| Desktop and full workspace checks/tests             | All engine/frontend requirements plus the platform's Tauri prerequisites, rustfmt, Clippy, clang-format and clang-tidy |
| Icon regeneration only                              | ImageMagick and the project's Tauri CLI                                                                                |

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
The execution runtime currently requires a 64-bit little-endian host.

Tauri's minimum platform requirements alone do not establish Oplab's supported
WebView minimums. Native CSS and Web APIs must also pass on the intended host.

## Native toolchain

The bridge accepts **LLVM 23**, with **the same LLVM/LLD release identity** and a
compatible C++ ABI. LLVM/LLD 23.1.1 is verified; the build does not pin that patch.
A compiler-only LLVM installer or a standalone `ld.lld` is insufficient. Supply:

- `llvm-config`, LLVM headers and libraries with the **X86 and AArch64** targets.
- LLD's `include/lld/Common/Driver.h`, `lib/lldELF` and `lib/lldCommon` libraries
  (platform naming applies), plus a host-runnable `bin/ld.lld` for version discovery.
- GNU C23 and C++23 compilers with a compatible standard library. The bridge uses CXX; CXX does
  not provide LLVM, an SDK, or a C++ standard library.
- QEMU 11.1.1 and Intel XED v2026.08.23, built with `cargo xtask sdk`.
- Git, Python, Ninja, pkg-config and GLib development files. QEMU's configure
  creates its own Python environment and obtains its declared build dependencies.
- `clang-format` and `clang-tidy` from the selected LLVM installation.

### QEMU and XED SDK

After selecting the host compiler, run:

```sh
cargo xtask sdk
export XED_PREFIX="$PWD/target/native-sdk/xed"
export OPLAB_QEMU_DIR="$PWD/target/native-sdk/lib"
```

Next install frontend dependencies and run `pnpm tauri dev` to build and stage
the worker, or use `cargo xtask sidecar` for staging alone.
The default uses Cargo's target directory; adjust the two exports when using
`CARGO_TARGET_DIR` or `--prefix`. `--qemu-source`, `--xed-source` and
`--mbuild-source` optionally clone local Git repositories instead of downloading
upstream sources. Exact release tags are verified. User source checkouts are never
modified. Incremental QEMU builds reconfigure Meson and clear its dependency
cache so package upgrades do not retain stale library paths; unchanged compilation
outputs remain incremental. Use a fresh prefix when changing compilers, host
targets or SDK release tags. Stop workers before rebuilding an SDK and restart
them after staging. XED uses mbuild v2026.08.23
and installs headers and a static library independently of QEMU. XED and mbuild
are independently versioned; both selected tags are v2026.08.23.

The owned QEMU checkout receives two build-only integrations: an additional Meson
shared-library target and the minimal ARM GICv5 CPU-interface configuration required
by MAX helpers. CPU translators, helpers, SoftFloat and lifecycle source are not
patched. The project adapter uses GNU C23 with warnings treated as errors;
upstream QEMU retains its own language standard and warning policy. Per-target
compiler arguments, source selection and dependencies come from QEMU's build graph.
Unchanged adapter files retain their timestamps for incremental builds.
Rebuild the SDK after adapter changes; Cargo only generates bindings for the
private boundary header.

`--asan` builds an instrumented QEMU SDK for native memory checks. Load the selected
Clang AddressSanitizer runtime when running it inside an uninstrumented Rust test
binary. Do not ship the instrumented SDK. Apple Silicon builds are verified;
Linux and Windows SDK construction and deployment remain acceptance gates.

### macOS example

After installing Rust, Node, pnpm and the macOS SDK, a Homebrew setup is:

```sh
brew install llvm@23 lld@23 ninja pkgconf glib
export LLD_PREFIX="$(brew --prefix lld@23)"
export CC="$(brew --prefix llvm@23)/bin/clang"
export CXX="$(brew --prefix llvm@23)/bin/clang++"
export LLVM_SYS_231_PREFIX="$(brew --prefix llvm@23)"
export LIBCLANG_PATH="$LLVM_SYS_231_PREFIX/lib"
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
`llvm-config` and analysis tools, not just compiler/linker executables.
On supported Unix hosts, `LLVM_BUILD_LLVM_DYLIB` and `LLVM_LINK_LLVM_DYLIB` select a
shared LLVM build. Those options are not a portable Windows recipe.

### Discovery and overrides

[`llvm-sys`](https://gitlab.com/taricorp/llvm-sys.rs) owns LLVM discovery and
linkage for both ORC and the CXX bridge. The bridge obtains the selected
`llvm-config` through [Cargo dependency metadata](https://doc.rust-lang.org/cargo/reference/build-scripts.html#the-links-manifest-key)
and adds matching LLD and static XED libraries. It does not parse LLVM linker
arguments or maintain a second LLVM selector.

| Variable              | Purpose                                          | Default                                                              |
| --------------------- | ------------------------------------------------ | -------------------------------------------------------------------- |
| `LLVM_SYS_231_PREFIX` | LLVM 23 installation for both Rust and C++       | llvm-sys discovery on `PATH`                                         |
| `LLD_PREFIX`          | Matching LLD `include`, `lib` and `bin`          | Selected LLVM prefix                                                 |
| `LLD_LINK_KIND`       | `dylib` or `static` for LLD only                 | Shared when LLVM and both LLD libraries support it; otherwise static |
| `XED_PREFIX`          | Static Intel XED installation                    | Required                                                             |
| `OPLAB_QEMU_DIR`      | Directory containing both private QEMU libraries | Required for native execution and staging                            |
| `LIBCLANG_PATH`       | libclang for bindgen                             | bindgen discovery                                                    |

The workspace enables llvm-sys `prefer-dynamic`: use shared LLVM when available,
otherwise static. Static LLD with shared LLVM is valid; shared LLD with static LLVM
is rejected to avoid duplicate LLVM state. Release-static linkage and cross builds
remain unverified. Static native libraries still have transitive system dependencies.

`LLD_PREFIX`, `LLD_LINK_KIND` and `XED_PREFIX` follow
[cc-rs environment precedence](https://docs.rs/cc/latest/cc/#external-configuration-via-environment-variables):
`NAME_<target-triple>`, `NAME_<target_with_underscores>`, `HOST_NAME` for native
builds or `TARGET_NAME` for cross builds, then `NAME`. Empty overrides fail.
LLVM and bindgen retain their own discovery rules. The desktop resolves its bundled
QEMU resource directory and passes it to the worker instead of relying on the
inherited development path.

`CC`, `CXX`, `AR`, `CXXFLAGS`, target flags and standard-library selection retain
upstream behavior. CXX's `link-cplusplus` selects the C++ runtime; cc-rs selects the
compiler and archiver. The bridge adds `/EHsc` for MSVC-compatible compilers to
preserve CXX exception unwinding. It does not copy LLVM's `--cxxflags`, which may
select an older standard or disable exceptions.

On macOS, explicit compiler sysroot flags take precedence; otherwise `SDKROOT` or
`xcrun` supplies the SDK. Cross builds need host-runnable discovery tools describing
target libraries, a target SDK/compiler and Cargo linker settings.
`cargo xtask fmt` finds host `llvm-config` under `LLVM_SYS_231_PREFIX/bin`, or on
`PATH` when no prefix is set. clang-tidy uses the actual CXX compilation metadata.

### Diagnose setup before changing code

With the variables above selected, run:

```sh
rustc --version
node --version
pnpm --version
"$LLVM_SYS_231_PREFIX/bin/llvm-config" --version
"$LLVM_SYS_231_PREFIX/bin/llvm-config" --targets-built
"$LLVM_SYS_231_PREFIX/bin/llvm-config" --includedir --libdir
"$LLD_PREFIX/bin/ld.lld" --version
ninja --version
pkg-config --version
```

These commands assume explicit LLVM/LLD prefixes. Shell examples are POSIX; use
PowerShell environment syntax and its call operator on Windows. Missing headers
or libraries require a complete development installation. Keep compiler, SDK,
deployment minimum and library settings consistent across the dependency SDK and
Cargo build; do not patch generated headers to hide
configuration errors.

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
| xtask        | `cargo xtask sdk`                        | Build isolated, versioned QEMU and static XED SDKs                                     |
| xtask        | `cargo xtask sidecar [--release]`        | Build and stage the worker without starting the app                                    |

package.json owns frontend recipes, xtask owns cross-tool verification and formatting,
and Tauri owns desktop builds. Use `cargo xtask --help` for options. Focused runs use
native arguments: `pnpm test --project logic`, `pnpm test --project browser` or
`cargo test -p oplab-core --locked`.

Check/test commands compile dependencies, refresh ignored generated files and stage
the worker plus QEMU libraries in `src-tauri/binaries/` and `src-tauri/runtime/`.
They preserve lockfiles, tracked source and the Git index. Codegen writes committed
DTOs unless `--check` is supplied; formatting writes source unless `--check` is supplied.
Frontend and catalog changes use HMR; engine changes require worker staging and
an app restart. Tauri's watcher does not rebuild the independent worker automatically.
Protocol changes require rebuilding clients and worker together: the private
[development contract](protocol.md#negotiation-and-identity) evolves in place at version 1.

## API documentation

Comments describe caller-visible contracts and non-obvious constraints: units,
ownership, bounds, failure and partial effects. Remove narration of syntax, change
history and details already expressed by types. Keep FFI safety reasoning beside
unsafe operations and native lifetime rules beside their owning boundary.

- [TSDoc](https://tsdoc.org/): start with a short summary. Put additional behavior
  in `@remarks`; use `@param`, `@returns` and `@throws` only for information beyond
  the signature. Use backticks for identifiers and units, not repeated type tags.
- [Rust](https://doc.rust-lang.org/reference/comments.html): use `//!` for modules
  and `///` for items. Separate summaries from `# Errors`, `# Panics` or `# Safety`
  where applicable; link related items with rustdoc links. Keep useful examples
  runnable and distinguish guest faults returned as data from infrastructure errors.
- Generated DTO comments originate in Rust. Run `cargo xtask codegen` after changing
  them; do not edit TypeScript output or generated/cache documentation.

With the native toolchain configured, verify rendering diagnostics and examples:

```sh
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
cargo test --workspace --doc --locked
cargo xtask codegen --check
```

README introduces the product and first workflow; AGENTS gives repository rules.
Each reference owns one subject. Keep dated acceptance in the roadmap, not inline
comments or repeated audit histories. See [AGENTS.md guidance](https://agents.md/)
and [README examples](https://github.com/matiassingers/awesome-readme).

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
| `src-tauri/binaries/`, `src-tauri/runtime/`                                       | Ignored worker and private QEMU libraries staged for Tauri      |
| Cargo bundle directory                                                            | Platform packages from `pnpm tauri build`                       |
| `src/lib/protocol/generated/`                                                     | Committed ts-rs contracts; regenerate with xtask                |
| `src/lib/paraglide/`, `.svelte-kit/`, `src-tauri/gen/`, autogenerated permissions | Ignored generated output                                        |

For the verified macOS debug bundle:

```sh
pnpm tauri build --debug --bundles app
```

`pnpm tauri build` selects release builds and configured platform bundle targets.
Both debug and release bundles embed a static frontend; this does not imply static
native linkage. Tauri hooks stage the worker and private QEMU libraries. Declaring
`externalBin` does not bundle transitive LLVM/LLD/GLib libraries. Development bundles
can depend on locally installed libraries; inspect loader paths and dependencies
before distributing. [Runtime](runtime.md#distribution-design) describes the planned
mixed linkage. Release-static LLVM, dependency relocation, signing/JIT policy and
independently installed acceptance remain [release gates](roadmap.md#release-gates).

### Distribution licensing

Oplab's source uses standard MPL-2.0. The root `LICENSE` contains the unchanged
[official text](https://www.mozilla.org/media/MPL/2.0/index.txt); the README names
the copyright holder. No Exhibit B opt-out applies: retaining that exhibit in the
standard text does not apply its notice to the project.

QEMU, XED, LLVM/LLD and their compiled transitive dependencies require a complete
notice and compatibility inventory before release. Preserve MPL secondary-license
compatibility. The [backend reference](runtime.md#distribution-design)
records the intended distribution boundary; license review does not block local
implementation or acceptance testing.

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
`oplab-toolchain/src/assembly/ffi.rs.h` refers to the generated bridge header under
`OUT_DIR/cxxbridge/include`, not a source-tree file. CXX also exposes handwritten
headers through its crate include directory; no custom include prefix is needed.
The bridge's C++ translation units compile through cc-rs's `parallel`
feature, coordinated by Cargo's jobserver and `-j` limit.

The CXX bridge's `OUT_DIR` holds compilation metadata. xtask obtains it from
Cargo and runs clang-tidy against the cc-rs/CXX compiler arguments, carrying over
cc-rs's tool environment for SDK/MSVC headers. The JSON database uses argument
arrays without shell escaping; paths and arguments must be UTF-8.
C and C++ formatting inherit LLVM style with a 100-column limit; C++ uses C++23.
clang-tidy checks correctness, concurrency, modernization, direct includes and
selected C++ Core Guidelines; clang-format owns formatting. Both tools come from
the selected LLVM installation, and compiler diagnostics remain authoritative.
The GNU C23 adapter uses QEMU's Meson compilation database and a self-contained
clang-tidy policy because its SDK can live outside the repository. It preserves
QEMU's ordered umbrella headers, fixed callback signatures and native mask types;
`nullptr` modernization also applies to C23. Checks reject stale staged adapter
sources and report only owned headers, without rewriting dependency headers.
Knip declares the dynamically imported Svelte editor as an
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

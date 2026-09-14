# Testing and acceptance

Tests establish behavior and invariants. Keep independent facts and regressions;
remove assertions about incidental object identity, DOM structure, queue ordering,
private storage, or generated formatting. A rewrite must not discard useful coverage
just to change its appearance. [Development](development.md) owns check commands;
[roadmap](roadmap.md) owns progress.

## Ownership

| Location                                          | Evidence                                                                                  |
| ------------------------------------------------- | ----------------------------------------------------------------------------------------- |
| `crates/oplab-core/tests`                         | Exact scalars, strict schemas, range/permission rules, fragmented and incomplete framing. |
| `crates/oplab-engine/tests`                       | Architectural bytes, ELF geometry, actual guest effects, real CLI and worker processes.   |
| `crates/oplab-engine/tests/unit`                  | Private scheduling, cancellation, stream baselines, and native mapping invariants.        |
| `src-tauri/tests/unit`                            | Real worker supervision, reattachment, stale leases, unknown outcomes, kill/reap.         |
| `tests/protocol`, `tests/i18n`, `tests/workbench` | Pure frontend properties, catalogs, scratch recovery, editor and workbench behavior.      |

Rust files under `tests/unit` are `#[cfg(test)]` modules referenced with `#[path]`.
They stay physically outside production source while retaining private access;
they do not widen APIs for tests. Root integration tests exercise public behavior.
This follows Rust's distinction between [unit and integration tests](https://doc.rust-lang.org/book/ch11-03-test-organization.html).
SvelteKit includes root `tests` in its generated TypeScript project; Vitest discovers
them explicitly. No production test helpers or separate test-only TS project exist.

## Properties and fixtures

Proptest varies full-width addresses, non-wrapping ranges, pipe fragments, ELF page
geometry, pending document edits/cancellation, and bounded integer programs. Models
use wide arithmetic, maps of latest valid document requests, or independently
computed guest results. Assert exact-once delivery and shutdown barriers without
fixing incidental lane order. Retain targeted tests for native faults and known
architectural encodings that a round trip through the same library cannot establish.
See [Proptest](https://proptest-rs.github.io/proptest/intro.html).

fast-check exercises exact frontend scalars, coherent observation reduction,
stale identities, Unicode scratch recovery, locale preference order, and readable
memory windows at address-space boundaries. Prefer constructive valid generators,
then separate malformed-input properties. Keep shrinking and persisted regression
seeds useful; do not ignore minimized failures as disposable build output.
See [fast-check arbitraries](https://fast-check.dev/docs/core-blocks/arbitraries/).

Process fixtures drain stdout/stderr concurrently, bound retained output, apply
failure deadlines, and kill/reap children before joining blocked I/O. Use controlled
promises or channel handoffs for races. Sleeps are not evidence that a task ran;
timeouts bound failures rather than define normal correctness.

## Browser and desktop

Vitest has a Node logic project and a real browser component project in
`vite.config.ts`, sharing SvelteKit and Paraglide. Browser tests use only Playwright
Chromium with `channel: 'chromium'` and `headless: true`, selecting
[new headless mode](https://playwright.dev/docs/browsers#chromium-new-headless-mode).
Do not install or configure Firefox, WebKit, or the separate Chromium headless shell.
Use semantic locators, actual keyboard input, and awaited visible outcomes.
[Svelte's testing guidance](https://svelte.dev/docs/svelte/testing) supports this
browser component approach. Worker ports are explicit fixtures, not fabricated
proof of native IPC.

Browser acceptance covers layout, keyboard behavior, locale switching, retained
editor history/search, and stale builds. Native acceptance must additionally cover
real IPC, worker discovery, both guests, failures, and static assets in a bundle.
A Vite preview is not a packaged WebView, and macOS development evidence is not a
Windows/Linux or signed-installation claim.

For future automated desktop runs, current [Tauri guidance](https://tauri.app/develop/tests/webdriver/)
recommends WebdriverIO with `@wdio/tauri-service`. Its embedded driver supports
Windows, Linux, and macOS; direct upstream `tauri-driver` supports Windows/Linux.
Use the embedded provider for a consistent initial host matrix, with automation
plugins behind a dedicated development/test feature absent from production builds.
Run core acceptance with real commands and the actual worker, without IPC mocking.
Do not duplicate renderer-only suites already covered by Vitest. The service and
plugins are not installed until the desktop harness is implemented.

## Established engine evidence

- Assembly: exact integer and branch bytes, final-layout absolute/PC-relative
  relocations, explicit operand widths, literal pools, macros, `.org`, alignment,
  Unicode diagnostic offsets, sections/symbols/zero-fill, relinking, and final-address
  boundaries. Host file/console access and dependent-library loading are rejected.
- Loading: sectionless ELF, file/BSS/padding separation, page congruence, permission
  conflicts, unsupported runtime headers, aggregate bounds, and the final page of
  the address space. Real guest stores cannot write RX pages.
- Execution: both integer banks, flags and instruction alias effects, arithmetic
  properties, whole-REP stepping, budgets, breakpoints before effects, loop re-arming,
  reset, coherent memory, fault classification, environment stops, and completion
  before an unmapped fetch. Instruction starts and native dispatches remain distinct.
- Worker: actual binary transfers, pipelined cancellation with one legal outcome,
  controls during outstanding assembly, reserved output capacity, shutdown barriers,
  writer loss, session generations, coherent subscriptions, and invalid replacement
  preserving the existing machine/subscription.
- Desktop: connection/view leases, real guest execution, withheld frontend credit,
  reattachment without mutation replay, stale callers, and a nonresponsive child
  whose admitted outcome is unknown before forced termination.

The compiled bridge uses C++23 with LLVM/LLD 23.1.1 and bundled Unicorn 2.1.5 in the
current macOS verification. Shared native builds are established. Source mapping
is explicitly unavailable despite retained DWARF; decoding does not establish an
instruction/extension execution matrix. Standard linking does not establish TLS,
ABI, OS services, imported executable, or dynamic runtime support.

## Remaining gates

Adversarial expansion and native allocation/CPU limits require stress and host
containment evidence. Output bounds, cooperative cancellation, sampled RSS cutoffs,
exception handling, and worker isolation each have different guarantees.

Add isolated fuzz targets for boundary parsers, state-machine properties for complete
experiments, Criterion workloads for measured bottlenecks, and multi-host CI when
those surfaces are implemented. nextest can orchestrate Rust suites without changing
what they prove. Run actual installed artifacts, native dependency/license audits,
signing/JIT checks, and production capability/CSP review before release. No unused
harness or placeholder configuration is required to record these gates.

The current Vitest/Vite combination emits a warning that the mock interceptor's
`configureServer` hook is ignored. These tests use explicit ports; they pass without
that hook, and the warning is not filtered. Treat future reliance on that facility
as unverified until upstream compatibility is resolved.

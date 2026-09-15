# Testing and acceptance

Tests should establish behavior, independent architectural facts and invariants.
Prefer the smallest test at the boundary that can detect a meaningful regression.
Do not freeze incidental DOM structure, object identity, zero-storage representation,
independent reply order or formatter output. Test counts and coverage percentages
are diagnostic information, not acceptance targets.
[Development](development.md#commands-and-ownership) owns command recipes;
[roadmap](roadmap.md#verification) owns dated results and outstanding gates.

## Suites

| Location                                          | Evidence                                                                            |
| ------------------------------------------------- | ----------------------------------------------------------------------------------- |
| `crates/core/tests`                               | Scalar/schema validity, address/permission rules, fragmented and incomplete framing |
| `crates/engine/tests`                             | Exact encodings, ELF geometry, real guest effects, CLI and worker processes         |
| `crates/engine/tests/unit`                        | Private scheduling, output reservations, cancellation and stream baselines          |
| `src-tauri/tests/unit`                            | Real worker supervision, leases, reattachment, unknown outcomes and kill/reap       |
| `tests/protocol`, `tests/i18n`, `tests/workbench` | Frontend properties, catalogs, recovery and browser interaction                     |

Rust `tests/unit` files are private `#[cfg(test)]` modules included through `#[path]`;
root integration tests exercise public APIs. Tests remain outside production source
without widening APIs just for access. Public session tests verify guest memory
protection through observable effects. See [Rust test organization](https://doc.rust-lang.org/book/ch11-03-test-organization.html).
SvelteKit includes root tests in its generated TS project; Vitest discovers them
explicitly. No production test helpers or duplicate test-only TS project are needed.

## Contracts and properties

Use [Proptest's guidance](https://proptest-rs.github.io/proptest/proptest/tips-and-best-practices.html)
and [fast-check arbitraries](https://fast-check.dev/docs/core-blocks/arbitraries/):
construct relevant inputs, retain shrinking and compare against an independent
oracle. Keep exact boundary examples alongside properties. Do not reproduce the
implementation in the expected result or merely round-trip two production helpers.

| Boundary                   | Independent evidence                                                                                                               |
| -------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| Addresses and wire scalars | Standard decimal/hex formatting and wider integer arithmetic                                                                       |
| Memory access              | Complete containment in one mapping and a permission bit-set model                                                                 |
| Framing                    | Published little-endian headers, binary chunk boundaries, arbitrary pipe fragmentation and truncated input                         |
| ELF loading                | Hand-built ELF64 headers, page envelopes, file bytes and observed zero-fill                                                        |
| Guest execution            | Generated add/subtract/XOR programs compared with wrapping `u32` arithmetic on both guests                                         |
| Build scheduling           | Latest valid request per document, cancellation and exactly-once delivery without assuming unrelated reply order                   |
| Observations               | Generated full/delta histories, independent complete samples, exact counters, retained baselines and stale identities              |
| Editor language            | Target-specific literal/comment examples and incremental parsing compared with a fresh parse after generated edits                 |
| Recovery                   | Unicode and incomplete inputs survive; source budgets count UTF-8 bytes; valid preferences survive independently of corrupt fields |
| Localization               | Catalog key/parameter agreement and ordered supported-language preferences                                                         |

Properties use bounded inputs so failures remain small and reproducible. Native
program generation has a lower case count than pure logic; it still executes real
LLVM/Unicorn work. Repeated runs must explore new inputs rather than permanently
pinning every property to one seed.

For a failure, preserve the minimized input and the printed replay information.
[Proptest persists regressions](https://proptest-rs.github.io/proptest/proptest/failure-persistence.html);
keep those files in version control. For fast-check, replay the printed `seed` and
`path` through `fc.assert` options, then retain the smallest meaningful regression.
Do not ignore failures, retry until green or weaken a contract to accommodate a fixture.

## Effects and fixtures

Frontend feature tests mirror `src/lib` ownership; editor and machine tests live
under their corresponding `tests/workbench` subdirectories. Shared DTO factories
live in `tests/fixtures/protocol.ts`; they supply data rather than implementing a
second worker or emulator. Browser ports reject unexpected commands. A component
fixture proves the frontend's use of a contract, while actual process tests prove
that the native implementation fulfills it.

Use white-box tests only where deterministic control adds evidence: holding a native
job at the handoff, returning output capacity, coalescing unsent samples and losing
a writer. Assert control priority and shutdown barriers, not FIFO order among
independent responses. Do not use a short absence-of-message timeout to infer that
a scheduler ran.

Process fixtures drain output concurrently, bound retained bytes and kill/reap before
joining blocked I/O. Interactive requests use one absolute reply deadline, so an
unrelated event cannot restart the timeout; transport failures retain their cause.
Polling child exit under a deadline is allowed. Sleeps must not stand in for a
state transition, and elapsed execution time is not a performance assertion.

## Browser coverage

`vite.config.ts` defines a Node logic project and a real browser component project,
both inheriting SvelteKit/Paraglide. Browser tests use Playwright **Chromium only**,
`channel: 'chromium'`, `headless: true`: [new headless mode](https://playwright.dev/docs/browsers#chromium-new-headless-mode).
Install with `--no-shell`; do not add Firefox, WebKit or the separate headless shell.
See [Vitest projects](https://vitest.dev/guide/projects) and
[Svelte testing](https://svelte.dev/docs/svelte/testing).

Follow [Vitest component testing](https://vitest.dev/guide/browser/component-testing)
and [Playwright best practices](https://playwright.dev/docs/best-practices): use
semantic locators, real input and awaited visible outcomes. Use the renderer's
[automatic cleanup](https://vitest.dev/api/browser/svelte) and clear storage between
cases; explicitly unmount only when testing lifecycle behavior.

Coverage includes locale changes with retained text/history/search, late builds,
corrupt scratch reattachment, actual draft unmount/reopen, appearance/focus mode,
completion/comment commands and native disabled controls. A workbench flow verifies
that assembling does not load, loading does not run, ELF completion metadata reaches
the load request, and a rejected reset retains the displayed machine. Further
keyboard/screen-reader acceptance, including toolbar focus when all actions begin
disabled, remains open. Use Bits UI
and native semantics; do not add a custom focus framework to satisfy a test.

Worker ports are explicit component fixtures. They do not establish native IPC,
ELF validity or actual machine effects. Use browser HMR for visual acceptance at
1280×820 and the desktop minimum 880×600, then inspect narrow layouts, zoom, both
languages, long values and keyboard focus. Do not publish synthetic fixture states
as evidence of native behavior.

The current Vitest/Vite combination warns that the mocks interceptor's
`configureServer` hook is ignored. The suite uses explicit ports and passes without
that hook; the warning is not filtered. Future reliance on that facility needs
upstream compatibility verification.

## Native evidence

- **Assembly:** independent integer/branch bytes, absolute and PC-relative relocation,
  operand widths, pools/macros/origins/alignment, Unicode offsets, symbols/zero-fill,
  repeated linking and address limits; host input/output and dependent-library access
  are rejected.
- **Loading:** sectionless ELF, BSS/padding separation, congruence and permission
  conflicts, unsupported runtime headers, aggregate bounds and the last address page;
  real guest stores cannot write RX pages.
- **Decoding/CLI:** independent instruction bytes and addresses, bounded complete
  prefixes, explicit invalid-byte locations, raw-stdin decode and separate usage errors.
- **Execution:** canonical registers, flags/alias effects, arithmetic properties,
  whole-REP stepping, instruction budgets, before-effect breakpoints/re-arming,
  reset, coherent memory, fault/environment outcomes and boundary completion.
- **Worker:** actual binary transfers, cancellation with one outcome, controls during
  assembly, reserved capacity, shutdown barriers, writer loss, generations and full/delta
  subscriptions; invalid replacement preserves the current machine/subscription.
- **Desktop:** leases, real execution, withheld frontend credit, reattachment without
  mutation replay and nonresponsive children producing unknown outcomes before kill/reap.

For UI changes, verify a fresh static desktop bundle as well as the browser:

1. Build/load each architecture's example, step, run to completion and inspect the
   expected register/memory effect (the built-in examples store 42).
2. Reset and verify initial state. Exercise a bounded loop, pause and stop.
3. Edit source or change target; confirm the old machine remains distinct and stale
   artifacts cannot replace current work. Exercise an assembly error.
4. Verify fonts, dynamic editor loading, both locales and focus behavior under the
   actual WebView origin/CSP. Test failure/restart when the supervisor changes.

These checks do not establish a full ISA/extension matrix, source mapping, TLS,
OS/ABI support or general imported-executable support. Native execution requires
permission for Unicorn JIT operations and host CPU/cache discovery. On Apple Silicon,
Unicorn 2.1.5 queries `hw.cachelinesize`; denying that query can trigger an unsupported
`CTR_EL0` fallback read and `SIGILL` before loading a guest. Run native tests with the
required host access and report sandbox restrictions separately from test results.
Browser success is not packaged-WebView success, and a local debug bundle is not
installed/signed distribution evidence.

## Automation and release gates

[Tauri's WebDriver guide](https://tauri.app/develop/tests/webdriver/) recommends
WebdriverIO with `@wdio/tauri-service`. Its embedded provider supports Windows,
Linux and macOS; direct upstream `tauri-driver` supports Windows/Linux. A future
native harness should use real IPC and the actual worker, keep automation plugins
behind a test-only feature and exclude them from production. Avoid duplicating
renderer suites. No desktop WebDriver harness is installed yet.

Release acceptance still needs multi-host CI, actual installed artifacts, dynamic
library discovery/bundling, dependency licenses, signing/JIT policy and production
capability/CSP checks. Add complete-workflow state-machine properties, isolated
parser fuzzing, adversarial expansion/resource tests and measured performance
workloads as those surfaces are developed. Output bounds, cooperative cancellation,
sampled RSS and OS quotas provide different guarantees; do not conflate them.

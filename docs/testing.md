# Testing and acceptance

Tests should establish behavior, independent architectural facts and invariants.
Prefer the smallest test at the boundary that can detect a meaningful regression.
Do not freeze DOM structure, object identity, internal storage, translation wording,
independent reply order or formatter output. Test counts and coverage percentages
are diagnostic information, not acceptance targets.
[Development](development.md#commands-and-ownership) owns command recipes;
[roadmap](roadmap.md#verification) owns dated results and outstanding gates.

## Suites

| Location                                          | Evidence                                                                                 |
| ------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| `crates/core/tests`                               | Scalar/schema validity, address/permission rules, fragmented and incomplete framing      |
| `crates/engine/tests`                             | Exact encodings, ELF geometry, real guest effects, CLI and worker processes              |
| `crates/engine/tests/unit`                        | Private scheduling, output reservations, cancellation and stream baselines               |
| `src-tauri/tests/unit`                            | Worker supervision, stream reconstruction, leases, unknown outcomes and bounded file I/O |
| `tests/protocol`, `tests/i18n`, `tests/workbench` | Frontend properties, catalogs, recovery and browser interaction                          |

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

| Boundary                   | Independent evidence                                                                                                                                                                      |
| -------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Addresses and wire scalars | Standard decimal/hex formatting and wider integer arithmetic                                                                                                                              |
| Memory access              | Complete containment in one mapping and a permission bit-set model                                                                                                                        |
| Framing                    | Published little-endian headers, chunk boundaries, fragmented/truncated input and invalid-header rejection before body reads                                                              |
| Assembly output            | Arbitrary byte payloads preserved in relocatable objects and linked images on both guests; fixed instruction/relocation fixtures                                                          |
| ELF loading                | Hand-built ELF64 headers, page envelopes, file bytes and observed zero-fill                                                                                                               |
| Instruction analysis       | Fixed effects, generated MOV/signed-branch operands, full-width destinations and independent extension encodings                                                                          |
| Batch CLI                  | Full-width arithmetic and memory from source/ELF/raw inputs; exit outcomes, sectionless ELF and completion symbols                                                                        |
| Live editing               | GPR isolation, alias edits compared with real MOV execution, flag preservation and guest conditions, PC/REP/breakpoint restart semantics, byte-patch/reset and stale-generation rejection |
| Guest execution            | Generated add/subtract/XOR programs compared with wrapping `u32` arithmetic on both guests                                                                                                |
| Build scheduling           | Latest valid request per document, cancellation and exactly-once delivery without assuming unrelated reply order                                                                          |
| Observations               | Both register banks, full-width breakpoint histories, independent complete samples, retained baselines and stale identities                                                               |
| Editor language            | Target-specific literal/comment examples and incremental parsing compared with a fresh parse after generated edits                                                                        |
| Source locations           | Unicode byte boundaries against CodeMirror text/line positions, explicit CRLF/BOM cases and invalid-boundary rejection                                                                    |
| File inspection            | Exact ELF extents, arbitrary 64-bit byte windows, standard filesystem I/O and documented size/encoding boundaries                                                                         |
| Recovery                   | Unicode and incomplete inputs survive; source budgets count UTF-8 bytes; valid preferences survive independently of corrupt fields                                                        |
| Localization               | Catalog key/parameter agreement and ordered supported-language preferences                                                                                                                |

Properties use bounded inputs so failures remain small and reproducible. Fixed
protocol boundaries use deterministic examples, not random selection from a short
list of constants. Bias wide-address generators toward the final page so overflow
branches are exercised routinely. File-budget tests state documented limits independently
of production constants; file I/O properties use standard filesystem reads/writes as
the oracle in each direction. Native program generation has a lower case count
than pure logic; it still executes real LLVM/Unicorn work. Repeated runs explore
new inputs rather than pinning every property to one seed.

For a failure, preserve the minimized input and the printed replay information.
[Proptest persists regressions](https://proptest-rs.github.io/proptest/proptest/failure-persistence.html);
keep those files in version control. For fast-check, replay the printed `seed` and
`path` through `fc.assert` options, then retain the smallest meaningful regression.
Do not ignore failures, retry until green or weaken a contract to accommodate a fixture.

## Effects and fixtures

Frontend tests mirror feature ownership. Shared data in `tests/fixtures/protocol.ts`
provides DTOs, not a second worker or emulator. Browser ports return explicit
scenario replies and reject unexpected commands; assertions verify outgoing mutations.
Component fixtures exercise the frontend contract; process tests verify its native
implementation.

Register component tests cover native input constraints; workbench tests cover
requests and authoritative replies. Native tests compare aliases with real MOV
effects, check that edits leave PC unchanged and exercise flag set/clear from the
opposite value.

Editing workflows use a small scratch fixture independent of built-in examples.
CodeMirror renders a viewport, not the entire document: verify long source through
assembly requests and source exports, including BOM and newline preservation.

Instruction analysis compares fixed encodings and generated operands with
architectural facts, including signed displacements, address wraparound and AArch64
bit-test branches with multiple immediate operands. Real worker tests verify that
analysis of either architecture, including rejected input, preserves the loaded
guest's registers, memory, status and execution counters. The
[capability samples](engine.md#verified-capability-samples) establish assembly and
recognition evidence, not extension execution support. Allow improved backend
metadata without freezing known omissions into the expected behavior.

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
both inheriting SvelteKit/Paraglide. Browser tests use
Playwright **Chromium only**, with `channel: 'chromium'` and `headless: true`
for [new headless mode](https://playwright.dev/docs/browsers#chromium-new-headless-mode).
Install with `--no-shell`; do not add Firefox, WebKit or the separate headless shell.
See [Vitest projects](https://vitest.dev/guide/projects) and
[Svelte testing](https://svelte.dev/docs/svelte/testing).

Follow [Vitest component testing](https://vitest.dev/guide/browser/component-testing)
and [Playwright best practices](https://playwright.dev/docs/best-practices): use
semantic locators, real input and awaited visible outcomes. Use the renderer's
[automatic cleanup](https://vitest.dev/api/browser/svelte) and clear storage between
cases; explicitly unmount only when testing lifecycle behavior.
Parameterize independent outcomes instead of mounting multiple cases in one test.
Settle injected promises and [Svelte updates](https://svelte.dev/docs/svelte/svelte#settled)
before asserting that stale UI is absent; a negative assertion alone can pass too early.
Read accessible names from the locale catalogs. Do not snapshot translations or
hard-code product wording in assertions; copy edits must not break behavior tests.

Browser coverage follows observable workflows:

- **Editing:** locale, font, wrapping, layout reset and focus-mode changes retain
  source/history/search; completion, comments and Unicode diagnostic navigation
  use the real editor. Stale builds cannot annotate current source.
- **Files and recovery:** draft unmount/reopen, corrupt scratch, exact exports,
  cancellation, failures, retries, import conflicts and callbacks after unmount.
  Long imported source reaches assembly/export unchanged; importing raw bytes never
  implicitly loads a machine.
- **Instructions:** byte-based pagination, native offset validation, explicit
  analysis/retry, late success/failure, input changes and reversions, locale
  retention and narrow-panel return without another decode. Re-decoding clears
  selection. Workbench tests also exercise the complete request/reply adapter.
- **Initial setup:** target-specific inputs and locale retention, exact full-width
  scalar conversion, add/remove controls, and rejection of invalid values before load
  followed by successful correction.
- **Execution:** assembly/import does not load, loading does not run, exact raw
  placement and bytes reach the request for both targets. Rejected controls preserve
  displayed state; pending loads retain later edits in both languages. Control replies
  without bytes retain captured memory, changed bytes invalidate decoded instructions,
  and reset clears the capture. Inspect at PC shares memory-form validation; corrected
  input submits the chosen range and clears the error without losing retained memory.
- **Live editing:** keyboard submission, overflow rejection, full-width stack-pointer
  writes, aliases, PC, individual flags and retained register selection. Switching
  away from a flag restores ordinary register input. Captured instruction rows send
  explicit PC writes; their PC marker changes only with a fresh memory capture.
  Pending writes disable submission. Rejected writes preserve displayed state for
  explicit retry; successful patches refresh memory. Both languages use the same
  behavior fixtures.

Broader keyboard/screen-reader acceptance remains open, including toolbar focus
when all actions begin disabled. Use Bits UI and native semantics without custom
focus patches. Component fixtures do not establish native IPC, ELF validity or
machine effects. Use browser HMR for visual acceptance at the default 1440×900 and
desktop minimum 880×600, then inspect 800px and 414px layouts, zoom, both languages,
long values and keyboard focus. Do not publish synthetic fixture states as evidence
of native behavior.

The current Vitest/Vite combination warns that the mocks interceptor's
`configureServer` hook is ignored. The suite uses explicit ports and passes without
that hook; the warning is not filtered. Future reliance on that facility needs
upstream compatibility verification.

## Native evidence

Engine and process suites exercise real LLVM, Unicorn, pipes and native file I/O.
The contract matrix above identifies their independent oracles. Keep these
cross-layer regressions covered:

- Assembler diagnostics retain Unicode offsets; source directives cannot read host
  files or write to protocol stdout.
- Both bundled sorting programs run at two link addresses, preserve input, produce
  the expected ordering and sum, and reproduce results after reset.
- Guest stores enforce mapping permissions; host patches preserve those permissions
  and invalidate executable translations, including at the address-space boundary.
- GPR/flag edits preserve REP continuation; explicit PC writes, including the same
  address, start a new instruction and rearm breakpoints. Fetch faults and completion
  remain execution outcomes rather than PC-write outcomes.
- Invalid replacement and stale-generation writes preserve the active machine.
- Controls remain serviceable during assembly and while observation credit is
  withheld. Shutdown drains accepted replies; reattachment never replays mutations.
- Child failures settle pending operations, report uncertain outcomes where necessary,
  and kill/reap the worker.

For UI changes, verify a fresh static desktop bundle as well as the browser:

1. Build/load each architecture's example, step, run to completion and inspect the
   sorted signed array in memory and the sum, 42, in RAX/X0. Inspect `total` for
   the stored sum. Reset and run again; the original read-only input must remain intact.
2. Configure initial registers and an extra mapped region for each guest; run code
   that consumes those values and stores a result. Reset and verify initial values,
   then reject an overlapping replacement without losing the machine. Exercise a
   bounded loop, pause and stop.
3. Edit source or change target; confirm the old machine remains distinct and stale
   artifacts cannot replace current work. Exercise an assembly error after Unicode
   source, navigate to its point and edit to clear it.
4. Import/export UTF-8 text and raw code; compare exact bytes. Inspect a known ELF
   code segment and imported bytes, change target/base, and verify stale results
   disappear. Analyze an instruction on each guest, switch locale and verify retained
   details without changing machine state. File selection/cancellation must not load
   or run a machine.
5. Load imported raw code with explicit base, entry and completion. Add a breakpoint,
   run to it, inspect captured memory/PC, toggle a decoded row and resume. Reset must
   restore initial registers and keep breakpoints; invalid entry must preserve the
   machine. Repeat for both targets and languages without replacing source.
6. While ready/paused, write a full-width GPR and patch data/code from the memory
   toolbar. Inspect fresh bytes and execute them; reset must restore initial state.
   Exercise subregister preservation/zero-extension, condition flags, PC redirection
   and a decoded row's Set next instruction action. Verify rejected input, both
   languages and native WebView hex parsing.
7. Verify fonts, dynamic editor loading, both locales and focus behavior under the
   actual WebView origin/CSP. Test failure/restart when the supervisor changes.

The [engine contract](engine.md) defines the supported runtime and metadata limits.
Native execution requires permission for Unicorn JIT operations and host CPU/cache
discovery. On Apple Silicon, Unicorn 2.1.5 queries `hw.cachelinesize`; denying that
query can trigger an unsupported `CTR_EL0` fallback read and `SIGILL` before loading
a guest. Run native tests with the required host access and report sandbox
restrictions separately from test results.
Browser success is not packaged-WebView success, and a local debug bundle is not
installed/signed distribution evidence.

## Automation and release gates

[Tauri's WebDriver guide](https://tauri.app/develop/tests/webdriver/) recommends
WebdriverIO with `@wdio/tauri-service`. Its embedded provider supports Windows,
Linux and macOS; direct upstream `tauri-driver` supports Windows/Linux. No desktop
WebDriver harness is installed yet. When adding one, follow the current plugin setup,
exercise real IPC and the actual worker, and keep automation behind a test-only
feature. Avoid duplicating renderer suites or including automation in production.

The [release gates](roadmap.md#release-gates) track multi-host CI, installed-artifact
acceptance, containment and performance work. Output bounds, cooperative cancellation,
sampled RSS and OS quotas provide different guarantees.

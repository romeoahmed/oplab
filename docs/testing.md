# Testing and acceptance

Test behavior, independent architectural facts and invariants at the smallest useful
boundary. Do not freeze incidental DOM structure, translation wording, object identity,
formatter output or unrelated reply order. Counts and coverage percentages are
diagnostics, not acceptance targets. [Development](development.md#commands-and-ownership)
owns commands; [roadmap](roadmap.md#verification) records dated results and gaps.

## Suites

| Location                 | Evidence                                                                     |
| ------------------------ | ---------------------------------------------------------------------------- |
| `crates/core/tests`      | Scalar/schema validity, address/permission rules and framing                 |
| `crates/toolchain/tests` | Exact encodings, ELF geometry, DWARF points, relocations and static metadata |
| `crates/runtime/tests`   | CPU lifecycle, memory permissions, REP and self-modifying code               |
| `crates/engine/tests`    | Loading, session policy, live edits and real guest/SIMD effects              |
| `crates/runner/tests`    | CLI/worker processes and private concurrency boundaries                      |
| `src-tauri/tests/unit`   | Supervision, leases, stream reconstruction and native file I/O               |
| Root `tests/`            | Frontend properties, recovery, catalogs and browser interaction              |

Rust public tests use package `tests/`; private `#[cfg(test)]` path modules use
`tests/unit` without widening APIs. See [Rust test organization](https://doc.rust-lang.org/book/ch11-03-test-organization.html).
Frontend tests mirror feature paths and belong to SvelteKit's generated TS project.
Shared fixtures contain DTOs, not a second worker or emulator. Injected browser
ports return explicit scenario replies, reject unexpected calls and expose outgoing
mutations for assertions; native process tests verify their implementation.

## Contracts and properties

Follow [Proptest](https://proptest-rs.github.io/proptest/proptest/tips-and-best-practices.html)
and [fast-check](https://fast-check.dev/docs/core-blocks/arbitraries/): construct
relevant bounded inputs, retain shrinking and use independent oracles. Keep exact
boundary cases alongside generated cases. Do not duplicate the implementation or
only round-trip two production helpers. Compare serialized vector bytes with an
independent hexadecimal representation; combine mixed instruction lengths when
checking disassembly boundaries.

| Boundary            | Independent evidence                                                                                   |
| ------------------- | ------------------------------------------------------------------------------------------------------ |
| Addresses/scalars   | Standard formatting, wider arithmetic and the final address-space page                                 |
| Memory              | Complete containment in one mapping, bit-set permissions and observed faults                           |
| Framing             | Published headers, fragmentation/truncation and rejection before body reads                            |
| Assembly/ELF        | Fixed encodings/relocations, arbitrary byte payloads, hand-built ELF headers and zero-fill             |
| Source provenance   | Relocated points, Unicode, macro/repeat attribution, gaps, foreign files and complete truncated groups |
| Static analysis     | Architectural effects, generated MOV/branch operands and independent extension encodings               |
| Execution/editing   | Scalar reference results, real MOV alias behavior, flag preservation, PC/REP restart and reset         |
| SIMD                | Full YMM/Z/P/FFR banks, alias preservation, AVX2/SVE lanes, rounding and independent guest stores      |
| Scheduling/delivery | Latest valid pending build, exactly-once outcomes, retained stream baselines and stale identities      |
| Editor              | Incremental versus fresh parsing, GNU blocks, literal labels and Unicode byte/UTF-16 boundaries        |
| Presentation        | Exact lane reconstruction, IEEE-754 fixtures, signed zero, NaN, infinities and subnormals              |
| Files/recovery      | Standard filesystem I/O, exact exports, UTF-8 budgets and preservation of the last valid draft         |
| Localization        | Key/parameter-set agreement and supported-language preference order                                    |

Enumerate small finite domains such as SVE lengths; generate bit patterns and
overlapping edit sequences with shrinking. Check active views separately from
full-width storage, including predicates and low aliases. Half-precision expectations
use IEEE-754 fields and explicit ties-to-even/subnormal cases.

Deterministic examples suit fixed protocol limits; random selection from a short
constant list adds little. State policy limits independently of production constants;
verify acceptance at the limit and rejection beyond it. Native generation uses fewer
cases than pure logic but still executes the real backend. Preserve minimized regressions and replay data:
[Proptest persistence](https://proptest-rs.github.io/proptest/proptest/failure-persistence.html)
belongs in version control; fast-check prints the `seed` and `path` for replay.
Do not retry until green or weaken a contract to accommodate a fixture.

Use white-box control where it adds evidence: holding a job at a handoff, returning
output capacity, coalescing samples or losing a writer. Assert barriers and control
priority, not FIFO order among independent replies. Process fixtures drain output
concurrently, bound retained bytes and kill/reap before joining blocked I/O. One
absolute deadline bounds an interactive reply, regardless of unrelated events.
Polling for a terminal state shares one deadline across replies. Sleeps or a brief
absence of messages do not prove that a transition occurred.

Use Unicode escapes for non-ASCII encoding fixtures. Cover one- through four-byte
UTF-8 scalars at the byte budget and on either side; source length in UTF-16 code
units is not a byte count.

## Browser coverage

`vite.config.ts` defines Node logic and real-browser component projects, inheriting
SvelteKit/Paraglide. Use Playwright **Chromium only**, `channel: 'chromium'` and
`headless: true` for [new headless mode](https://playwright.dev/docs/browsers#chromium-new-headless-mode).
Install with `--no-shell`. Follow [Vitest component testing](https://vitest.dev/guide/browser/component-testing),
[Svelte testing](https://svelte.dev/docs/svelte/testing) and
[Playwright best practices](https://playwright.dev/docs/best-practices).

Use semantic locators, real input, awaited visible outcomes and the renderer's
[automatic cleanup](https://vitest.dev/api/browser/svelte). Clear storage between
cases; explicitly unmount only for lifecycle tests. Settle injected promises and
[Svelte updates](https://svelte.dev/docs/svelte/svelte#settled) before negative
assertions. Read accessible names from catalogs rather than hard-coding copy.
Translation placeholders may repeat without changing the required parameter set.
Scope role locators by accessible name; include hidden elements explicitly when
checking visibility transitions instead of retaining a previous DOM node.
Parameterize independent outcomes. Use small editor documents instead of coupling
workflow tests to bundled examples; verify long documents through assembly/export
because CodeMirror renders only a viewport.

Keep these observable workflows covered:

- **Editor:** blank/empty-draft recovery, explicit examples, source/history/search
  retention across locale, font, wrapping and focus changes; completion, comments,
  F12 labels, GNU folding and Unicode diagnostics. Search covers literal backslashes,
  invalid regex, case/whole-word toggles, no-match controls, keyboard replacement and
  undo. Line navigation preserves source/search state and supports relative positions.
  Completion includes AVX2/SVE hints. Stale builds cannot annotate current text.
  Source breakpoints cover partial groups, pending/rejected updates, retry and
  invalidation after edits; generated source maps preserve every exact relation.
- **Files/recovery:** exact BOM/newline export before editing, long UTF-8 imports,
  cancellation, failure/retry, import conflicts and callbacks after unmount. Oversized
  edits preserve the previous recoverable draft. Binary import never loads a machine.
- **Instructions:** byte-based paging, offset validation, explicit analysis/retry,
  late outcomes, input changes/reversions and locale retention. Re-decoding clears
  selection; returning from narrow analysis does not decode again. Superseded
  decode success/failure cannot clear pending state or replace a newer result,
  regardless of completion order.
- **Machine/setup:** tab switching retains SIMD format and unfinished input. Exact
  values and unavailable state render correctly. Initial GPRs/mappings retain
  per-target input and reject malformed values before load. Opening a view has no
  machine effect.
- **Execution:** assembly does not load and loading does not run. Pending loads use
  captured inputs while later edits remain editable. Rejected controls retain state;
  replies without bytes retain memory, changed bytes invalidate disassembly, and
  reset clears the capture. Inspect at PC shares native memory-form validation.
- **Live edits:** keyboard submission, overflow/flag constraints, aliases, PC,
  memory, SIMD lanes and rounding. Pending requests prevent duplicate submission,
  including Enter from an editable field. Rejected writes retain authoritative values
  for retry; new observations preserve typed SIMD drafts. Both languages share the
  same behavior fixtures.

For visual acceptance, use HMR at 1440×900 and the desktop minimum 880×600, then
check 800px/414px layouts, zoom, long values and both languages. Inspect the floating
upper-right search and line-navigation panels, including simultaneous display,
replacement, wrapping, keyboard focus and retained drafts after closing/reopening
observations. Broader keyboard and screen-reader acceptance remains open, including toolbars whose actions start disabled;
prefer native semantics and Bits UI over focus patches.

Component fixtures do not establish native IPC or guest behavior. Known tool warnings
and the scope of completed visual/native checks belong in the
[verification record](roadmap.md#verification).

## Native evidence

Native suites use real LLVM/QEMU, process pipes and filesystem I/O. They must cover
actual guest effects, permission boundaries, REP continuation, executable-cache
invalidation, failed replacement, reset and process loss. Validate output isolation
and controls under assembly/output backpressure. Stream tests preserve the last
valid baseline after rejected deltas and exercise both bank replacement and inheritance.

A recognized encoding or passing sample does not establish an entire extension.
Allow backend metadata to improve without freezing known omissions. The
[runtime scope](runtime.md#coverage-and-limits) and [engine contract](engine.md)
define current limits.

For relevant UI/native changes, verify a freshly built desktop bundle:

1. Assemble/load each bundled example; step, run and inspect `output`, RAX/X0 and
   `checksum` (4814 / `0x12ce`). Reset and repeat; read-only input remains intact.
2. Configure initial registers/mappings on both guests. Reject an overlapping
   replacement without losing state; exercise a bounded loop, pause and stop.
3. Navigate between source and instructions; set, resume and remove line breakpoints.
   Edit source/target and verify stale mappings/diagnostics clear without changing
   the loaded machine. Check Unicode diagnostic navigation.
4. Import/export exact UTF-8 and raw bytes. Inspect ELF segments and captured memory,
   analyze both architectures, change locale and verify retained state.
5. Load raw code with explicit placement, stop at a breakpoint, resume and reset.
   Invalid entry preserves the machine; import alone neither loads nor runs.
6. Edit GPRs, aliases, flags, PC and data/code; inspect fresh effects and reset.
   Exercise full YMM/Z and low XMM/V writes, highest lanes, P/FFR bits,
   f16/f32/f64, signed zero/raw NaNs and directed rounding,
   including rejection and stale writes in both languages.
7. Check fonts, dynamic editor loading, focus and worker failure/restart under the
   actual WebView origin/CSP.

LLVM ORC requires executable-memory permission. Changes to native ownership need
QEMU adapter lifecycle/reset/concurrent-session tests with AddressSanitizer; record
known upstream limitations without hiding diagnostics. Report sandbox restrictions
separately. Browser success is not packaged-WebView success, and a local debug
bundle is not signed or independently installed distribution evidence.

## Automation and release gates

No desktop WebDriver harness is installed. Follow
[Tauri's WebDriver guide](https://tauri.app/develop/tests/webdriver/) when adding one,
and verify its host support. The harness must exercise real IPC and the worker
without duplicating component suites or shipping automation in production.

[Release gates](roadmap.md#release-gates) cover multi-host CI, installed artifacts,
containment and measured performance. Output bounds, cooperative cancellation,
sampled RSS and OS quotas provide different guarantees.

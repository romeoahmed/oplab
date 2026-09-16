# Protocol and CLI

This document defines the worker protocol, desktop boundary and CLI output.
Rust declarations in `oplab-core::protocol` own the wire contracts;
`cargo xtask codegen [--check]` exports or verifies their TypeScript declarations.
[Engine](engine.md) owns machine semantics and artifact limits;
[development](development.md#native-toolchain) owns CLI build requirements.

## Framing

Every frame begins with an eight-byte header:

| Offset | Field       | Encoding                                                 |
| ------ | ----------- | -------------------------------------------------------- |
| 0–1    | Magic       | ASCII `OP`                                               |
| 2      | Kind        | `0` JSON control, `1` binary, `2` JSON observation event |
| 3      | Flags       | Zero                                                     |
| 4–7    | Body length | Unsigned 32-bit little-endian                            |

JSON bodies are bounded to 1 MiB and binary bodies to 64 KiB; empty bodies are
invalid. Validate each header before reading its body and bound allocations by
the frame and complete-payload budgets. Handle short reads/writes normally.
EOF before a header is clean shutdown; EOF inside a frame or declared payload is
failure. Malformed input closes the connection without scanning for another magic.
Stdout contains protocol bytes only; stderr uses static failure categories.

Binary data follows the control/event that declares it. Complete messages are
contiguous: no other reply or event can interleave payload frames.
A header or JSON reply alone never completes a message with outstanding binary data.
Reject an unexpected payload kind or chunk length before waiting for its body.

## Negotiation and identity

The private development protocol stays at version **1**. Desktop, CLI and worker
are built together against one current schema; there is no compatibility or migration
layer for earlier builds. Rebuild and restart native processes after contract changes.

Send `hello` with version `1` first. The reply advertises targets, assembler identity,
execution support and source-mapping availability. A mismatched handshake receives
an error and closes without waiting for stdin EOF. This checks the protocol identifier,
not build identity; version 1 alone does not establish cross-build compatibility.

Request IDs are positive, strictly increasing canonical decimal strings scoped to
a connection. Reuse closes the connection without a duplicate-ID reply. Tagged
commands reject unknown fields/variants. Every accepted request has one terminal
reply unless the connection fails; a disconnect cannot establish success or justify
automatically retrying a mutation. Replies may arrive out of order, so correlate IDs.

| Value                                        | Wire representation                                  |
| -------------------------------------------- | ---------------------------------------------------- |
| Address                                      | `0x` plus sixteen lowercase hexadecimal digits       |
| 64-bit counter or GPR value                  | Decimal string, no leading zeros except `0` itself   |
| Unavailable diagnostic address/source offset | Explicit `null`                                      |
| Source offset                                | Original UTF-8 byte position, not a source range/map |

Exact scalar values are bounded to unsigned 64-bit ranges; JSON numbers are rejected
where exact strings are required. Human input is not constrained to wire spelling.
Build identity includes document, revision, target, base and assembler name/version.
Compare the entire identity before displaying a build.

## Artifacts and sessions

An `assembled` reply declares `object_bytes`, `image_bytes`, build identity and a
bounded ELF-derived image view. Each file is 1 byte–1 MiB. Relocatable ELF follows
first, linked ELF second, in 64-KiB binary chunks except the final remainder.
There is no padding or terminator. Validate kind, order and exact lengths before
publishing; reject extra frames and discard incomplete artifacts on failure.
ELF remains authoritative for symbols, relocations, debugging and program headers.
The bounded image view exposes named address symbols, excluding undefined, file,
section and TLS entries. TLS offsets remain in the complete ELF; they are not
virtual addresses or desktop stop positions.

`load` declares `image`, target, completion, instruction budget, image length,
`initial` and `replace`, followed by a binary payload of 1 byte to 1 MiB.
`image: {type: "elf"}` uses standard ELF addresses, entry and permissions.
`image: {type: "raw", data: {base, entry}}` maps exact bytes RX at `base`, with an
explicit entry inside those bytes; both addresses use canonical hexadecimal.
No guest memory is mapped until the transfer and loader validation succeed.
`replace: null` requires no session; replacement requires the current exact key and
a non-running state. Failure preserves the old machine.
Success uses the load request ID as session ID and starts generation zero.

`initial` contains `registers: [{name, value}]` and
`mappings: [{address, length, flags}]`. Register values are canonical decimal strings;
names must be distinct lowercase canonical GPRs for the selected target. Omitted
GPRs are zero. Both arrays are required, including when empty. Mapping addresses
use canonical hexadecimal; lengths are byte counts. Flags use ELF `PF_R=4`,
`PF_W=2`, `PF_X=1`; zero describes a guard region. Extra regions are zero-filled.
The [loader](engine.md#raw-code-and-initial-conditions) checks page geometry,
overlap, target and aggregate limits before native mapping.
These values are load inputs, not live patches, and reset reapplies them.

`execute` carries a session key and `run`, `step`, `pause`, `cancel`, `reset`,
`breakpoint`, `write_register`, `write_memory`, `observe` or `close`. Keys contain
decimal `session` and `generation`.
Stale keys fail before mutation. Reset advances generation; close drops the owner,
even when running, and returns `session_closed`. IDs must also be qualified by the
worker connection, because they can recur after restart.

`write_register` carries `{name, value}`. Names accept canonical GPRs, subregisters,
RIP/PC and individual application flags; values use exact decimal strings, including
for RIP/PC. The [register policy](engine.md#live-editing) defines widths, preservation
and PC restart behavior. Initial setup remains canonical-only.
`write_memory` carries `{address, length}` and 1–65,536 following binary bytes;
the declared length must match exactly. The desktop form limits each patch to
4 KiB. Tauri and the worker share payload-length validation. Both write operations
require Ready/Paused state and follow the
[live-editing contract](engine.md#live-editing).

Write acknowledgements contain a full register/control observation without memory,
so a successful mutation does not depend on bulk-output capacity. Observe or
subscribe separately for fresh bytes. A native write/cache failure returns a
Crashed observation with no registers; framing or process loss retains the usual
unknown-outcome semantics. Never replay a mutation to recover a missing observation.

Other successful controls return `observed`: a complete snapshot containing key,
increasing sequence, status, instruction/dispatch counters, canonical registers,
fault metadata, sorted distinct `breakpoints` (at most 256) and optional memory
metadata. Breakpoints survive reset; a new load starts with an empty set.
Sequence begins at one, continues across reset and never wraps. Run/step acceptance
can report Running; poll or subscribe to learn completion. Newer observations can
precede older correlated replies, so check key/generation/sequence before updating
a live view.

Memory windows contain 1–65,536 bytes within one mapping and transfer after the
snapshot. Registers, memory and counters are captured coherently at one owner boundary.
64-bit register storage uses decimal strings; PC uses the canonical hex address.
Raw flags carry no definedness claim. Crashed snapshots have null registers and
cannot supply fresh memory. Publish only after the entire transfer validates.

The execution owner has one bounded command slot and one reply slot per accepted
command. It polls between slices, advances after servicing commands, and blocks
when stopped. Shutdown/EOF disconnect and join the owner. A native hang requires
the desktop supervisor, not a detached replacement thread.

## Instruction inspection

`decode` supplies a target, canonical base address, 1–65,536 bytes and an instruction
limit of 1–4,096. Its JSON-only `decoded` reply contains each instruction's address,
original bytes and display text. Invalid or incomplete instructions before the
limit fail the whole request; no partial prefix is returned. Bytes beyond the
limit are not decoded. Consumers advance by returned byte lengths, not row counts.

Both `decode` and `analyze` carry bytes as JSON arrays, not trailing binary frames.
Input ranges cannot wrap; AArch64 base addresses must be four-byte aligned.

### Static analysis

`analyze` supplies a target, canonical base address and the exact bytes of one
instruction (1–15 for x86_64, four for AArch64). Unlike `decode`, it does not accept
an instruction stream or a row limit. Invalid/trailing input receives a correlated
`decode` diagnostic; successful `analyzed` replies carry JSON only.

`InstructionAnalysis` retains register aliases, conditional access categories,
optional memory widths and direct destinations. Its architecture union preserves
x86 CPUID/flag/control facts separately from AArch64 groups/flags/writeback.
Unknown access is distinct from an absent entry. Missing metadata does not establish
that an effect is absent; see the
[analysis limitations and capability samples](engine.md#static-analysis).

Requests carry no session key. Consumers associate each response with its input
and selection; invalidated responses must not replace the current view. Analysis
results neither change the loaded machine nor establish source locations. The
[frontend ownership rules](architecture.md#interface) define presentation
and locale retention.

## Subscriptions

`subscribe` supplies an exact key and optional memory window. Its request ID becomes
the subscription ID. Only one is active per connection. Replacement captures and
validates the new window before replacing the old subscription; invalid requests
preserve the old one. `subscribed` precedes the first event. Subscribing does not
start or pause execution.

The dispatcher captures immediately, after successful controls, and approximately
every 33 ms while the last sample is Running. Incoming requests still check the
sampling deadline. Stopped sessions are not periodically polled. This cadence is
observation pacing, not a native deadline or guaranteed display rate.

Kind `2` carries `StreamEvent { subscription, update }`, without a request ID:

| Update  | Meaning                                                                                                    |
| ------- | ---------------------------------------------------------------------------------------------------------- |
| `full`  | Complete observation followed by its optional binary memory                                                |
| `delta` | Exact key, baseline `base`, new sequence, current status/counters/fault, register update and memory length |
| `ended` | Structured capture failure; ends the subscription without itself mutating the guest                        |

A delta retains or replaces the complete register bank, including explicit null
after native loss. Zero memory length retains the baseline bytes; nonzero length
replaces the entire window. Breakpoints come from the baseline. Changes to the
window metadata or breakpoint set require `full`. There are no individual-register,
breakpoint or byte-range patches.

Only complete samples coalesce, in one dedicated output slot. The writer computes
a delta against the last **delivered** sample and advances that baseline only after
all bytes are written. If samples 2 and 3 collapse into 4, a client with sample 1
receives `base: 1`. A new subscription always begins with a full sample.

Consumers maintain a stream baseline distinct from correlated `observed` replies.
Validate connection, subscription, key and increasing sequence. Gaps are legal;
a delta with an unknown base requires resubscription for a full sample. Consume
complete events into the baseline before coalescing renders. Partial or mismatched
binary payloads must never publish partial machine state.

`unsubscribe` requires the exact ID. Replacement, successful reset/load/close,
shutdown and EOF clear pending samples. An in-flight event finishes before the
invalidating reply; none follows it. Failed controls retain the subscription.
Reset/load require explicit resubscription to the returned key. Capture failures
use `ended`; explicit invalidation is conveyed by the relevant control reply.
Synchronous `Worker::handle` rejects subscription operations because it has no
unsolicited output stream.

## Scheduling and backpressure

Independent threads own native assembly, native execution and blocking pipe I/O.
The dispatcher admits one active build and up to eight pending builds, one per
document. A newer valid pending build replaces its predecessor with `superseded`;
it does not implicitly cancel active work. Invalid input cannot replace valid work.
A full pending queue rejects a new document with `resource_limit`, while replacement
for an already queued document remains possible. Controls and instruction inspection
do not await assembly.

`cancel_assembly` names an outstanding request. If cancellation wins publication,
the original receives `cancelled` and the cancel request receives `assembly_cancelled`
with its ID. Otherwise an unknown/already published/repeated cancellation receives
`invalid_input`. Exactly one build outcome is published. Pending work is removed;
active work sees a latch before compile, between compile/link and after link. The
active slot remains occupied until its native call returns.

Inbound events are bounded to 16 slots. Output uses bounded control and bulk lanes,
a reserved bulk slot for each admitted native build, and a separate observation slot.
The writer chooses whole messages in this order:

1. Control replies and errors.
2. Paused/terminal observations and capture failures.
3. Bulk artifacts, decoded windows, instruction analyses and memory-bearing Ready/Running replies.
4. Ready/Running subscription samples.

An in-flight message finishes first. Priority does not imply a hard latency guarantee.
Other bulk traffic cannot consume a build reservation. Without an unreserved slot,
a decode/analysis/memory-read result becomes a correlated `resource_limit` error;
a completed capture can therefore leave a sequence gap. Builds wait for capacity
while controls remain serviceable. Writer wakeups never block behind a full
inbound queue.

Control saturation applies backpressure instead of dropping outcomes. Clients must
drain output while submitting work. Writer loss wakes producers with failure.
Output bounds exclude the complete message being written and the last delivered
observation baseline, including at most one 64-KiB memory window.

Shutdown is the final input request. It stops execution, drains accepted assembly
and output, then sends `closed` from a separate final slot. Worker threads join before
exit. EOF drains without a fabricated request/reply. Malformed input or owner failure
exits with uncertain state rather than continuing. Embedded callers use synchronous
engine APIs; the pipe runtime owns process-level lifetime.

## Desktop integration

Four main-window Tauri commands manage the worker. Two separate commands handle
[file import/export](#desktop-file-boundary).

| Command          | Responsibility                                                                                                                         |
| ---------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| `worker_connect` | Attach a Channel and obtain capabilities, connection/view lease and retained metadata; explicit restart creates a new worker           |
| `worker_request` | Send a binary framed `DesktopCall` plus optional load/patch bytes; supervisor assigns worker IDs and returns the complete framed reply |
| `worker_ack`     | Grant credit for a delivered subscription/sequence                                                                                     |
| `worker_detach`  | Invalidate the view without replaying or cancelling admitted mutations                                                                 |

Reattachment issues a new lease while preserving the worker. Zero, detached and
obsolete leases cannot mutate it. Source association is not inferred from retained
session/artifact metadata.

| Supervisor policy                      | Value                         |
| -------------------------------------- | ----------------------------- |
| Outstanding requests                   | 16                            |
| Assembly request deadline              | 15 seconds                    |
| Other request deadline                 | 3 seconds                     |
| Continuously observed Running deadline | 60 seconds                    |
| RSS cutoff / sample interval           | Above 1 GiB / 25 ms           |
| Drained stderr allowance               | 64 KiB, contents not retained |
| Graceful shutdown before kill/reap     | 2 seconds                     |

These are sampled supervisory cutoffs with scheduling latency, not OS quotas.
Missing RSS samples do not prove compliance. Failure settles requests once; any
request whose pipe write began reports `outcome_unknown: true`. Mutations are never
retried automatically. A dead worker needs explicit restart and loses its machine.

Worker deltas are reconstructed before WebView coalescing. The Channel keeps one
unacknowledged observation and one latest pending sample; replies and a small failure
notice remain independent. Frontend leases and subscription serialization reject
stale delivery. [Tauri Channels](https://tauri.app/develop/calling-frontend/) provide
transport; application credit supplies the bound.

## CLI

The [clap CLI](https://docs.rs/clap/latest/clap/_derive/_tutorial/index.html) reads
stdin and shares the native engine operations. Addresses accept ordinary hexadecimal
input, including `0x1000` and `0XFF`. Help and argument validation precede input reads.
Usage errors exit 2; operational or output failures exit 1.

| Command                             | stdout                   | Operational errors                            |
| ----------------------------------- | ------------------------ | --------------------------------------------- |
| `assemble`                          | Complete linked ELF      | JSON diagnostic on stderr; stdout stays empty |
| `capabilities`, `decode`, `analyze` | Correlated JSON response | JSON on stdout                                |
| `run source`, `run elf`, `run raw`  | Batch JSON report        | JSON on stdout                                |

For assembly and inspection, input read, size and encoding failures produce a static
message on stderr. Batch execution reports those failures as JSON. JSON output ends
with a newline; raw source excerpts and host paths are excluded from diagnostics.

```sh
cargo run --locked -p oplab-engine --bin oplab-cli -- capabilities
cargo run --locked -p oplab-engine --bin oplab-cli -- assemble x86_64 0x1000 < experiment.s > experiment.elf
cargo run --locked -p oplab-engine --bin oplab-cli -- decode aarch64 0x1000 < code.bin
cargo run --locked -p oplab-engine --bin oplab-cli -- analyze x86_64 0x1000 < instruction.bin
```

### Batch execution

`run source` compiles unchanged UTF-8 stdin (up to 256 KiB), links at the supplied
base, then executes that image. `run elf` accepts a complete static ELF64 executable
(up to 1 MiB), checks the selected guest, and uses its program headers and `e_entry`
without relocating it. Ordinary section headers are optional; symbol-based completion
requires a symbol table, and extended program-header counts require section zero.
`run raw TARGET BASE` maps unchanged binary stdin (1 byte–1 MiB) RX. `--entry`
defaults to BASE; it must be aligned and fit inside the input. Use `--until`, since
raw bytes have no ELF symbols. All modes share [loading](engine.md#loading) and
[session policy](engine.md#execution); successful assembly alone does not imply
a loadable runtime image.

```sh
cargo run --locked -p oplab-engine --bin oplab-cli -- run source x86_64 0x1000 --until-symbol done --budget 100 < experiment.s
cargo run --locked -p oplab-engine --bin oplab-cli -- run elf aarch64 --until 0x1008 --budget 100 --memory 0x1000 --memory-bytes 8 < experiment.elf
```

| Option                                    | Contract                                                                                                                                  |
| ----------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| `--until ADDRESS` / `--until-symbol NAME` | Exactly one is required. Stop before fetching the resolved address; it must be aligned and distinct from the entry.                       |
| `--entry ADDRESS`                         | Raw mode only: explicit initial fetch address, defaulting to BASE.                                                                        |
| `--register NAME=VALUE`                   | Repeat for distinct canonical GPRs, including RSP/SP. Values accept decimal or `0x` hex; unspecified GPRs are zero.                       |
| `--map ADDRESS:SIZE:PERMISSIONS`          | Repeat for additional zero-filled page-aligned regions. Size accepts decimal or `0x` hex. Permissions are `r`, `w`, `x` in order, or `-`. |
| `--budget N`                              | Required instruction-start limit, 1–100,000,000. REP iterations count as one instruction.                                                 |
| `--timeout-ms N`                          | Cooperative execution limit, 1–3,600,000 ms; default 10,000.                                                                              |
| `--memory ADDRESS`                        | Include one final mapped memory window; validated before execution.                                                                       |
| `--memory-bytes N`                        | 1–65,536 bytes, default 64 when observing memory. Explicit use requires `--memory`.                                                       |

A completion symbol must name exactly one entry in the ordinary ELF symbol table,
defined in an existing section or as an absolute value. Missing, undefined, unallocated
common, ambiguous and file symbols fail. [TLS symbols](https://gabi.xinuos.com/elf/05-symtab.html#symbol-type)
are rejected because their values are offsets, not virtual addresses. An explicit
address needs no symbols. Neither form establishes source provenance or an instruction
boundary; completion is an explicit control policy.

Each invocation owns one session and captures final state after execution. PC comes
from ELF or the explicit raw entry; flags retain backend defaults. Added mappings
cannot overlap image pages or widen their permissions. No stack, return address,
host ABI or system services are supplied implicitly.

For example, a source document can use `push rdi; pop rax` with `--register rdi=42`,
`--register rsp=0x9000` and `--map 0x8000:4096:rw`, ending at its declared completion.
The caller supplies the stack and argument; Oplab adds no call or return sequence.

The timeout begins after input, assembly, loading and initial observation validation.
It is checked between slices using Rust's monotonic `Instant`; a native call may
overrun it. A terminal outcome reached within the last slice takes precedence over
the next timeout check. Timeout cancels the session and retains partial effects.
Native hangs, input stalls and assembler expansion require an external process
supervisor; this CLI does not inherit the desktop supervisor's deadlines or RSS limits.

`run` writes one newline-terminated JSON object to stdout:

- `{"type":"executed","data":{...}}` contains `target`, resolved `completion`,
  `outcome`, decimal-string `instructions`/`dispatches`, canonical `registers`,
  nullable `fault`, and nullable `memory: {address, bytes}`. Memory uses a bounded
  JSON byte array; register/fault shapes and exact scalars match the worker contract.
- `outcome` is `completed`, `budget`, `guest_fault`, `unsupported_environment` or
  `timeout`. Non-completion outcomes exit 1 and retain final effects.
- `{"type":"error","data":{...}}` uses the standard `Diagnostic` for rejected
  input, assembly/loading failures or an unavailable native observation. It exits 1;
  no machine state is fabricated after a backend failure.

Batch output has no request ID. Output errors are detected through serialization,
writing and the final flush; exit 0 requires a completed run and successful output.

## Desktop file boundary

`import_file` and `export_file` use the Tauri command boundary independently of the
worker's binary stream. `FileFormat` is generated from Rust alongside the other
frontend contracts. Source is valid UTF-8 up to 256 KiB. Raw binary files and
complete ELF exports contain 1 byte to 1 MiB. The file budget is independent of
the 64 KiB decode-request budget.

Dialogs select each path explicitly. Frontend callers supply a localized title and
format, and receive contents or cancellation, never host paths. Errors are stable
categories. Import is read-only; exports validate contents before replacing a
selected file through a flushed same-directory temporary file. Replacement is
atomic; directory crash durability is not guaranteed. No file operation implies
assembly, execution or mutation of the current worker session.

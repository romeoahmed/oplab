# Worker protocol

The isolated worker provides negotiation, assembly, decoding, ELF loading, execution,
coherent observations and shutdown. Rust declarations in `oplab-core::protocol`
are authoritative; `cargo xtask codegen [--check]` exports or verifies the committed
TypeScript declarations. [Engine](engine.md) owns machine semantics and artifact limits.

## Framing

Every frame begins with an eight-byte header:

| Offset | Field       | Encoding                                                 |
| ------ | ----------- | -------------------------------------------------------- |
| 0–1    | Magic       | ASCII `OP`                                               |
| 2      | Kind        | `0` JSON control, `1` binary, `2` JSON observation event |
| 3      | Flags       | Zero                                                     |
| 4–7    | Body length | Unsigned 32-bit little-endian                            |

JSON bodies are bounded to 1 MiB and binary bodies to 64 KiB; empty bodies are
invalid. Validate headers before allocating. Handle short reads/writes normally.
EOF before a header is clean shutdown; EOF inside a frame or declared payload is
failure. Malformed input closes the connection without scanning for another magic.
Stdout contains protocol bytes only; stderr uses static failure categories.

Version 1 transfers binary data only after the control/event declaring it. Complete
messages are contiguous: no other reply or event can interleave payload frames.
A header or JSON reply alone never completes a message with outstanding binary data.

## Negotiation and identity

Send `hello` with version `1` first. The reply advertises targets, assembler identity,
execution support and unavailable source mapping. An incompatible handshake receives
an error and closes without waiting for stdin EOF.

Request IDs are positive, strictly increasing canonical decimal strings scoped to
a connection. Reuse closes the connection without a duplicate-ID reply. Tagged
commands reject unknown fields/variants. Every accepted request has one terminal
reply unless the connection fails; a disconnect cannot establish success or justify
automatically retrying a mutation. Replies may arrive out of order, so correlate IDs.

| Value                                        | Wire representation                                  |
| -------------------------------------------- | ---------------------------------------------------- |
| Address                                      | `0x` plus sixteen lowercase hexadecimal digits       |
| 64-bit counter                               | Decimal string, no leading zeros except `0` itself   |
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

`load` declares target, completion, instruction budget, image length and `replace`,
followed by the same binary chunk format. No machine is allocated until transfer and
loader validation succeed. `replace: null` requires no session; replacement requires
the current exact key and a non-running state. Failure preserves the old machine.
Success uses the load request ID as session ID and starts generation zero.

`execute` carries a session key and `run`, `step`, `pause`, `cancel`, `reset`,
`breakpoint`, `observe` or `close`. Keys contain decimal `session` and `generation`.
Stale keys fail before mutation. Reset advances generation; close drops the owner,
even when running, and returns `session_closed`. IDs must also be qualified by the
worker connection, because they can recur after restart.

Other successful controls return `observed`: a complete snapshot containing key,
increasing sequence, status, instruction/dispatch counters, canonical registers,
fault metadata and optional memory metadata. Sequence begins at one, continues
across reset and never wraps. Run/step acceptance can report Running; poll or
subscribe to learn completion. Prioritized newer observations can precede older
correlated replies, so check key/generation/sequence before updating a live view.

Memory windows contain 1–65,536 bytes within one mapping and transfer after the
snapshot. Registers, memory and counters are captured coherently at one owner boundary.
64-bit register storage uses decimal strings; PC uses the canonical hex address.
Raw flags carry no definedness claim. Crashed snapshots have null registers and
cannot supply fresh memory. Publish only after the entire transfer validates.

The execution owner has one bounded command slot and one reply slot per accepted
command. It polls between slices, advances after servicing commands, and blocks
when stopped. Shutdown/EOF disconnect and join the owner. A native hang requires
the desktop supervisor, not a detached replacement thread.

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

A delta either retains the previous register bank or replaces the complete bank,
including explicit null after native loss. Zero memory length retains the baseline;
nonzero length replaces the entire baseline window. Window changes require `full`.
These are bank/window deltas, not individual-register or byte-range patches.

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
for an already queued document remains possible. Controls/decoding do not await assembly.

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
3. Bulk artifacts, decoded windows and memory-bearing Ready/Running replies.
4. Ready/Running subscription samples.

An in-flight message finishes first. Priority does not imply a hard latency guarantee.
Other bulk traffic cannot consume a build reservation. Without an unreserved slot,
a decode/memory-read result becomes a correlated `resource_limit` error; a completed
capture can therefore leave a sequence gap. Builds wait for capacity while controls
remain serviceable. Writer wakeups never block behind a full inbound queue.

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

The main window has four Tauri commands:

| Command          | Responsibility                                                                                                               |
| ---------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| `worker_connect` | Attach a Channel and obtain capabilities, connection/view lease and retained metadata; explicit restart creates a new worker |
| `worker_request` | Send a binary framed `DesktopCall` plus optional ELF; supervisor assigns worker IDs and returns the complete framed reply    |
| `worker_ack`     | Grant credit for a delivered subscription/sequence                                                                           |
| `worker_detach`  | Invalidate the view without replaying or cancelling admitted mutations                                                       |

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

The clap CLI shares engine operations. `assemble` reads source from stdin and writes
only linked ELF to stdout; operation failures leave stdout empty and emit a structured
JSON diagnostic to stderr. `capabilities` and `decode` write newline-terminated JSON.
Help/version and argument validation run before stdin reads. Usage errors exit 2;
operation failures also exit nonzero.

```sh
cargo run --locked -p oplab-engine --bin oplab-cli -- capabilities
cargo run --locked -p oplab-engine --bin oplab-cli -- assemble x86_64 0x1000 < experiment.asm > experiment.elf
cargo run --locked -p oplab-engine --bin oplab-cli -- decode aarch64 0x1000 < code.bin
```

CLI addresses accept ordinary hexadecimal input, including `0x1000` and `0XFF`.
These commands do not execute guests. CLI experiment execution and saved projects
remain planned; current execution is available through the engine and framed worker.

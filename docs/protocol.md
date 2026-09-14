# Worker protocol

The protocol provides negotiation, LLVM assembly, bounded decoding, ELF loading,
machine controls, coherent observations, subscriptions, and orderly shutdown. Execution runs
on a dedicated native owner thread, while a separate thread processes bounded
assembly work. The dispatcher samples subscriptions at native ownership boundaries.
The desktop supervisor launches and owns this worker; the WebView accesses it through four scoped Tauri commands.

## Framing

Each frame begins with eight bytes:

| Offset | Field       | Encoding                                                    |
| ------ | ----------- | ----------------------------------------------------------- |
| 0–1    | Magic       | ASCII `OP`                                                  |
| 2      | Kind        | `0`: JSON control; `1`: binary; `2`: JSON observation event |
| 3      | Flags       | Must be zero                                                |
| 4–7    | Body length | Unsigned 32-bit little-endian                               |

Control and observation JSON bodies are limited to 1 MiB; binary bodies to 64 KiB. Empty bodies are
invalid. Validate the entire header before allocating its body. Short reads and
short writes are normal pipe behavior. EOF before a header is clean shutdown;
EOF within a header or body is a failed connection. Never scan for another magic
sequence after malformed input.

Version 1 uses ordered binary payloads for assembly artifacts, load requests, and
memory observations. Binary frames are accepted only where the preceding control
reply, load request, or observation event declares a payload. Stdout carries protocol bytes only; stderr contains static failure categories.

## Requests and replies

Send `hello` with protocol version `1` before any other operation. Its reply reports
the exact assembler settings, available targets, and execution support. Original
source mapping remains unavailable. Incompatible initial negotiation receives a protocol
error and closes the connection.

Request IDs are positive, strictly increasing decimal strings scoped to one worker
connection. Reusing an identifier closes the connection without issuing another
reply with the same ID. Commands and replies are explicitly tagged unions; unknown
fields and unsupported variants are rejected. Each accepted request receives one
terminal reply. A disconnect is not evidence of successful completion or permission
to retry a state-changing operation.

Replies may arrive out of request order. Correlate by ID rather than by position;
for example, a pause or decode reply can precede an earlier assembly result.
Each reply and its binary payloads still form one contiguous message.

Priority can deliver a reset or terminal observation before an older memory reply.
Check session identity, generation, and observation sequence before applying values
to a live view, even after correlating the reply to its original request.

Addresses use `0x` followed by sixteen lowercase hexadecimal digits. Counters use
canonical decimal strings without leading zeros, except `0` itself. Both are bounded
to unsigned 64-bit values. JSON numbers are rejected for these scalars. Optional diagnostic addresses and original UTF-8 byte offsets (`source_offset`)
are explicit `null` when unavailable, not omitted fields. A source offset is a
position, not a statement range or source map.

The Rust declarations in `oplab-core::protocol` are authoritative. Run
`cargo xtask codegen` to regenerate the tracked TypeScript declarations and
`cargo xtask codegen --check` to verify content and file-set drift. Generation uses explicit
ts-rs configuration, independent of ambient export environment variables. Both
commands format the temporary declarations with the repository Oxfmt configuration
before comparing or writing the canonical output.

## Artifact transfer

An `assembled` control reply contains the build identity, `object_bytes`,
`image_bytes`, and a bounded ELF-derived image view. The two lengths must each be between one
byte and 1 MiB. The original relocatable ELF follows first, then the linked executable
ELF. Each file uses binary frames of exactly 64 KiB until its final remaining chunk.
No padding, separate terminator, or file content is placed in JSON.

Replies are serialized as complete messages: payload frames cannot interleave with
another response. The control reply is not successful completion until all declared
bytes arrive. Validate lengths before reserving buffers; reject wrong kinds, wrong
chunk lengths, premature EOF, and extra unsolicited frames. Connection failure
discards the incomplete artifact. ELF retains the authoritative section, symbol,
relocation, debug, and program-header structure.

## Sessions and observations

`load` declares `target`, `completion`, `instruction_budget`, `image_bytes`, and
`replace`. The complete ELF image follows in the same chunk format as artifacts,
with a one-MiB file limit. No machine is allocated until the transfer is complete.
The [loader](engine.md#loading) validates ELF; transport never invents maps or permissions.

`replace: null` requires no active session. Replacing an existing session requires
its exact key and a non-running state. Failed replacement preserves the old machine.
A successful load uses its request ID as the new session ID and starts generation
zero. The supervisor must also distinguish worker connections; these IDs are not
globally unique and must not survive a worker restart without a connection identity.

`execute` carries a session key and a tagged action: `run`, `step`, `pause`, `cancel`,
`reset`, `breakpoint`, `observe`, or `close`. The key contains decimal-string
`session` and `generation` fields. Stale keys receive `stale_session` before any
machine mutation. Reset restores the initial image and increments generation while
retaining address breakpoints. Close drops even a running session on its owner;
its reply is `session_closed` with the invalidated key.

Other successful actions return `observed`, a full snapshot with key, monotonically
increasing sequence, status, instruction/dispatch counts, canonical integer
registers, fault metadata, and optional memory metadata. Sequence starts at one and
continues across reset; it never wraps. Controls return the state when accepted,
so run and step may return Running. Use a subscription or poll `observe` until the
required pause or termination; acceptance is not completion. The [execution contract](engine.md#execution)
defines instruction, breakpoint, fault, and completion behavior.

`observe` optionally includes a memory window with exact address and a byte length
from one through 65,536, confined to one mapped region. Its binary bytes follow
the reply. Memory, registers, PC, counters, and fault metadata are captured at one
owner boundary before execution continues. Registers use exact decimal strings
for 64-bit raw values and canonical hexadecimal strings for PC. Raw flags carry
no definedness claim. Crashed observations have null registers; a memory read from
that lost machine fails explicitly. Consumers must validate the complete transfer
before publishing an observation.

The execution owner has one bounded command slot and each accepted command has one
bounded reply slot. It polls commands between slices and advances after servicing
a command, so observation traffic cannot indefinitely prevent execution progress.
When stopped, it blocks on the command receiver. Shutdown and EOF disconnect the
queue and join the native owner before process exit. No detached replacement thread
or automatic mutation retry exists. A native hang still requires the separate
desktop supervisor's deadline and forced termination.

## Observation subscriptions

`subscribe` carries an exact session key and optional memory window. Its request ID
becomes the subscription ID. One subscription is active per connection; a valid
replacement first captures its requested window, then replaces the previous
subscription. Invalid sessions or windows preserve the active subscription. A
`subscribed` reply settles the request before the new subscription's first event.
A subscription is passive: it never starts or pauses guest execution.

The dispatcher captures immediately on subscription and after successful controls.
While its last sample was Running, it samples again after approximately 33 ms using
[standard `recv_timeout`](https://doc.rust-lang.org/std/sync/mpsc/struct.Receiver.html#method.recv_timeout).
Incoming commands still trigger the deadline check, so a busy input queue does not
suppress sampling. Stopped sessions have no periodic poll; later controls refresh
them. This is observation pacing, not a native execution deadline or a guaranteed
presentation rate. Each sample receives the owner's next observation sequence.

Kind `2` carries `StreamEvent { subscription, update }`. Events have no request ID
and never settle a request. The update is one of:

- `full`: a complete `Observation`, followed by its optional binary memory window.
- `delta`: exact session key, `base` and new `sequence`, current status, counters,
  fault, register update, and `memory_bytes`. Register `unchanged` retains the
  previous complete bank; `replace` supplies a complete bank or explicit null after
  native state loss. Zero `memory_bytes` retains memory; otherwise the complete
  baseline window follows with exactly that length. Window changes require a full
  event. These are bank/window deltas, not per-register or byte-range patches.
- `ended`: a structured capture failure. The subscription stops; this does not
  itself stop or mutate the guest. A requested memory capture can fail after a
  native crash; a register-only read can still report Crashed with null registers.

Only complete samples coalesce, in one dedicated output slot. The writer retains
one delivered sample and computes the next delta against it after selecting the
queued sample. It advances the baseline only after the complete event and binary
payload are written successfully. Thus samples 2 and 3 may be replaced by sample 4,
but a client that received sample 1 gets `base: 1`, never an unavailable `base: 3`.
The first event of a new subscription is always full, even if its initial queued
sample was replaced before delivery.

Consumers retain a separate stream baseline; correlated `observed` replies never
advance it. Check connection, subscription, session, generation, and increasing
sequence before applying an event. A delta's `base` must exactly match that stream
baseline. Sequence gaps are legal; missing bases require discarding the baseline
and subscribing again for a full event. Consume every complete event into the
baseline before coalescing frontend renders. Reject incomplete or mismatched binary
payloads without publishing partial state. The pure frontend reducer implements
these checks. The desktop supervisor reconstructs complete samples before coalescing them for the WebView.

`unsubscribe` requires the exact subscription ID and returns `unsubscribed`.
Replacement, successful reset/load/close, shutdown, and EOF clear the pending slot.
An event already being written finishes before the invalidating control reply;
none from that subscription follows the acknowledgement. Failed controls retain
the subscription. Reset/load require an explicit new subscription to the returned
key. Capture failures end with `ended`; explicit invalidation is conveyed by the
corresponding control reply. The synchronous `Worker::handle` API cannot host an
unsolicited stream and rejects subscription commands with `invalid_input`.

## Output priority and backpressure

The writer selects complete messages from bounded control and bulk lanes.
Control replies, errors, and paused/terminal observations precede queued bulk data.
Assembled artifacts, decoded windows, and memory-bearing Ready/Running observations
use the bulk lane. The separate observation slot never consumes a reply or build
reservation. Paused/terminal samples and capture failures follow control replies
and precede bulk; Ready/Running samples follow bulk. An in-flight message finishes
before another is selected; control frames cannot be inserted into its binary payload.
Priority is therefore an ordering rule, not a hard latency guarantee.

The dispatcher reserves one bulk slot before starting a native build. Other bulk
replies cannot consume that reservation. If all unreserved bulk slots are occupied,
a decode or memory-read result becomes a correlated `resource_limit` error. This
does not mutate guest state; a captured full observation can leave a sequence gap.
New native builds wait for capacity while queued controls remain serviceable.
When the writer removes a message, it wakes the dispatcher to reconsider pending
work. The wakeup never blocks the writer behind a full inbound queue.

Control saturation applies backpressure instead of discarding request outcomes.
Clients must continue draining output while submitting requests. Closing the writer
wakes blocked producers with failure; it cannot turn a lost reply into success.
The shutdown acknowledgement has a separate final slot and is released only after
both lanes drain, so `closed` remains last even though ordinary controls have priority.
Bounds exclude the one complete message currently being written and the writer's
last delivered observation baseline (at most one 64-KiB memory window).

## Assembly scheduling and cancellation

The dispatcher admits one active native build and at most eight pending builds.
There is one pending entry per document. A newer
valid request for that document replaces its pending predecessor, which receives
`superseded`. An active build is not implicitly cancelled by newer source. Complete build identity remains necessary
when deciding whether to display a result.

Invalid input is rejected before it can replace valid pending work. A full queue
rejects a new document with `resource_limit`; replacement of an existing pending
document remains possible. Execution controls and decoding do not wait for native
assembly completion. Blocking pipe reads and writes have separate process-owned
threads, with 16 inbound event slots and the reserved output lanes described above.
These bounds provide backpressure; clients must drain replies while sending work.

`cancel_assembly` identifies an outstanding request with an exact decimal-string
`request` field. When accepted, the original request receives `cancelled` and the
cancellation request receives `assembly_cancelled` containing that original ID.
A repeated, unknown, or already published request receives `invalid_input`.
Publication and cancellation are ordered by the dispatcher, so a race has exactly
one outcome: the original build result, or cancellation. Later native completion
cannot publish a second reply.

Pending cancellation removes the job. Active cancellation sets a latch checked
before compilation, between compilation and linking, and after linking. It cannot
interrupt an LLVM/LLD call already in progress. The active slot remains occupied
until that call returns, even though the cancelled request is already settled.
The desktop supervisor enforces separate time and resident-memory thresholds.

Shutdown is the final input request. It stops the execution owner, drains accepted
assembly work, and queues `closed` after all accepted replies. The build and output
threads finish before process exit. EOF performs the same drain without a synthetic request or reply.
On malformed input or owner failure, the process exits instead of continuing with
uncertain state. An incompatible first handshake receives its protocol error without
waiting for the peer to close stdin. This standard-pipe runtime is a process entry;
embedded callers use the synchronous engine APIs or `Worker::handle`.

## Build identity

Every build captures document identity, revision, target, base, and assembler
name/version. Consumers compare the complete identity before accepting a result.
The [engine contract](engine.md) owns language semantics, artifact/resource limits,
and native failure boundaries. Transport carries complete standard ELF files and
does not add source translation, optimizer settings, or compatibility schemas.

## Desktop integration

`worker_connect` attaches a Channel and returns capabilities, a connection identity,
a view lease, and any retained session/artifact metadata. Reattachment creates a
new lease while preserving the worker; explicit restart creates a new process.
`worker_request` accepts one binary framed `DesktopCall` and optional ELF payload,
then returns the complete framed worker response through raw IPC. The supervisor
allocates worker request IDs. `worker_ack` grants credit for a specific delivered
subscription/sequence; `worker_detach` invalidates the current view without replaying
or cancelling already admitted mutations. Only the main window can invoke these
application commands. Zero, detached, and obsolete view leases cannot mutate a worker.

The supervisor admits at most 16 requests. Separate threads own stdin, stdout,
stderr, and monitoring. Assembly requests have a 15-second deadline; other requests
have 3 seconds. Continuously observed Running state has a 60-second deadline.
Monitoring samples worker RSS every 25 ms and terminates above 1 GiB. These are
supervisory cutoffs with scheduling/polling latency, not OS-enforced CPU or memory
quotas. Missing process-memory samples are not evidence of memory compliance.
Stderr is drained independently without retaining its contents and fails the
connection after 64 KiB. Shutdown gives the worker two seconds before kill/reap.

Failure settles outstanding requests once. A request whose pipe write has begun
reports `outcome_unknown: true`; the supervisor never retries a mutation. A failed
worker requires explicit restart. Process isolation contains native exits but
cannot preserve a guest machine after the worker dies.

Worker deltas are reconstructed into full observations before WebView coalescing.
The Channel holds one unacknowledged observation and one latest pending sample;
control replies and a small failure notification remain independent of that credit.
A slow frontend therefore cannot grow an unbounded application observation queue.
The frontend filters stale identities, keeps a coherent stream baseline, and
serializes replacement subscriptions across memory-window and session changes.
See [Tauri Channels](https://v2.tauri.app/develop/calling-frontend/), which provide
transport rather than an application backpressure policy.

## CLI

The CLI uses synchronous dispatch over the same engine operations and artifact
projection. `assemble` reads source from stdin and
writes a standard linked ELF image to stdout, suitable for ordinary binary tools.
Assembly failures leave stdout empty and write a structured JSON diagnostic to
stderr. `capabilities` and `decode` emit a JSON response followed by a newline.
clap provides command/argument validation and `--help` / `--version` before reading
stdin. Usage errors exit with status 2; operation failures also exit nonzero.
Help is human-readable; successful assembly stdout remains exclusively ELF bytes.

```sh
cargo run -p oplab-engine --bin oplab-cli -- capabilities
cargo run -p oplab-engine --bin oplab-cli -- assemble x86_64 0x1000 < experiment.asm > experiment.elf
cargo run -p oplab-engine --bin oplab-cli -- decode aarch64 0x1000 < code.bin
```

Human CLI inputs accept ordinary hexadecimal addresses such as `0x1000` or `0XFF`.
Canonical fixed-width formatting is confined to JSON transport and does not dictate
what users type.

These CLI commands do not execute guest code. Native execution is available through
the framed worker and engine library; CLI experiment execution, function setup,
and saved projects remain subsequent work.

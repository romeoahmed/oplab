import type { Counter } from './generated/Counter';
import type { Diagnostic } from './generated/Diagnostic';
import type { Observation } from './generated/Observation';
import type { ObservationDelta } from './generated/ObservationDelta';
import type { SessionKey } from './generated/SessionKey';
import type { StreamEvent } from './generated/StreamEvent';
import { parseAddress, parseCounter } from './scalars';

type Snapshot = { observation: Observation; memory: Uint8Array | null };
type Cursor = { subscription: Counter; key: SessionKey; baseline: Snapshot | null };
type Update =
  | { type: 'ignored' }
  | { type: 'resync' }
  | { type: 'ended'; error: Diagnostic }
  | { type: 'updated'; snapshot: Snapshot };

function sameKey(left: SessionKey, right: SessionKey): boolean {
  return left.session === right.session && left.generation === right.generation;
}

function validate(snapshot: Snapshot): void {
  const { observation, memory } = snapshot;
  if (parseCounter(observation.sequence) === 0n)
    throw new RangeError('Invalid observation sequence');
  parseCounter(observation.instructions);
  parseCounter(observation.dispatches);
  if ((observation.status.type === 'crashed') !== (observation.registers === null))
    throw new RangeError('Incoherent register availability');
  const window = observation.memory;
  if (window === null) {
    if (memory !== null) throw new RangeError('Unexpected observation memory');
    return;
  }
  if (
    !Number.isInteger(window.length) ||
    window.length < 1 ||
    window.length > 65536 ||
    memory?.byteLength !== window.length ||
    parseAddress(window.address) + BigInt(window.length) > 1n << 64n
  )
    throw new RangeError('Invalid observation memory');
}

function applyDelta(base: Snapshot, delta: ObservationDelta, memory: Uint8Array | null): Snapshot {
  if (
    !Number.isInteger(delta.memory_bytes) ||
    (delta.memory_bytes === 0
      ? memory !== null
      : delta.memory_bytes !== base.observation.memory?.length || memory === null)
  )
    throw new RangeError('Invalid delta memory');
  return {
    observation: {
      key: delta.key,
      sequence: delta.sequence,
      status: delta.status,
      instructions: delta.instructions,
      dispatches: delta.dispatches,
      registers:
        delta.registers.type === 'unchanged' ? base.observation.registers : delta.registers.data,
      fault: delta.fault,
      memory: base.observation.memory,
    },
    memory: delta.memory_bytes === 0 ? base.memory : memory,
  };
}

/**
 * Reconstruct a complete subscription event without mutating the cursor.
 *
 * @remarks
 * Correlated replies never establish a stream baseline. On `updated`, retain the
 * returned snapshot as the next baseline; on `resync`, subscribe again for a full
 * event. Snapshots may share input objects and buffers: callers must not mutate them.
 *
 * @returns `ignored` for stale identities/sequences, `resync` for a missing delta
 * baseline, `ended` for capture failure, or `updated` with the coherent snapshot.
 * @throws RangeError - Scalars, memory, register availability or counters are incoherent.
 */
export function applyObservation(
  cursor: Cursor,
  event: StreamEvent,
  memory: Uint8Array | null,
): Update {
  if (event.subscription !== cursor.subscription) return { type: 'ignored' };
  const update = event.update;
  if (update.type === 'ended') {
    if (memory !== null) throw new RangeError('Unexpected ended payload');
    return { type: 'ended', error: update.data };
  }
  if (!sameKey(cursor.key, update.data.key)) return { type: 'ignored' };
  const base =
    cursor.baseline !== null && sameKey(cursor.baseline.observation.key, cursor.key)
      ? cursor.baseline
      : null;
  const sequence = parseCounter(update.data.sequence);
  if (base !== null && sequence <= parseCounter(base.observation.sequence))
    return { type: 'ignored' };
  let snapshot: Snapshot;
  if (update.type === 'full') snapshot = { observation: update.data, memory };
  else {
    if (
      base === null ||
      !sameKey(base.observation.key, update.data.key) ||
      update.data.base !== base.observation.sequence
    )
      return { type: 'resync' };
    snapshot = applyDelta(base, update.data, memory);
  }
  validate(snapshot);
  if (
    base !== null &&
    (parseCounter(snapshot.observation.instructions) <
      parseCounter(base.observation.instructions) ||
      parseCounter(snapshot.observation.dispatches) < parseCounter(base.observation.dispatches))
  )
    throw new RangeError('Observation counters moved backwards');
  return { type: 'updated', snapshot };
}

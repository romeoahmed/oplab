import { expect, test } from 'vitest';
import fc from 'fast-check';
import { applyObservation } from '$lib/protocol/observations';
import type { Observation } from '$lib/protocol/generated/Observation';
import type { ObservationDelta } from '$lib/protocol/generated/ObservationDelta';
import type { StreamEvent } from '$lib/protocol/generated/StreamEvent';

const key = { session: '2', generation: '0' };
function observation(sequence = '9007199254740993'): Observation {
  return {
    key,
    sequence,
    status: { type: 'running' },
    instructions: '1',
    dispatches: '1',
    registers: {
      type: 'aarch64',
      data: {
        x: [
          '0',
          '0',
          '0',
          '0',
          '0',
          '0',
          '0',
          '0',
          '0',
          '0',
          '0',
          '0',
          '0',
          '0',
          '0',
          '0',
          '0',
          '0',
          '0',
          '0',
          '0',
          '0',
          '0',
          '0',
          '0',
          '0',
          '0',
          '0',
          '0',
          '0',
          '0',
        ],
        sp: '0',
        pc: '0x0000000000001000',
        nzcv: 0,
      },
    },
    fault: null,
    memory: { address: '0x0000000000002000', length: 8 },
  };
}
function delta(base: Observation, sequence: string): ObservationDelta {
  return {
    key,
    base: base.sequence,
    sequence,
    status: base.status,
    instructions: base.instructions,
    dispatches: base.dispatches,
    registers: { type: 'unchanged' },
    fault: null,
    memory_bytes: 0,
  };
}
function event(data: ObservationDelta): StreamEvent {
  return { subscription: '3', update: { type: 'delta', data } };
}

test('coalesced sequence gaps preserve exact banks and memory without mutating the baseline', () => {
  fc.assert(
    fc.property(
      fc.bigInt({ min: 1n, max: 100000n }),
      fc.uint8Array({ minLength: 8, maxLength: 8 }),
      (gap, bytes) => {
        const original = observation();
        const baseline = { observation: original, memory: new Uint8Array(8) };
        const cursor = { subscription: '3', key, baseline };
        const next = (BigInt(original.sequence) + gap).toString();
        const unchanged = applyObservation(cursor, event(delta(original, next)), null);
        if (unchanged.type !== 'updated') throw new Error('Missing update');
        expect(unchanged.snapshot.observation.sequence).toBe(next);
        expect(unchanged.snapshot.observation.registers).toEqual(original.registers);
        expect(unchanged.snapshot.memory).toEqual(baseline.memory);
        const changed = applyObservation(
          cursor,
          event({ ...delta(original, next), memory_bytes: 8 }),
          bytes,
        );
        if (changed.type !== 'updated') throw new Error('Missing memory update');
        expect(changed.snapshot.memory).toEqual(bytes);
        expect(baseline.memory).toEqual(new Uint8Array(8));
        expect(original.sequence).toBe('9007199254740993');
      },
    ),
  );
});

test('stale subscriptions and generations are ignored; missing delta bases require a full resubscription', () => {
  const original = observation();
  const cursor = {
    subscription: '3',
    key,
    baseline: { observation: original, memory: new Uint8Array(8) },
  };
  const next = delta(original, '9007199254740995');
  for (const stale of [
    { ...event(next), subscription: '1' },
    event({ ...next, key: { ...key, generation: '1' } }),
    event({ ...next, sequence: original.sequence }),
  ])
    expect(applyObservation(cursor, stale, null).type).toBe('ignored');
  expect(applyObservation(cursor, event({ ...next, base: '9007199254740994' }), null).type).toBe(
    'resync',
  );
  expect(applyObservation({ ...cursor, baseline: null }, event(next), null).type).toBe('resync');
  const full: StreamEvent = {
    subscription: '3',
    update: { type: 'full', data: observation(next.sequence) },
  };
  expect(applyObservation({ ...cursor, baseline: null }, full, new Uint8Array(8)).type).toBe(
    'updated',
  );
});

test('a coherent crash clears registers; binary mismatch never produces a partial state', () => {
  const original = observation();
  const cursor = {
    subscription: '3',
    key,
    baseline: { observation: original, memory: new Uint8Array(8) },
  };
  const next = delta(original, '9007199254740994');
  const crash = applyObservation(
    cursor,
    event({ ...next, status: { type: 'crashed' }, registers: { type: 'replace', data: null } }),
    null,
  );
  if (crash.type !== 'updated') throw new Error('Missing crash');
  expect(crash.snapshot.observation.registers).toBeNull();
  for (const [size, bytes] of [
    [0, new Uint8Array(8)],
    [8, null],
    [8, new Uint8Array(7)],
    [4, new Uint8Array(4)],
  ] as const)
    expect(() => applyObservation(cursor, event({ ...next, memory_bytes: size }), bytes)).toThrow(
      RangeError,
    );
  expect(() =>
    applyObservation(cursor, event({ ...next, status: { type: 'crashed' } }), null),
  ).toThrow(RangeError);
  expect(() => applyObservation(cursor, event({ ...next, instructions: '0' }), null)).toThrow(
    RangeError,
  );
});

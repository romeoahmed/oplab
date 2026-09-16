import type { Observation } from '$lib/protocol/generated/Observation';
import type { ObservationDelta } from '$lib/protocol/generated/ObservationDelta';
import type { StreamEvent } from '$lib/protocol/generated/StreamEvent';
import type { Target } from '$lib/protocol/generated/Target';
import { applyObservation } from '$lib/protocol/observations';
import fc from 'fast-check';
import { expect, test } from 'vitest';

import { observation } from '../fixtures/protocol';

function baseline(target: Target = 'x86_64'): { observation: Observation; memory: Uint8Array } {
  const value = observation(undefined, target);
  value.cpu = target === 'x86_64' ? 'nehalem' : 'cortex_a53';
  value.memory = { address: '0x0000000000002000', length: 8 };
  return { observation: value, memory: new Uint8Array(8) };
}
function delta(base = baseline().observation, sequence = '9007199254740994'): ObservationDelta {
  return {
    key: base.key,
    status: base.status,
    instructions: base.instructions,
    dispatches: base.dispatches,
    fault: base.fault,
    base: base.sequence,
    sequence,
    registers: { type: 'unchanged' },
    memory_bytes: 0,
  };
}
function event(data: ObservationDelta): StreamEvent {
  return { subscription: '3', update: { type: 'delta', data } };
}

// The model is the latest complete sample, independently chosen by the generator.
// Production decides how a full or delta packet reconstructs that sample.
test.each(['x86_64', 'aarch64'] as const)(
  '%s full/delta histories reconstruct samples without mutating retained state',
  (target) => {
    fc.assert(
      fc.property(
        fc.array(
          fc.record({
            gap: fc.integer({ min: 1, max: 1000 }),
            full: fc.boolean(),
            breakpoints: fc.uniqueArray(fc.bigInt({ min: 0n, max: (1n << 64n) - 1n }), {
              maxLength: 16,
            }),
            vector: fc.option(fc.bigInt({ min: 0n, max: (1n << 128n) - 1n })),
            value: fc.option(fc.bigInt({ min: 0n, max: (1n << 64n) - 1n })),
            memory: fc.option(fc.uint8Array({ minLength: 8, maxLength: 8 })),
          }),
          { minLength: 1, maxLength: 30 },
        ),
        (samples) => {
          let previous = baseline(target);
          for (const [index, sample] of samples.entries()) {
            const retained = structuredClone(previous);
            const expected = structuredClone(previous);
            expected.observation.sequence = String(
              BigInt(previous.observation.sequence) + BigInt(sample.gap),
            );
            if (sample.full)
              expected.observation.breakpoints = sample.breakpoints
                .toSorted((a, b) => (a < b ? -1 : a > b ? 1 : 0))
                .map((value) => `0x${value.toString(16).padStart(16, '0')}`);
            expected.observation.instructions = String(index + 1);
            expected.observation.dispatches = String(index + 1);
            const bank = expected.observation.registers;
            if (bank === null) throw new Error('Missing fixture bank');
            if (sample.value !== null) {
              const registers = bank.type === 'x86_64' ? bank.data.gpr : bank.data.x;
              registers[0] = String(sample.value);
            }
            if (sample.vector !== null) {
              const vectors = bank.type === 'x86_64' ? bank.data.xmm : bank.data.v;
              vectors[vectors.length - 1] = `0x${sample.vector.toString(16).padStart(32, '0')}`;
            }
            expected.memory = sample.memory ?? previous.memory;
            const packet: StreamEvent = sample.full
              ? { subscription: '3', update: { type: 'full', data: expected.observation } }
              : event({
                  ...delta(expected.observation, expected.observation.sequence),
                  base: previous.observation.sequence,
                  registers:
                    sample.value === null && sample.vector === null
                      ? { type: 'unchanged' }
                      : { type: 'replace', data: bank },
                  memory_bytes: sample.memory === null ? 0 : 8,
                });
            const model = structuredClone(expected);
            const result = applyObservation(
              { subscription: '3', key: previous.observation.key, baseline: previous },
              packet,
              sample.full ? expected.memory : sample.memory,
            );
            expect(result).toEqual({ type: 'updated', snapshot: model });
            expect(previous).toEqual(retained);
            if (result.type !== 'updated' || result.snapshot.memory === null)
              throw new Error('Missing complete sample');
            previous = { observation: result.snapshot.observation, memory: result.snapshot.memory };
          }
        },
      ),
    );
  },
);

test('stale identities are ignored; absent or lost delta bases require a full sample', () => {
  const initial = baseline();
  const cursor = { subscription: '3', key: initial.observation.key, baseline: initial };
  const next = delta();
  for (const stale of [
    { ...event(next), subscription: '1' },
    event({ ...next, key: { ...next.key, session: '1' } }),
    event({ ...next, key: { ...next.key, generation: '1' } }),
    event({ ...next, sequence: initial.observation.sequence }),
  ])
    expect(applyObservation(cursor, stale, null)).toEqual({ type: 'ignored' });
  for (const candidate of [cursor, { ...cursor, baseline: null }]) {
    expect(applyObservation(candidate, event({ ...next, base: '1' }), null)).toEqual({
      type: 'resync',
    });
  }
  const result = applyObservation(cursor, event(next), null);
  expect(result).toEqual({
    type: 'updated',
    snapshot: { ...initial, observation: { ...initial.observation, sequence: next.sequence } },
  });
  const fresh = observation(next.sequence);
  expect(
    applyObservation(
      { ...cursor, baseline: null },
      { subscription: '3', update: { type: 'full', data: fresh } },
      null,
    ),
  ).toEqual({ type: 'updated', snapshot: { observation: fresh, memory: null } });
});

test('crashes clear registers; incomplete memory and backwards counters never publish partial state', () => {
  const initial = baseline();
  initial.observation.instructions = '1';
  initial.observation.dispatches = '2';
  const cursor = { subscription: '3', key: initial.observation.key, baseline: initial };
  const next = delta(initial.observation);
  const retained = structuredClone(initial);
  const crash = applyObservation(
    cursor,
    event({ ...next, status: { type: 'crashed' }, registers: { type: 'replace', data: null } }),
    null,
  );
  expect(crash).toMatchObject({
    type: 'updated',
    snapshot: { observation: { status: { type: 'crashed' }, registers: null } },
  });
  for (const [size, bytes] of [
    [0, new Uint8Array(8)],
    [8, null],
    [8, new Uint8Array(7)],
    [4, new Uint8Array(4)],
  ] as const)
    expect(() => applyObservation(cursor, event({ ...next, memory_bytes: size }), bytes)).toThrow(
      RangeError,
    );
  for (const invalid of [
    { ...next, status: { type: 'crashed' as const } },
    { ...next, instructions: '0' },
    { ...next, dispatches: '1' },
  ])
    expect(() => applyObservation(cursor, event(invalid), null)).toThrow(RangeError);
  expect(initial).toEqual(retained);
  const ended: StreamEvent = {
    subscription: '3',
    update: { type: 'ended', data: { code: 'stale_session', address: null, source_offset: null } },
  };
  expect(applyObservation(cursor, ended, null)).toEqual({
    type: 'ended',
    error: ended.update.data,
  });
  expect(() => applyObservation(cursor, ended, new Uint8Array(1))).toThrow(RangeError);
});

test('full memory windows validate byte counts and the exclusive 64-bit address boundary', () => {
  fc.assert(
    fc.property(
      fc.integer({ min: 0, max: 65536 }),
      fc.integer({ min: 0, max: 65537 }),
      (remaining, length) => {
        const start = (1n << 64n) - 1n - BigInt(remaining);
        const value = observation();
        value.memory = { address: `0x${start.toString(16).padStart(16, '0')}`, length };
        const bytes = new Uint8Array(length);
        const apply = () =>
          applyObservation(
            { subscription: '3', key: value.key, baseline: null },
            { subscription: '3', update: { type: 'full', data: value } },
            bytes,
          );
        if (length > 0 && length <= 65536 && BigInt(length) + start <= 1n << 64n) {
          expect(apply()).toEqual({
            type: 'updated',
            snapshot: { observation: value, memory: bytes },
          });
        } else expect(apply).toThrow(RangeError);
      },
    ),
    {
      examples: [
        [0, 0],
        [0, 1],
        [0, 2],
        [65535, 65536],
        [65534, 65536],
        [65536, 65537],
      ],
    },
  );
});

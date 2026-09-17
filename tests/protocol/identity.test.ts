import type { BuildIdentity } from '$lib/protocol/generated/BuildIdentity';
import { sameBuildIdentity } from '$lib/protocol/identity';
import { expect, test } from 'vitest';

test('complete build identity rejects cross-document and stale settings', () => {
  const identity: BuildIdentity = {
    document: 'one',
    revision: '1',
    target: 'x86_64',
    base: '0x0000000000001000',
    assembler: { name: 'llvm-mc', version: '23.1.1' },
  };
  expect(sameBuildIdentity(identity, structuredClone(identity))).toBe(true);
  const different: BuildIdentity[] = [
    { ...identity, document: 'two' },
    { ...identity, revision: '2' },
    { ...identity, target: 'aarch64' },
    { ...identity, base: '0x0000000000002000' },
    { ...identity, assembler: { ...identity.assembler, version: '23.1.2' } },
    { ...identity, assembler: { ...identity.assembler, name: 'other' } },
  ];
  for (const candidate of different) expect(sameBuildIdentity(identity, candidate)).toBe(false);
});

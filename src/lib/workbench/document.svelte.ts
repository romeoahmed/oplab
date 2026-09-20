import type { Artifact } from '$lib/protocol/generated/Artifact';
import type { BuildIdentity } from '$lib/protocol/generated/BuildIdentity';
import type { Diagnostic } from '$lib/protocol/generated/Diagnostic';
import type { Target } from '$lib/protocol/generated/Target';

import type { InitialInput } from './load/initial-state';
import type { Scratch } from './scratch';

/** Editable inputs and immutable build results for one source document. */
export class SourceDocument {
  inputs: Scratch;
  name = $state('');
  candidate = $state.raw<{ artifact: Artifact; object: Uint8Array; image: Uint8Array } | null>(
    null,
  );
  failure = $state.raw<{ identity: BuildIdentity; diagnostic: Diagnostic } | null>(null);
  setups = $state<Record<Target, InitialInput>>({
    x86_64: { registers: [], mappings: [] },
    aarch64: { registers: [], mappings: [] },
  });

  constructor(inputs: Scratch, name: string) {
    this.inputs = $state(inputs);
    this.name = name;
  }
}
export function newDocument(name: string, target: Target = 'x86_64') {
  return new SourceDocument(
    {
      documentId: crypto.randomUUID(),
      revision: '0',
      source: '',
      target,
      base: '0x1000',
      completion: 'done',
      budget: '1000000',
    },
    name,
  );
}

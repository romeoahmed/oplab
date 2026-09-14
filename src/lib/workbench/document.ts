import { parseCounter } from '$lib/protocol/scalars';
import type { Target } from '$lib/protocol/generated/Target';

/** Recoverable editor inputs; incomplete human fields remain editable after restart. */
export type Scratch = {
  documentId: string;
  revision: string;
  source: string;
  target: Target;
  base: string;
  completion: string;
  budget: string;
};

/** Validate the complete stored value before replacing any current document state. */
export function readScratch(value: unknown): Scratch {
  if (
    typeof value !== 'object' ||
    value === null ||
    !('source' in value) ||
    typeof value.source !== 'string' ||
    new TextEncoder().encode(value.source).length > 262144 ||
    !('target' in value) ||
    (value.target !== 'x86_64' && value.target !== 'aarch64') ||
    !('revision' in value) ||
    typeof value.revision !== 'string' ||
    !('documentId' in value) ||
    typeof value.documentId !== 'string' ||
    !/^[A-Za-z0-9_-]{1,64}$/.test(value.documentId) ||
    !('base' in value) ||
    typeof value.base !== 'string' ||
    value.base.length > 80 ||
    !('completion' in value) ||
    typeof value.completion !== 'string' ||
    value.completion.length > 1024 ||
    !('budget' in value) ||
    typeof value.budget !== 'string' ||
    value.budget.length > 80
  )
    throw new RangeError('Invalid scratch document');
  parseCounter(value.revision);
  return {
    documentId: value.documentId,
    revision: value.revision,
    source: value.source,
    target: value.target,
    base: value.base,
    completion: value.completion,
    budget: value.budget,
  };
}

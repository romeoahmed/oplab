import type { Scratch } from '$lib/workbench/scratch';

/** Small document for UI workflows, independent of the bundled programs. */
export const scratch = {
  documentId: 'fixture',
  revision: '0',
  source: '.text\nnop\ndone: nop\n',
  target: 'x86_64',
  base: '0x1000',
  completion: 'done',
  budget: '1000000',
} satisfies Scratch;

/** Seed one document through the current workspace recovery format. */
export function storeDraft(inputs: Scratch = scratch): void {
  localStorage.setItem(
    'oplab.workspace.v1',
    JSON.stringify({
      active: inputs.documentId,
      documents: [{ ...inputs, name: 'untitled-1.s' }],
    }),
  );
}

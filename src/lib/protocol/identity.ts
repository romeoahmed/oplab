import type { BuildIdentity } from './generated/BuildIdentity';

/** Compare document, revision and all assembly settings before accepting a build. */
export function sameBuildIdentity(left: BuildIdentity, right: BuildIdentity): boolean {
  return (
    left.document === right.document &&
    left.revision === right.revision &&
    left.target === right.target &&
    left.base === right.base &&
    left.assembler.name === right.assembler.name &&
    left.assembler.version === right.assembler.version
  );
}

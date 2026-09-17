import type { InitialState } from '$lib/protocol/generated/InitialState';
import { formatCounter, normalizeAddress, parseUnsigned } from '$lib/protocol/scalars';

export type InitialInput = {
  registers: { name: string; value: string }[];
  mappings: { address: string; length: string; flags: number }[];
};

/**
 * Capture editable initial conditions as exact wire values before starting a load.
 *
 * @remarks
 * The loader validates register names, page alignment, overlap and aggregate memory limits.
 *
 * @throws RangeError - An input is malformed or exceeds its field or collection limit.
 */
export function initialState(input: InitialInput): InitialState {
  if (input.registers.length > 32 || input.mappings.length > 63)
    throw new RangeError('Too many initial conditions');
  return {
    registers: input.registers.map(({ name, value }) => ({
      name,
      value: formatCounter(parseUnsigned(value)),
    })),
    mappings: input.mappings.map(({ address, length, flags }) => {
      const size = parseUnsigned(length);
      if (size < 1n || size > 67108864n || !Number.isInteger(flags) || flags < 0 || flags > 7)
        throw new RangeError('Invalid mapping');
      return { address: normalizeAddress(address), length: Number(size), flags };
    }),
  };
}

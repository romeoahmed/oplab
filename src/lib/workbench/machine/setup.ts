import type { CpuModel } from '$lib/protocol/generated/CpuModel';
import type { InitialState } from '$lib/protocol/generated/InitialState';
import type { Target } from '$lib/protocol/generated/Target';
import { formatCounter, normalizeAddress } from '$lib/protocol/scalars';

export type SetupInput = {
  cpu: CpuModel | null;
  registers: { name: string; value: string }[];
  mappings: { address: string; length: string; flags: number }[];
};

const x86 = [
  'rax',
  'rcx',
  'rdx',
  'rbx',
  'rsp',
  'rbp',
  'rsi',
  'rdi',
  'r8',
  'r9',
  'r10',
  'r11',
  'r12',
  'r13',
  'r14',
  'r15',
];
const aarch64 = [...Array.from({ length: 31 }, (_, index) => `x${String(index)}`), 'sp'];

/** Canonical GPR names in display order; AArch64 appends SP after X0–X30. */
export function registerNames(target: Target): readonly string[] {
  return target === 'x86_64' ? x86 : aarch64;
}

/**
 * Parse unsigned decimal or 0x-prefixed input, allowing surrounding whitespace.
 *
 * @remarks
 * The result is exact but not width-limited; callers enforce their own numeric bounds.
 *
 * @throws RangeError - The trimmed input is empty or contains invalid digits.
 */
export function unsigned(value: string): bigint {
  const text = value.trim();
  if (!/^(?:[0-9]+|0x[\da-f]+)$/i.test(text)) throw new RangeError('Invalid unsigned integer');
  return BigInt(text);
}

/**
 * Capture editable initial conditions as exact wire values before starting a load.
 *
 * @remarks
 * The loader validates register names, page alignment, overlap and aggregate memory limits.
 *
 * @throws RangeError - An input is malformed or exceeds its field or collection limit.
 */
export function initialState(input: SetupInput): InitialState {
  if (input.registers.length > 32 || input.mappings.length > 63)
    throw new RangeError('Too many initial conditions');
  return {
    cpu: input.cpu,
    registers: input.registers.map(({ name, value }) => ({
      name,
      value: formatCounter(unsigned(value)),
    })),
    mappings: input.mappings.map(({ address, length, flags }) => {
      const size = unsigned(length);
      if (size < 1n || size > 67108864n || !Number.isInteger(flags) || flags < 0 || flags > 7)
        throw new RangeError('Invalid mapping');
      return { address: normalizeAddress(address), length: Number(size), flags };
    }),
  };
}

/** Product names for the supported emulator profiles; model names are not translated. */
export const cpuNames: Record<CpuModel, string> = {
  haswell: 'Haswell',
  nehalem: 'Nehalem',
  cortex_a53: 'Cortex-A53',
  cortex_a72: 'Cortex-A72',
};

/** Default first, followed by the alternate verified profile for this guest. */
export function cpuModels(target: Target): readonly CpuModel[] {
  return target === 'x86_64' ? ['haswell', 'nehalem'] : ['cortex_a72', 'cortex_a53'];
}

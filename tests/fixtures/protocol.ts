import type { WorkerPort } from '$lib/desktop/worker';
import type { BuildIdentity } from '$lib/protocol/generated/BuildIdentity';
import type { ConnectionInfo } from '$lib/protocol/generated/ConnectionInfo';
import type { InstructionAnalysis } from '$lib/protocol/generated/InstructionAnalysis';
import type { Observation } from '$lib/protocol/generated/Observation';
import type { Registers } from '$lib/protocol/generated/Registers';
import type { Target } from '$lib/protocol/generated/Target';

// Shared protocol data; native behavior is exercised by the Rust process tests.
export function observation(sequence = '9007199254740993', target: Target = 'x86_64'): Observation {
  return {
    cpu: target === 'x86_64' ? 'haswell' : 'cortex_a72',
    key: { session: '2', generation: '0' },
    sequence,
    status: { type: 'ready' },
    instructions: '0',
    dispatches: '0',
    registers:
      target === 'aarch64'
        ? {
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
              v: Array(32).fill('0x00000000000000000000000000000000') as Extract<
                Registers,
                { type: 'aarch64' }
              >['data']['v'],
              fpcr: 0,
              fpsr: 0,
            },
          }
        : {
            type: 'x86_64',
            data: {
              gpr: ['0', '0', '0', '0', '0', '0', '0', '0', '0', '0', '0', '0', '0', '0', '0', '0'],
              rip: '0x0000000000001000',
              rflags: '2',
              xmm: Array(16).fill('0x00000000000000000000000000000000') as Extract<
                Registers,
                { type: 'x86_64' }
              >['data']['xmm'],
              mxcsr: 0,
            },
          },
    memory: null,
    fault: null,
    breakpoints: [],
  };
}

export function connection(session: Observation | null = null): ConnectionInfo {
  return {
    connection: '1',
    view: '1',
    session,
    artifact: null,
    capabilities: {
      version: 1,
      targets: ['x86_64', 'aarch64'],
      assembler: { name: 'LLVM MC', version: '23' },
      source_mapping: false,
      execution: true,
    },
  };
}

export function assembled(identity: BuildIdentity): Awaited<ReturnType<WorkerPort['request']>> {
  return {
    response: {
      id: '1',
      result: {
        type: 'assembled',
        data: {
          identity,
          object_bytes: 1,
          image_bytes: 1,
          image: {
            entry: identity.base,
            segments: [],
            symbols: [{ name: 'done', address: '0x0000000000001008', size: '0' }],
            symbols_truncated: false,
          },
        },
      },
    },
    payloads: [new Uint8Array([1]), new Uint8Array([2])],
  };
}

/** Static x86 NOP facts; fixtures never infer instruction behavior. */
export const nopAnalysis = {
  registers: [],
  memory: [],
  branch_target: null,
  architecture: {
    type: 'x86',
    data: {
      flow: 'next',
      cpuid: ['X64'],
      privileged: false,
      registers_incomplete: false,
      flags: { read: [], written: [], cleared: [], set: [], undefined: [] },
    },
  },
} satisfies InstructionAnalysis;

import { desktopWorker, type WorkerPort } from '$lib/desktop/worker';
import type { Artifact } from '$lib/protocol/generated/Artifact';
import type { BuildIdentity } from '$lib/protocol/generated/BuildIdentity';
import type { ConnectionInfo } from '$lib/protocol/generated/ConnectionInfo';
import type { DecodedInstruction } from '$lib/protocol/generated/DecodedInstruction';
import type { DesktopFailure } from '$lib/protocol/generated/DesktopFailure';
import type { Diagnostic } from '$lib/protocol/generated/Diagnostic';
import type { DiagnosticCode } from '$lib/protocol/generated/DiagnosticCode';
import type { FailureCode } from '$lib/protocol/generated/FailureCode';
import type { InstructionAnalysis } from '$lib/protocol/generated/InstructionAnalysis';
import type { LoadImage } from '$lib/protocol/generated/LoadImage';
import type { MemoryWindow } from '$lib/protocol/generated/MemoryWindow';
import type { Observation } from '$lib/protocol/generated/Observation';
import type { SessionAction } from '$lib/protocol/generated/SessionAction';
import type { StreamEvent } from '$lib/protocol/generated/StreamEvent';
import type { Target } from '$lib/protocol/generated/Target';
import { applyObservation } from '$lib/protocol/observations';
import {
  normalizeAddress,
  formatCounter,
  parseCounter,
  sameBuildIdentity,
} from '$lib/protocol/scalars';

import aarch64 from '../../../examples/aarch64.s?raw';
import x86_64 from '../../../examples/x86_64.s?raw';
import { initialMemory, patchBytes } from './machine/memory';
import { initialState, unsigned, type SetupInput } from './machine/setup';
import { readScratch } from './scratch';

type Stream = { event: StreamEvent; memory: Uint8Array | null };
type Snapshot = { observation: Observation; memory: Uint8Array | null };
type Problem = DiagnosticCode | FailureCode | 'completion' | 'input';
class RequestError extends Error {
  constructor(
    readonly code: Problem,
    readonly address: string | null = null,
  ) {
    super(code);
  }
}

type Factory = (
  onstream: (stream: Stream) => void,
  onfailure: (failure: DesktopFailure) => void,
) => WorkerPort | null;

export const examples: Record<Target, string> = {
  x86_64,
  aarch64,
};

/**
 * Own a scratch document, build candidate and separately loaded machine.
 *
 * @remarks
 * Call `initialize` after mounting and `dispose` on unmount. Async results retain
 * their request identities; source edits never mutate the loaded machine. Exposed
 * artifacts and snapshots are read-only by convention, including their byte buffers.
 */
export function createWorkbench(factory: Factory = desktopWorker) {
  let source = $state(examples.x86_64);
  let target = $state<Target>('x86_64');
  let base = $state('0x1000');
  let completion = $state('done');
  let budget = $state('1000000');
  const setups = $state<Record<Target, SetupInput>>({
    x86_64: { registers: [], mappings: [] },
    aarch64: { registers: [], mappings: [] },
  });
  let memoryAddress = $state('0x2000');
  let memoryLength = $state(64);
  let revision = $state(0n);
  let documentId = $state<string>(crypto.randomUUID());
  let info = $state.raw<ConnectionInfo | null>(null);
  let candidate = $state.raw<{ artifact: Artifact; object: Uint8Array; image: Uint8Array } | null>(
    null,
  );
  let binary = $state.raw<Uint8Array>();
  let raw = $state<{ target: Target; base: string; entry: string; completion: string }>({
    target: 'x86_64',
    base: '0x1000',
    entry: '0x1000',
    completion: '',
  });
  type Loaded =
    | { type: 'source'; identity: BuildIdentity }
    | { type: 'raw'; bytes: Uint8Array; target: Target; base: string; entry: string };
  let loaded = $state.raw<Loaded | null>(null);
  let snapshot = $state.raw<Snapshot | null>(null);
  let inspected = $state.raw<Snapshot | null>(null);
  let connected = $state(false);
  let connecting = $state(false);
  let building = $state(false);
  let controlling = $state(false);
  let problem = $state<string | null>(null);
  let buildFailure = $state.raw<{ identity: BuildIdentity; diagnostic: Diagnostic } | null>(null);
  const diagnostic = $derived(
    buildFailure !== null && matches(buildFailure.identity) ? buildFailure.diagnostic : null,
  );
  let storageFailed = $state(false);
  let unknown = $state(false);
  let subscription: string | null = null;
  let memoryRequested = false;
  let baseline: Snapshot | null = null;
  let subscribing = false;
  let subscriptionTask: Promise<void> | null = null;
  let refreshSubscription = false;
  // Channel credit bounds delivery to one event while the subscribe reply is pending.
  let pending: Stream | null = null;
  const lifetime = new AbortController();
  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  const port = factory(receive, failed);

  function identity(): BuildIdentity {
    if (info === null) throw new Error('No worker');
    return {
      document: documentId,
      revision: formatCounter(revision),
      target,
      base: normalizeAddress(base),
      assembler: info.capabilities.assembler,
    };
  }
  function matches(value: BuildIdentity): boolean {
    try {
      return sameBuildIdentity(value, identity());
    } catch {
      return false;
    }
  }
  function report(error: unknown): void {
    if (lifetime.signal.aborted || (error instanceof DOMException && error.name === 'AbortError'))
      return;
    if (
      typeof error === 'object' &&
      error !== null &&
      'code' in error &&
      typeof error.code === 'string'
    ) {
      problem = error.code;
      unknown = 'outcome_unknown' in error && error.outcome_unknown === true;
    } else problem = 'input';
  }
  function failed(error: DesktopFailure): void {
    connected = false;
    inspected = null;
    snapshot = null;
    loaded = null;
    baseline = null;
    subscription = null;
    report(error);
  }
  function publish(next: Snapshot): void {
    if (snapshot !== null) {
      const current = snapshot.observation;
      if (
        parseCounter(next.observation.key.session) < parseCounter(current.key.session) ||
        (next.observation.key.session === current.key.session &&
          parseCounter(next.observation.sequence) <= parseCounter(current.sequence))
      )
        return;
    }
    snapshot = next;
    const bytes = next.memory;
    if (bytes !== null) {
      const previous = inspected;
      // Identical captured bytes retain decode/selection state across observations.
      const sameWindow =
        previous?.observation.key.session === next.observation.key.session &&
        previous.observation.key.generation === next.observation.key.generation &&
        previous.observation.memory?.address === next.observation.memory?.address &&
        previous.memory?.length === bytes.length &&
        previous.memory.every((byte, index) => byte === bytes[index]);
      inspected = sameWindow ? { observation: next.observation, memory: previous.memory } : next;
    }
  }
  function acknowledge(stream: Stream): void {
    const update = stream.event.update;
    const sequence = update.type === 'ended' ? '0' : update.data.sequence;
    void port?.acknowledge(stream.event.subscription, sequence).catch(report);
  }
  function receive(stream: Stream): void {
    if (lifetime.signal.aborted) return;
    if (subscribing) {
      pending = stream;
      return;
    }
    try {
      if (subscription === stream.event.subscription && snapshot !== null) {
        const result = applyObservation(
          { subscription, key: snapshot.observation.key, baseline },
          stream.event,
          stream.memory,
        );
        if (result.type === 'updated') {
          baseline = result.snapshot;
          publish(result.snapshot);
        } else if (result.type === 'ended') {
          problem = result.error.code;
          subscription = null;
          baseline = null;
        } else if (result.type === 'resync') {
          baseline = null;
          void subscribe().catch(report);
        }
      }
    } catch (error) {
      report(error);
    }
    acknowledge(stream);
  }
  function currentSession() {
    return snapshot?.observation.key;
  }
  function subscribe(): Promise<void> {
    if (port === null || snapshot === null || lifetime.signal.aborted) return Promise.resolve();
    refreshSubscription = true;
    subscriptionTask ??= updateSubscription().finally(() => {
      subscriptionTask = null;
      if (refreshSubscription) void subscribe().catch(report);
    });
    return subscriptionTask;
  }
  async function updateSubscription(): Promise<void> {
    subscribing = true;
    try {
      while (
        refreshSubscription &&
        port !== null &&
        snapshot !== null &&
        !lifetime.signal.aborted
      ) {
        refreshSubscription = false;
        const key = snapshot.observation.key;
        const message = await port.request({
          type: 'subscribe',
          data: {
            session: key,
            memory: memoryRequested
              ? { address: normalizeAddress(memoryAddress), length: memoryLength }
              : null,
          },
        });
        lifetime.signal.throwIfAborted();
        const current = currentSession();
        if (current?.session !== key.session || current.generation !== key.generation) continue;
        if (message.response.result.type === 'error')
          throw new RequestError(message.response.result.data.code);
        if (message.response.result.type !== 'subscribed')
          throw new Error('Unexpected subscription reply');
        subscription = message.response.result.data;
        baseline = null;
      }
    } finally {
      subscribing = false;
      const delayed = pending;
      pending = null;
      if (delayed !== null) receive(delayed);
    }
  }
  async function connect(restart = false): Promise<void> {
    if (port === null || connecting) return;
    connecting = true;
    connected = false;
    problem = null;
    unknown = false;
    try {
      const next = await port.connect(restart);
      lifetime.signal.throwIfAborted();
      buildFailure = null;
      info = next;
      connected = true;
      // A retained native session does not prove which local source produced it.
      loaded = null;
      inspected = null;
      subscription = null;
      baseline = null;
      snapshot = next.session === null ? null : { observation: next.session, memory: null };
      const window = next.session?.memory;
      memoryRequested = window != null;
      if (window != null) {
        memoryAddress = window.address;
        memoryLength = window.length;
      }
      if (next.session !== null) await subscribe();
    } catch (error) {
      report(error);
    } finally {
      connecting = false;
    }
  }
  async function assemble(): Promise<void> {
    if (port === null || !connected || building) return;
    building = true;
    buildFailure = null;
    problem = null;
    unknown = false;
    try {
      const build = identity();
      const message = await port.request({ type: 'assemble', data: { identity: build, source } });
      if (lifetime.signal.aborted || !matches(build)) return;
      const result = message.response.result;
      if (result.type === 'error') {
        buildFailure = { identity: build, diagnostic: result.data };
        return;
      }
      const object = message.payloads[0];
      const image = message.payloads[1];
      if (
        result.type !== 'assembled' ||
        object === undefined ||
        image === undefined ||
        !sameBuildIdentity(result.data.identity, build)
      )
        throw new Error('Invalid artifact');
      candidate = { artifact: result.data, object, image };
    } catch (error) {
      report(error);
    } finally {
      building = false;
    }
  }
  async function decode(
    bytes: Uint8Array,
    guest: Target,
    address: string,
  ): Promise<DecodedInstruction[]> {
    if (port === null || !connected) throw new RequestError('unavailable');
    const message = await port.request({
      type: 'decode',
      data: {
        target: guest,
        base: normalizeAddress(address),
        bytes: Array.from(bytes),
        limit: 256,
      },
    });
    lifetime.signal.throwIfAborted();
    const result = message.response.result;
    if (result.type === 'error') throw new RequestError(result.data.code, result.data.address);
    if (result.type !== 'decoded') throw new RequestError('protocol');
    return result.data;
  }
  async function analyze(
    bytes: Uint8Array,
    guest: Target,
    address: string,
  ): Promise<InstructionAnalysis> {
    if (port === null || !connected) throw new RequestError('unavailable');
    const message = await port.request({
      type: 'analyze',
      data: { target: guest, base: normalizeAddress(address), bytes: Array.from(bytes) },
    });
    lifetime.signal.throwIfAborted();
    const result = message.response.result;
    if (result.type === 'error') throw new RequestError(result.data.code, result.data.address);
    if (result.type !== 'analyzed') throw new RequestError('protocol');
    return result.data;
  }
  function completionAddress(artifact: Artifact): string {
    if (/^(?:0x)?[\da-f]+$/i.test(completion.trim())) return normalizeAddress(completion);
    const values = artifact.image.symbols
      .filter((symbol) => symbol.name === completion.trim())
      .map((symbol) => symbol.address);
    const [address] = values;
    if (address === undefined || values.some((value) => value !== address))
      throw new RequestError('completion');
    return address;
  }
  type LoadInput = {
    bytes: Uint8Array;
    image: LoadImage;
    target: Target;
    completion: string;
    identity: Loaded;
    memory: MemoryWindow | null;
  };
  async function loadInput(prepare: () => LoadInput): Promise<void> {
    if (
      port === null ||
      !connected ||
      controlling ||
      snapshot?.observation.status.type === 'running'
    )
      return;
    controlling = true;
    problem = null;
    unknown = false;
    try {
      const input = prepare();
      const maximum = BigInt(budget);
      if (maximum < 1n || maximum > 100000000n) throw new RangeError('Invalid budget');
      const message = await port.request(
        {
          type: 'load',
          data: {
            image: input.image,
            replace: snapshot?.observation.key ?? null,
            target: input.target,
            completion: input.completion,
            instruction_budget: formatCounter(maximum),
            image_bytes: input.bytes.length,
            initial: initialState(setups[input.target]),
          },
        },
        input.bytes,
      );
      lifetime.signal.throwIfAborted();
      const result = message.response.result;
      if (result.type === 'error') throw new RequestError(result.data.code);
      if (result.type !== 'observed') throw new Error('Invalid load result');
      snapshot = { observation: result.data, memory: null };
      inspected = null;
      loaded = input.identity;
      const window = input.memory;
      memoryRequested = window !== null;
      if (window !== null) {
        memoryAddress = window.address;
        memoryLength = window.length;
      }
      await subscribe();
    } catch (error) {
      report(error);
    } finally {
      controlling = false;
    }
  }
  function load(): Promise<void> {
    return loadInput(() => {
      if (candidate === null || !matches(candidate.artifact.identity))
        throw new RequestError('input');
      return {
        bytes: candidate.image,
        image: { type: 'elf' },
        target: candidate.artifact.identity.target,
        completion: completionAddress(candidate.artifact),
        identity: { type: 'source', identity: candidate.artifact.identity },
        memory: initialMemory(candidate.artifact.image),
      };
    });
  }
  function loadRaw(): Promise<void> {
    return loadInput(() => {
      if (binary === undefined) throw new RequestError('input');
      const base = normalizeAddress(raw.base);
      const entry = normalizeAddress(raw.entry);
      return {
        bytes: binary,
        image: { type: 'raw', data: { base, entry } },
        target: raw.target,
        completion: normalizeAddress(raw.completion),
        identity: { type: 'raw', bytes: binary, target: raw.target, base, entry },
        memory: { address: base, length: Math.min(binary.length, 64) },
      };
    });
  }
  async function execute(action: SessionAction, payload?: Uint8Array): Promise<void> {
    if (port === null || snapshot === null || controlling || !connected) return;
    controlling = true;
    problem = null;
    unknown = false;
    try {
      const message = await port.request(
        {
          type: 'execute',
          data: { session: snapshot.observation.key, action },
        },
        payload,
      );
      lifetime.signal.throwIfAborted();
      const result = message.response.result;
      if (result.type === 'error') throw new RequestError(result.data.code);
      if (result.type === 'observed') {
        publish({ observation: result.data, memory: message.payloads[0] ?? null });
        if (action.type === 'reset') await subscribe();
        if (action.type === 'write_memory' && result.data.status.type !== 'crashed') {
          inspected = null;
          memoryAddress = action.data.address;
          memoryLength = action.data.length;
          memoryRequested = true;
          await subscribe();
        }
      } else if (result.type === 'session_closed') {
        snapshot = null;
        inspected = null;
        loaded = null;
        subscription = null;
        baseline = null;
      }
    } catch (error) {
      report(error);
    } finally {
      controlling = false;
    }
  }
  function save(): void {
    try {
      localStorage.setItem(
        'oplab.scratch.v1',
        JSON.stringify({
          documentId,
          source,
          target,
          base,
          completion,
          budget,
          revision: formatCounter(revision),
        }),
      );
      storageFailed = false;
    } catch {
      storageFailed = true;
    }
  }
  function persist(): void {
    clearTimeout(saveTimer);
    saveTimer = setTimeout(save, 250);
  }
  function initialize(): void {
    try {
      const stored = localStorage.getItem('oplab.scratch.v1');
      if (stored !== null) {
        const value = readScratch(JSON.parse(stored));
        const storedRevision = parseCounter(value.revision);
        source = value.source;
        target = value.target;
        revision = storedRevision;
        documentId = value.documentId;
        base = value.base;
        completion = value.completion;
        budget = value.budget;
      }
    } catch {
      storageFailed = true;
    }
    void connect();
  }
  return {
    decode,
    analyze,
    get source() {
      return source;
    },
    setSource(value: string) {
      if (value !== source) {
        source = value;
        if (revision === (1n << 64n) - 1n) {
          documentId = crypto.randomUUID();
          revision = 0n;
        } else revision += 1n;
        persist();
      }
    },
    get target() {
      return target;
    },
    setTarget(value: Target) {
      target = value;
      persist();
    },
    get base() {
      return base;
    },
    set base(value: string) {
      base = value;
      persist();
    },
    get completion() {
      return completion;
    },
    set completion(value: string) {
      completion = value;
      persist();
    },
    get setup() {
      return setups[target];
    },
    set setup(value: SetupInput) {
      setups[target] = value;
    },
    get budget() {
      return budget;
    },
    set budget(value: string) {
      budget = value;
      persist();
    },
    get memoryAddress() {
      return memoryAddress;
    },
    set memoryAddress(value: string) {
      memoryAddress = value;
    },
    get memoryLength() {
      return memoryLength;
    },
    set memoryLength(value: number) {
      memoryLength = value;
    },
    get preview() {
      return port === null;
    },
    get connected() {
      return connected;
    },
    get connecting() {
      return connecting;
    },
    get building() {
      return building;
    },
    get controlling() {
      return controlling;
    },
    get problem() {
      return problem ?? diagnostic?.code ?? (storageFailed ? 'storage' : null);
    },
    get diagnostic() {
      return diagnostic;
    },
    get unknown() {
      return unknown;
    },
    get candidate() {
      return candidate;
    },
    get artifactCurrent() {
      return candidate !== null && matches(candidate.artifact.identity);
    },
    get loadedCurrent() {
      if (loaded === null) return false;
      if (loaded.type === 'source') return matches(loaded.identity);
      try {
        return (
          loaded.bytes === binary &&
          loaded.target === raw.target &&
          loaded.base === normalizeAddress(raw.base) &&
          loaded.entry === normalizeAddress(raw.entry)
        );
      } catch {
        return false;
      }
    },
    get loadedRevision() {
      return loaded?.type === 'source' ? loaded.identity.revision : null;
    },
    get loadedKind() {
      return loaded?.type ?? null;
    },
    get binary() {
      return binary;
    },
    get raw() {
      return raw;
    },
    set raw(value: typeof raw) {
      raw = value;
    },
    get rawSetup() {
      return setups[raw.target];
    },
    set rawSetup(value: SetupInput) {
      setups[raw.target] = value;
    },
    importBinary(bytes: Uint8Array) {
      binary = bytes;
      raw.target = target;
    },
    loadRaw,
    async writeRegister(name: string, value: string) {
      try {
        await execute({
          type: 'write_register',
          data: { name, value: formatCounter(unsigned(value)) },
        });
      } catch (error) {
        report(error);
      }
    },
    async writeMemory(address: string, text: string) {
      try {
        const bytes = patchBytes(text);
        await execute(
          {
            type: 'write_memory',
            data: { address: normalizeAddress(address), length: bytes.length },
          },
          bytes,
        );
      } catch (error) {
        report(error);
      }
    },
    async breakpoint(address: string, enabled: boolean) {
      try {
        await execute({
          type: 'breakpoint',
          data: { address: normalizeAddress(address), enabled },
        });
      } catch (error) {
        report(error);
      }
    },
    get inspected() {
      const key = snapshot?.observation.key;
      return connected &&
        inspected?.observation.key.session === key?.session &&
        inspected?.observation.key.generation === key?.generation
        ? inspected
        : null;
    },
    get snapshot() {
      return snapshot;
    },
    get revision() {
      return formatCounter(revision);
    },
    initialize,
    connect,
    assemble,
    load,
    execute,
    async inspect() {
      problem = null;
      unknown = false;
      memoryRequested = true;
      try {
        await subscribe();
      } catch (error) {
        report(error);
      }
    },
    dispose() {
      lifetime.abort();
      clearTimeout(saveTimer);
      save();
      port?.detach();
    },
  };
}

import { desktopWorker, type WorkerPort } from '$lib/desktop/worker';
import type { Artifact } from '$lib/protocol/generated/Artifact';
import type { BuildIdentity } from '$lib/protocol/generated/BuildIdentity';
import type { ConnectionInfo } from '$lib/protocol/generated/ConnectionInfo';
import type { DataWatchpoint } from '$lib/protocol/generated/DataWatchpoint';
import type { DecodedInstruction } from '$lib/protocol/generated/DecodedInstruction';
import type { DesktopFailure } from '$lib/protocol/generated/DesktopFailure';
import type { DiagnosticCode } from '$lib/protocol/generated/DiagnosticCode';
import type { ExecutionTrace } from '$lib/protocol/generated/ExecutionTrace';
import type { FailureCode } from '$lib/protocol/generated/FailureCode';
import type { InstructionAnalysis } from '$lib/protocol/generated/InstructionAnalysis';
import type { LoadImage } from '$lib/protocol/generated/LoadImage';
import type { MemoryWindow } from '$lib/protocol/generated/MemoryWindow';
import type { Observation } from '$lib/protocol/generated/Observation';
import type { RoundingMode } from '$lib/protocol/generated/RoundingMode';
import type { SessionAction } from '$lib/protocol/generated/SessionAction';
import type { SourceMap } from '$lib/protocol/generated/SourceMap';
import type { StreamEvent } from '$lib/protocol/generated/StreamEvent';
import type { Target } from '$lib/protocol/generated/Target';
import type { VectorWrite } from '$lib/protocol/generated/VectorWrite';
import { sameBuildIdentity } from '$lib/protocol/identity';
import { applyObservation } from '$lib/protocol/observations';
import {
  normalizeAddress,
  formatCounter,
  parseCounter,
  parseUnsigned,
} from '$lib/protocol/scalars';
import { untrack } from 'svelte';
import { SvelteSet } from 'svelte/reactivity';

import aarch64 from '../../../examples/aarch64.s?raw';
import x86_64 from '../../../examples/x86_64.s?raw';
import { SourceDocument, newDocument } from './document.svelte';
import { initialState, type InitialInput } from './load/initial-state';
import { initialMemory, patchBytes } from './machine/memory';
import { readWorkspace } from './scratch';

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
 * Own source documents and their builds, independently of the loaded machine.
 *
 * @remarks
 * Call `initialize` after mounting and `dispose` on unmount. Async results retain
 * their request identities; source edits never mutate the loaded machine. Exposed
 * artifacts and snapshots are read-only by convention, including their byte buffers.
 */
export function createWorkbench(factory: Factory = desktopWorker) {
  let trace = $state.raw<ExecutionTrace | null>(null);
  let traceRequest: symbol | undefined;
  let active = $state(newDocument('untitled-1.s'));
  let documents = $state([untrack(() => active)]);
  const source = $derived(active.inputs.source);
  const target = $derived(active.inputs.target);
  const base = $derived(active.inputs.base);
  const completion = $derived(active.inputs.completion);
  const budget = $derived(active.inputs.budget);
  const setups = $derived(active.setups);
  const documentId = $derived(active.inputs.documentId);
  const revision = $derived(parseCounter(active.inputs.revision));
  const candidate = $derived(active.candidate);
  let memoryAddress = $state('0x2000');
  let memoryLength = $state(64);
  let info = $state.raw<ConnectionInfo | null>(null);
  let binary = $state.raw<Uint8Array>();
  let rawBudget = $state('1000000');
  const rawSetups = $state<Record<Target, InitialInput>>({
    x86_64: { registers: [], mappings: [] },
    aarch64: { registers: [], mappings: [] },
  });
  let raw = $state<{ target: Target; base: string; entry: string; completion: string }>({
    target: 'x86_64',
    base: '0x1000',
    entry: '0x1000',
    completion: '',
  });
  type Loaded =
    | { type: 'source'; identity: BuildIdentity; sourceMap: SourceMap }
    | { type: 'raw'; bytes: Uint8Array; target: Target; base: string; entry: string };
  let loaded = $state.raw<Loaded | null>(null);
  let snapshot = $state.raw<Snapshot | null>(null);
  let inspected = $state.raw<Snapshot | null>(null);
  let connected = $state(false);
  let connecting = $state(false);
  let building = $state(false);
  let controlling = $state(false);
  let problem = $state<string | null>(null);
  const buildFailure = $derived(active.failure);
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
  let recoveryWritable = true;
  const port = factory(receive, failed);

  function identity(document = active): BuildIdentity {
    if (info === null) throw new Error('No worker');
    return {
      document: document.inputs.documentId,
      revision: document.inputs.revision,
      target: document.inputs.target,
      base: normalizeAddress(document.inputs.base),
      assembler: info.capabilities.assembler,
    };
  }
  function matches(value: BuildIdentity, document = active): boolean {
    try {
      return sameBuildIdentity(value, identity(document));
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
    trace = null;
    traceRequest = undefined;
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
    trace = null;
    traceRequest = undefined;
    problem = null;
    unknown = false;
    try {
      const next = await port.connect(restart);
      lifetime.signal.throwIfAborted();
      active.failure = null;
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
    active.failure = null;
    problem = null;
    unknown = false;
    const document = active;
    try {
      const build = identity(document);
      const message = await port.request({ type: 'assemble', data: { identity: build, source } });
      if (lifetime.signal.aborted || !documents.includes(document) || !matches(build, document))
        return;
      const result = message.response.result;
      if (result.type === 'error') {
        document.failure = { identity: build, diagnostic: result.data };
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
      document.candidate = { artifact: result.data, object, image };
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
    setup: InitialInput;
    budget: string;
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
      const maximum = BigInt(input.budget);
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
            initial: initialState(input.setup),
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
        identity: {
          type: 'source',
          identity: candidate.artifact.identity,
          sourceMap: candidate.artifact.source_map,
        },
        memory: initialMemory(candidate.artifact.image),
        setup: setups[candidate.artifact.identity.target],
        budget,
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
        setup: rawSetups[raw.target],
        budget: rawBudget,
      };
    });
  }
  async function readTrace(): Promise<void> {
    if (port === null || snapshot === null || !connected) return;
    const key = snapshot.observation.key;
    const request = Symbol();
    traceRequest = request;
    try {
      const message = await port.request({
        type: 'execute',
        data: { session: key, action: { type: 'read_trace' } },
      });
      lifetime.signal.throwIfAborted();
      const current = currentSession();
      if (
        traceRequest !== request ||
        current?.session !== key.session ||
        current.generation !== key.generation
      )
        return;
      const result = message.response.result;
      if (result.type === 'error') throw new RequestError(result.data.code);
      if (
        result.type !== 'trace' ||
        result.data.key.session !== key.session ||
        result.data.key.generation !== key.generation
      )
        throw new RequestError('protocol');
      trace = result.data;
    } catch (error) {
      const current = currentSession();
      if (
        traceRequest === request &&
        current?.session === key.session &&
        current.generation === key.generation
      )
        report(error);
    }
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
        if (action.type === 'record_trace' || action.type === 'clear_trace') await readTrace();
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
    if (!recoveryWritable) return;
    try {
      localStorage.setItem(
        'oplab.workspace.v1',
        JSON.stringify(
          readWorkspace({
            active: documentId,
            documents: documents.map((document) => ({ ...document.inputs, name: document.name })),
          }),
        ),
      );
      storageFailed = false;
    } catch {
      storageFailed = true;
    }
  }
  function persist(): void {
    recoveryWritable = true;
    clearTimeout(saveTimer);
    saveTimer = setTimeout(save, 250);
  }
  function initialize(): void {
    try {
      const stored = localStorage.getItem('oplab.workspace.v1');
      if (stored !== null) {
        const workspace = readWorkspace(JSON.parse(stored));
        documents = workspace.documents.map(
          ({ name, ...inputs }) => new SourceDocument(inputs, name),
        );
        active =
          documents.find((document) => document.inputs.documentId === workspace.active) ?? active;
      }
    } catch {
      recoveryWritable = false;
      storageFailed = true;
    }
    void connect();
  }
  return {
    get documents() {
      return documents.map((document) => ({
        id: document.inputs.documentId,
        name: document.name,
        hasSource: document.inputs.source.length > 0,
      }));
    },
    get documentId() {
      return documentId;
    },
    get documentName() {
      return active.name;
    },
    get loadedDocument() {
      const origin = loaded?.type === 'source' ? loaded.identity.document : null;
      return origin !== null
        ? (documents.find((document) => document.inputs.documentId === origin)?.inputs.documentId ??
            null)
        : null;
    },
    selectDocument(id: string) {
      const document = documents.find((document) => document.inputs.documentId === id);
      if (document === undefined || document === active) return;
      active = document;
      persist();
    },
    createDocument() {
      const names = new SvelteSet(documents.map((document) => document.name));
      let number = 1;
      while (names.has(`untitled-${String(number)}.s`)) number++;
      active = newDocument(`untitled-${String(number)}.s`, target);
      documents.push(active);
      persist();
    },
    renameDocument(name: string) {
      const trimmed = name.trim();
      if (trimmed.length === 0 || trimmed.length > 80) return;
      active.name = trimmed;
      persist();
    },
    closeDocument(id: string) {
      const index = documents.findIndex((document) => document.inputs.documentId === id);
      if (index < 0) return;
      const closingActive = active.inputs.documentId === id;
      documents = documents.filter((document) => document.inputs.documentId !== id);
      if (closingActive)
        active = documents[Math.min(index, documents.length - 1)] ?? newDocument('untitled-1.s');
      if (documents.length === 0) documents = [active];
      persist();
    },
    decode,
    analyze,
    readTrace,
    get trace() {
      const key = snapshot?.observation.key;
      return connected &&
        trace?.key.session === key?.session &&
        trace?.key.generation === key?.generation
        ? trace
        : null;
    },
    async runToSource(line: number) {
      if (loaded?.type !== 'source' || !matches(loaded.identity)) return;
      const addresses = loaded.sourceMap.locations
        .filter((point) => point.line === line)
        .map((point) => point.address);
      if (addresses.length > 0) await execute({ type: 'run_until', data: { addresses } });
    },
    async runToAddress(address: string) {
      try {
        await execute({ type: 'run_until', data: { addresses: [normalizeAddress(address)] } });
      } catch (error) {
        report(error);
      }
    },
    get source() {
      return source;
    },
    setSource(value: string) {
      if (value !== source) {
        active.inputs.source = value;
        if (revision === (1n << 64n) - 1n) {
          active.inputs.documentId = crypto.randomUUID();
          active.inputs.revision = '0';
        } else active.inputs.revision = formatCounter(revision + 1n);
        persist();
      }
    },
    get target() {
      return target;
    },
    setTarget(value: Target) {
      active.inputs.target = value;
      persist();
    },
    get base() {
      return base;
    },
    set base(value: string) {
      active.inputs.base = value;
      persist();
    },
    get completion() {
      return completion;
    },
    set completion(value: string) {
      active.inputs.completion = value;
      persist();
    },
    get setup() {
      return setups[target];
    },
    set setup(value: InitialInput) {
      setups[target] = value;
    },
    get budget() {
      return budget;
    },
    set budget(value: string) {
      active.inputs.budget = value;
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
      if (loaded.type === 'source') {
        const build = loaded.identity;
        const document = documents.find(
          (document) => document.inputs.documentId === build.document,
        );
        return document !== undefined && matches(build, document);
      }
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
    get sourceMap() {
      return candidate !== null && matches(candidate.artifact.identity)
        ? candidate.artifact.source_map
        : null;
    },
    get loadedSourceMap() {
      return loaded?.type === 'source' && matches(loaded.identity) ? loaded.sourceMap : null;
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
    get rawBudget() {
      return rawBudget;
    },
    set rawBudget(value: string) {
      rawBudget = value;
    },
    get rawSetup() {
      return rawSetups[raw.target];
    },
    set rawSetup(value: InitialInput) {
      rawSetups[raw.target] = value;
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
          data: { name, value: formatCounter(parseUnsigned(value)) },
        });
      } catch (error) {
        report(error);
      }
    },
    async setRounding(mode: RoundingMode) {
      await execute({ type: 'set_rounding', data: mode });
    },
    async writeVector(write: VectorWrite) {
      await execute({ type: 'write_vector', data: write });
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
    async watchpoints(points: DataWatchpoint[]) {
      try {
        const data = points.map((point) => ({
          ...point,
          address: normalizeAddress(point.address),
        }));
        await execute({ type: 'watchpoints', data });
      } catch (error) {
        report(error);
      }
    },
    async breakpoint(address: string, enabled: boolean) {
      try {
        await execute({
          type: 'breakpoint',
          data: { addresses: [normalizeAddress(address)], enabled },
        });
      } catch (error) {
        report(error);
      }
    },
    async sourceBreakpoint(line: number) {
      if (loaded?.type !== 'source' || !matches(loaded.identity) || snapshot === null) return;
      const addresses = loaded.sourceMap.locations
        .filter((point) => point.line === line)
        .map((point) => point.address);
      if (addresses.length === 0) return;
      const active = snapshot.observation.breakpoints;
      await execute({
        type: 'breakpoint',
        data: { addresses, enabled: !addresses.every((address) => active.includes(address)) },
      });
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

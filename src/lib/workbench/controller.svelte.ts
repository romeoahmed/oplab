import { readScratch } from './scratch';
import { initialMemory } from './machine/memory';
import { desktopWorker, type WorkerPort } from '$lib/desktop/worker';
import { applyObservation } from '$lib/protocol/observations';
import {
  normalizeAddress,
  formatCounter,
  parseCounter,
  sameBuildIdentity,
} from '$lib/protocol/scalars';
import type { Artifact } from '$lib/protocol/generated/Artifact';
import type { BuildIdentity } from '$lib/protocol/generated/BuildIdentity';
import type { ConnectionInfo } from '$lib/protocol/generated/ConnectionInfo';
import type { DecodedInstruction } from '$lib/protocol/generated/DecodedInstruction';
import type { Diagnostic } from '$lib/protocol/generated/Diagnostic';
import type { DiagnosticCode } from '$lib/protocol/generated/DiagnosticCode';
import type { DesktopFailure } from '$lib/protocol/generated/DesktopFailure';
import type { FailureCode } from '$lib/protocol/generated/FailureCode';
import type { Observation } from '$lib/protocol/generated/Observation';
import type { SessionAction } from '$lib/protocol/generated/SessionAction';
import type { StreamEvent } from '$lib/protocol/generated/StreamEvent';
import type { Target } from '$lib/protocol/generated/Target';

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
  x86_64:
    '.intel_syntax noprefix\n.text\n# Store the answer, then stop at done.\n\nmov rax, 40\nadd rax, 2\nmov qword ptr [rip + output], rax\ndone: nop\n\n.bss\noutput: .skip 64\n',
  aarch64:
    '.text\n// Store the answer, then stop at done.\n\nmov x0, #40\nadd x0, x0, #2\nadr x1, output\nstr x0, [x1]\ndone: nop\n\n.bss\noutput: .skip 64\n',
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
  let memoryAddress = $state('0x2000');
  let memoryLength = $state(64);
  let revision = $state(0n);
  let documentId = $state<string>(crypto.randomUUID());
  let info = $state.raw<ConnectionInfo | null>(null);
  let candidate = $state.raw<{ artifact: Artifact; object: Uint8Array; image: Uint8Array } | null>(
    null,
  );
  let loaded = $state.raw<BuildIdentity | null>(null);
  let snapshot = $state.raw<Snapshot | null>(null);
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
    if (message.response.result.type === 'error') {
      const failure = message.response.result.data;
      throw new RequestError(failure.code, failure.address);
    }
    if (message.response.result.type !== 'decoded') throw new RequestError('protocol');
    return message.response.result.data;
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
  async function load(): Promise<void> {
    if (port === null || candidate === null || controlling || !matches(candidate.artifact.identity))
      return;
    controlling = true;
    problem = null;
    unknown = false;
    const artifact = candidate;
    try {
      const maximum = BigInt(budget);
      if (maximum < 1n || maximum > 100000000n) throw new RangeError('Invalid budget');
      const message = await port.request(
        {
          type: 'load',
          data: {
            replace: snapshot?.observation.key ?? null,
            target: artifact.artifact.identity.target,
            completion: completionAddress(artifact.artifact),
            instruction_budget: formatCounter(maximum),
            image_bytes: artifact.image.length,
          },
        },
        artifact.image,
      );
      lifetime.signal.throwIfAborted();
      const result = message.response.result;
      if (result.type === 'error') throw new RequestError(result.data.code);
      if (result.type !== 'observed') throw new Error('Invalid load result');
      snapshot = { observation: result.data, memory: null };
      loaded = artifact.artifact.identity;
      const window = initialMemory(artifact.artifact.image);
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
  async function execute(action: SessionAction): Promise<void> {
    if (port === null || snapshot === null || controlling || !connected) return;
    controlling = true;
    problem = null;
    unknown = false;
    try {
      const message = await port.request({
        type: 'execute',
        data: { session: snapshot.observation.key, action },
      });
      lifetime.signal.throwIfAborted();
      const result = message.response.result;
      if (result.type === 'error') throw new RequestError(result.data.code);
      if (result.type === 'observed') {
        publish({ observation: result.data, memory: message.payloads[0] ?? null });
        if (action.type === 'reset') await subscribe();
      } else if (result.type === 'session_closed') {
        snapshot = null;
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
      return loaded !== null && matches(loaded);
    },
    get loadedRevision() {
      return loaded?.revision ?? null;
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

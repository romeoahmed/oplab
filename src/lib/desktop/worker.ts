import { decodeResponse, decodeStream, encodeCall } from '$lib/protocol/frames';
import type { Command } from '$lib/protocol/generated/Command';
import type { ConnectionInfo } from '$lib/protocol/generated/ConnectionInfo';
import type { Counter } from '$lib/protocol/generated/Counter';
import type { DesktopFailure } from '$lib/protocol/generated/DesktopFailure';
import { Channel, invoke, isTauri } from '@tauri-apps/api/core';

type Stream = ReturnType<typeof decodeStream>;
/** A mounted view's worker attachment; requests are never retried automatically. */
export type WorkerPort = {
  /** Attach to the worker, or replace it and discard its state when `restart` is true. */
  connect: (restart: boolean) => Promise<ConnectionInfo>;
  /** Resolve a complete reply, including tagged engine errors; transport failures reject. */
  request: (command: Command, payload?: Uint8Array) => Promise<ReturnType<typeof decodeResponse>>;
  /** Return delivery credit after consuming an event; detached views have no credit to return. */
  acknowledge: (subscription: Counter, sequence: Counter) => Promise<void>;
  /** Invalidate this view immediately and release its native lease asynchronously. */
  detach: () => void;
};

/**
 * Create a worker port for one mounted workbench.
 *
 * @remarks
 * Reattachment invalidates pending callers. Detaching preserves admitted native
 * work and suppresses late events; it does not cancel a guest operation.
 *
 * @returns A disconnected port in Tauri, or `null` in browser preview.
 */
export function desktopWorker(
  onstream: (stream: Stream) => void,
  onfailure: (failure: DesktopFailure) => void,
): WorkerPort | null {
  if (!isTauri()) return null;
  let attachment: ConnectionInfo | null = null;
  let epoch = 0;
  return {
    async connect(restart) {
      const current = ++epoch;
      attachment = null;
      const channel = new Channel<ArrayBuffer | DesktopFailure>((message) => {
        if (current !== epoch) return;
        try {
          if (message instanceof ArrayBuffer) onstream(decodeStream(message));
          else onfailure(message);
        } catch {
          onfailure({ code: 'protocol', request: null, outcome_unknown: false });
        }
      });
      const info = await invoke<ConnectionInfo>('worker_connect', { channel, restart });
      if (current !== epoch) {
        await invoke('worker_detach', { connection: info.connection, view: info.view });
        throw new DOMException('Detached workbench', 'AbortError');
      }
      attachment = info;
      return info;
    },
    async request(command, payload) {
      if (attachment === null) throw new Error('Worker not connected');
      const current = epoch;
      const bytes = encodeCall(
        { connection: attachment.connection, view: attachment.view, command },
        payload ?? null,
      );
      const response = await invoke<ArrayBuffer>('worker_request', bytes);
      if (current !== epoch) throw new DOMException('Detached workbench', 'AbortError');
      return decodeResponse(response);
    },
    async acknowledge(subscription, sequence) {
      if (attachment === null) return;
      await invoke('worker_ack', {
        connection: attachment.connection,
        view: attachment.view,
        subscription,
        sequence,
      });
    },
    detach() {
      epoch += 1;
      if (attachment !== null) {
        void invoke('worker_detach', {
          connection: attachment.connection,
          view: attachment.view,
        }).catch(() => {});
        attachment = null;
      }
    },
  };
}

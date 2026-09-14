import { Channel, invoke, isTauri } from '@tauri-apps/api/core';
import type { Command } from '$lib/protocol/generated/Command';
import type { ConnectionInfo } from '$lib/protocol/generated/ConnectionInfo';
import type { Counter } from '$lib/protocol/generated/Counter';
import type { DesktopFailure } from '$lib/protocol/generated/DesktopFailure';
import { decodeResponse, decodeStream, encodeCall } from '$lib/protocol/frames';

type Stream = ReturnType<typeof decodeStream>;
export type WorkerPort = {
  connect: (restart: boolean) => Promise<ConnectionInfo>;
  request: (command: Command, image?: Uint8Array) => Promise<ReturnType<typeof decodeResponse>>;
  acknowledge: (subscription: Counter, sequence: Counter) => Promise<void>;
  detach: () => void;
};

/** Scope one Channel to one mounted workbench; a new attachment invalidates old callers. */
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
    async request(command, image) {
      if (attachment === null) throw new Error('Worker not connected');
      const current = epoch;
      const bytes = encodeCall(
        { connection: attachment.connection, view: attachment.view, command },
        image ?? null,
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

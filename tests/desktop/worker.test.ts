import { desktopWorker, type WorkerPort } from '$lib/desktop/worker';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { beforeEach, expect, test, vi } from 'vitest';

import { connection } from '../fixtures/protocol';

vi.mock('@tauri-apps/api/core', () => ({
  isTauri: vi.fn(),
  invoke: vi.fn(),
  Channel: class {
    constructor(readonly onmessage: (message: unknown) => void) {}
  },
}));

beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(isTauri).mockReturnValue(true);
});

function port(): WorkerPort {
  const result = desktopWorker(vi.fn(), vi.fn());
  if (result === null) throw new Error('Expected a desktop attachment');
  return result;
}

test.each(['request', 'acknowledge'] as const)(
  '%s rejects obsolete successes and failures without affecting a new attachment',
  async (operation) => {
    const worker = port();
    vi.mocked(invoke).mockResolvedValue(connection());
    await worker.connect(false);
    const perform = () =>
      operation === 'request'
        ? worker.request({ type: 'unsubscribe', data: { subscription: '1' } })
        : worker.acknowledge('1', '1');
    for (const reject of [false, true]) {
      const pending = Promise.withResolvers<unknown>();
      vi.mocked(invoke).mockReturnValueOnce(pending.promise);
      const assertion = expect(perform()).rejects.toMatchObject({ name: 'AbortError' });
      await worker.connect(false);
      if (reject) pending.reject({ code: 'disconnected' });
      else pending.resolve(new ArrayBuffer(0));
      await assertion;
    }
    const failure = { code: 'deadline', request: '3', outcome_unknown: true };
    vi.mocked(invoke).mockRejectedValueOnce(failure);
    await expect(perform()).rejects.toBe(failure);
  },
);

test('a detached pending connection releases only its own lease, even when release fails', async () => {
  const worker = port();
  const pending = Promise.withResolvers<unknown>();
  vi.mocked(invoke).mockReturnValueOnce(pending.promise);
  const result = worker.connect(false);
  const assertion = expect(result).rejects.toMatchObject({ name: 'AbortError' });
  worker.detach();
  vi.mocked(invoke).mockRejectedValueOnce({ code: 'stale_connection' });
  pending.resolve(connection());
  await assertion;
  expect(invoke).toHaveBeenLastCalledWith('worker_detach', { connection: '1', view: '1' });
  await expect(
    worker.request({ type: 'unsubscribe', data: { subscription: '1' } }),
  ).rejects.toThrow();
});

test('a pending connection failure is obsolete after detach', async () => {
  const worker = port();
  const pending = Promise.withResolvers<unknown>();
  vi.mocked(invoke).mockReturnValueOnce(pending.promise);
  const result = worker.connect(false);
  const assertion = expect(result).rejects.toMatchObject({ name: 'AbortError' });
  worker.detach();
  pending.reject({ code: 'unavailable' });
  await assertion;
});

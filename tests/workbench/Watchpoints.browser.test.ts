import type { WorkerPort } from '$lib/desktop/worker';
import { remove_watchpoint } from '$lib/paraglide/messages.js';
import type { DataWatchpoint } from '$lib/protocol/generated/DataWatchpoint';
import type { Observation } from '$lib/protocol/generated/Observation';
import type { SessionAction } from '$lib/protocol/generated/SessionAction';
import Workbench from '$lib/workbench/Workbench.svelte';
import { settled } from 'svelte';
import { afterEach, expect, test, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page, userEvent } from 'vitest/browser';

import en from '../../messages/en.json';
import zh from '../../messages/zh-CN.json';
import { connection, observation } from '../fixtures/protocol';

import '$lib/styles/theme.css';

afterEach(() => {
  localStorage.clear();
});

test.each(['en', 'zh-CN'] as const)(
  'watchpoint edits wait for acceptance, retain rejected state and resume in %s',
  async (locale) => {
    localStorage.clear();
    localStorage.setItem('PARAGLIDE_LOCALE', locale);
    const copy = locale === 'en' ? en : zh;
    const machine = observation('1');
    const point: DataWatchpoint = {
      address: '0x0000000000002000',
      length: 8,
      access: 'read_write',
    };
    type Result = Awaited<ReturnType<WorkerPort['request']>>;
    const observed = (data: Observation): Result => ({
      response: { id: data.sequence, result: { type: 'observed', data } },
      payloads: [],
    });
    const failure: Result = {
      response: {
        id: '1',
        result: {
          type: 'error',
          data: { code: 'invalid_input', address: null, source_offset: null },
        },
      },
      payloads: [],
    };
    const paused: Observation = {
      ...machine,
      sequence: '3',
      watchpoints: [point],
      status: {
        type: 'watchpoint',
        data: {
          watchpoint: point,
          address: point.address,
          pc: '0x0000000000001000',
          access: 'write',
        },
      },
    };
    const pending = Promise.withResolvers<Result>();
    const execute = vi
      .fn<(action: SessionAction) => Promise<Result>>(() => {
        throw new Error('Unexpected execution request');
      })
      .mockReturnValueOnce(pending.promise)
      .mockResolvedValueOnce(observed({ ...machine, sequence: '2', watchpoints: [point] }))
      .mockResolvedValueOnce(observed(paused))
      .mockResolvedValueOnce(failure)
      .mockResolvedValueOnce(observed({ ...paused, sequence: '4', watchpoints: [] }))
      .mockResolvedValueOnce(observed({ ...machine, sequence: '5', status: { type: 'running' } }));
    await render(Workbench, {
      portFactory: (): WorkerPort => ({
        connect: () => Promise.resolve(connection(machine)),
        request: (command) => {
          if (command.type === 'subscribe')
            return Promise.resolve({
              response: { id: '3', result: { type: 'subscribed', data: '3' } },
              payloads: [],
            });
          if (command.type !== 'execute') throw new Error('Unexpected command');
          expect(command.data.session).toEqual(machine.key);
          return execute(command.data.action);
        },
        acknowledge: () => Promise.resolve(),
        detach: () => {},
      }),
    });
    await page.getByRole('tab', { name: copy.breakpoint_view, exact: false }).click();
    const pane = page.getByRole('region', { name: copy.watchpoints, exact: true });
    const address = pane.getByRole('textbox', { name: copy.watch_address });
    const length = pane.getByRole('spinbutton', { name: copy.watch_length });
    const access = pane.getByRole('combobox', { name: copy.watch_access });
    const add = pane.getByRole('button', { name: copy.add_watchpoint });
    const remove = pane.getByRole('button', {
      name: remove_watchpoint({ address: point.address }, { locale }),
    });
    const run = page.getByRole('button', { name: copy.run, exact: true });
    await address.fill('2000');
    for (const invalid of ['', '0', '1.5', '65537']) {
      await length.fill(invalid);
      await expect.element(length).toBeInvalid();
      await add.click();
    }
    await settled();
    expect(execute).not.toHaveBeenCalled();
    await length.fill('8');
    await access.selectOptions('read_write');
    await length.click();
    await userEvent.keyboard('{Enter}');
    await expect
      .poll(() => execute)
      .toHaveBeenCalledExactlyOnceWith({ type: 'watchpoints', data: [point] });
    await expect.element(add).toBeDisabled();
    await userEvent.keyboard('{Enter}');
    await settled();
    expect(execute).toHaveBeenCalledTimes(1);
    await expect.element(remove).not.toBeInTheDocument();
    pending.resolve(failure);
    await expect.element(page.getByRole('alert')).toBeVisible();
    await expect.element(add).toBeEnabled();
    await expect.element(remove).not.toBeInTheDocument();
    await add.click();
    await expect.element(remove).toBeVisible();
    await run.click();
    await settled();
    await expect.element(run).toBeEnabled();
    await remove.click();
    await expect.element(page.getByRole('alert')).toBeVisible();
    await expect.element(remove).toBeEnabled();
    await remove.click();
    await expect.element(remove).not.toBeInTheDocument();
    await run.click();
    await expect.element(add).toBeDisabled();
    await expect.element(address).toBeDisabled();
    await expect.element(length).toBeDisabled();
    await expect.element(access).toBeDisabled();
    expect(execute.mock.calls.map(([action]) => action)).toEqual([
      { type: 'watchpoints', data: [point] },
      { type: 'watchpoints', data: [point] },
      { type: 'run' },
      { type: 'watchpoints', data: [] },
      { type: 'watchpoints', data: [] },
      { type: 'run' },
    ]);
  },
);

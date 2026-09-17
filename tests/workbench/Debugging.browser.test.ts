import type { WorkerPort } from '$lib/desktop/worker';
import type { ExecutionTrace } from '$lib/protocol/generated/ExecutionTrace';
import type { SessionAction } from '$lib/protocol/generated/SessionAction';
import { createWorkbench } from '$lib/workbench/controller.svelte';
import Trace from '$lib/workbench/machine/Trace.svelte';
import Workbench from '$lib/workbench/Workbench.svelte';
import { settled } from 'svelte';
import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page, userEvent } from 'vitest/browser';

import en from '../../messages/en.json';
import zh from '../../messages/zh-CN.json';
import { assembled, connection, observation } from '../fixtures/protocol';
import { scratch } from '../fixtures/scratch';

import '$lib/styles/theme.css';

beforeEach(() => {
  localStorage.clear();
});
afterEach(() => {
  localStorage.clear();
});

test.each(['en', 'zh-CN'] as const)(
  '%s source targets, stepping and trace controls use the loaded session',
  async (locale) => {
    const copy = locale === 'en' ? en : zh;
    localStorage.setItem('PARAGLIDE_LOCALE', locale);
    localStorage.setItem(
      'oplab.scratch.v1',
      JSON.stringify({ ...scratch, source: 'again: jmp again\ndone: nop' }),
    );
    const machine = observation('1');
    const pc = '0x0000000000001000';
    const trace: ExecutionTrace = { key: machine.key, enabled: false, entries: [], discarded: '0' };
    const actions: SessionAction[] = [];
    let sequence = 1;
    let instructions = 0;
    let traceReads = 0;
    await render(Workbench, {
      portFactory: (): WorkerPort => ({
        connect: () => Promise.resolve(connection()),
        request: (command) => {
          if (command.type === 'assemble') {
            const result = assembled(command.data.identity);
            if (result.response.result.type !== 'assembled') throw new Error('Invalid fixture');
            result.response.result.data.source_map.locations = [{ address: pc, line: 1 }];
            return Promise.resolve(result);
          }
          if (command.type === 'load')
            return Promise.resolve({
              response: { id: '2', result: { type: 'observed', data: machine } },
              payloads: [],
            });
          if (command.type === 'subscribe')
            return Promise.resolve({
              response: { id: '3', result: { type: 'subscribed', data: '3' } },
              payloads: [],
            });
          if (command.type === 'execute') {
            const action = command.data.action;
            expect(command.data.session).toEqual(machine.key);
            if (action.type === 'read_trace') {
              traceReads++;
              return Promise.resolve({
                response: {
                  id: '4',
                  result: { type: 'trace', data: { ...trace, entries: [...trace.entries] } },
                },
                payloads: [],
              });
            }
            actions.push(action);
            if (action.type === 'step' || action.type === 'step_over') {
              instructions++;
              if (trace.enabled) trace.entries = [{ instruction: String(instructions), pc }];
            } else if (action.type === 'record_trace') trace.enabled = action.data;
            else if (action.type === 'clear_trace') trace.entries = [];
            else if (action.type !== 'run_until')
              throw new Error(`Unexpected action ${action.type}`);
            return Promise.resolve({
              response: {
                id: '5',
                result: {
                  type: 'observed',
                  data: {
                    ...machine,
                    sequence: String(++sequence),
                    instructions: String(instructions),
                    status:
                      action.type === 'run_until'
                        ? { type: 'target', data: action.data.addresses[0] ?? pc }
                        : { type: 'stepped' },
                  },
                },
              },
              payloads: [],
            });
          }
          return Promise.reject(new Error(`Unexpected ${command.type}`));
        },
        acknowledge: () => Promise.resolve(),
        detach: () => {},
      }),
    });
    const editor = page.getByRole('textbox', { name: copy.editor_label });
    await expect.element(editor).toBeVisible();
    await page.getByRole('button', { name: copy.assemble, exact: true }).click();
    await page.getByRole('button', { name: copy.load_artifact, exact: true }).click();
    await page.getByRole('button', { name: copy.run_to_cursor, exact: true }).click();
    await expect
      .poll(() => actions.at(-1))
      .toEqual({ type: 'run_until', data: { addresses: [pc] } });
    await editor.click();
    await userEvent.keyboard('{F10}');
    await expect.poll(() => actions.at(-1)).toEqual({ type: 'step_over' });
    await expect.element(page.getByRole('button', { name: copy.step, exact: true })).toBeEnabled();
    await userEvent.keyboard('{F11}');
    await expect.poll(() => actions.at(-1)).toEqual({ type: 'step' });
    await settled();
    expect(traceReads).toBe(0);
    await page.getByRole('tab', { name: copy.trace, exact: true }).click();
    const record = page.getByRole('button', { name: copy.trace_record });
    await expect.element(record).toBeEnabled();
    const readsBeforeFocus = traceReads;
    const focus = page.getByRole('button', { name: copy.focus_editor, exact: true });
    await focus.click();
    await editor.click();
    await userEvent.keyboard('{F11}');
    await expect.element(page.getByRole('button', { name: copy.step, exact: true })).toBeEnabled();
    await settled();
    expect(traceReads).toBe(readsBeforeFocus);
    await focus.click();
    await expect.poll(() => traceReads).toBe(readsBeforeFocus + 1);
    await record.click();
    await expect.element(record).toHaveAttribute('aria-pressed', 'true');
    await settled();
    await expect.element(page.getByRole('cell', { name: pc, exact: true })).not.toBeInTheDocument();
    await editor.click();
    await userEvent.keyboard('{F11}');
    await expect.element(page.getByRole('cell', { name: pc, exact: true })).toBeVisible();
    await page.getByRole('button', { name: copy.trace_clear, exact: true }).click();
    await expect.element(page.getByRole('cell', { name: pc, exact: true })).not.toBeInTheDocument();
    await page
      .getByRole('tabpanel', { name: copy.trace })
      .getByRole('textbox', { name: copy.address })
      .fill('0x1004');
    await page.getByRole('button', { name: copy.run_to_address, exact: true }).click();
    await expect
      .poll(() => actions.at(-1))
      .toEqual({ type: 'run_until', data: { addresses: ['0x0000000000001004'] } });
  },
);

test.each([
  { replacement: 'reset', outcome: 'success' },
  { replacement: 'reset', outcome: 'failure' },
  { replacement: 'reconnect', outcome: 'success' },
  { replacement: 'reconnect', outcome: 'failure' },
  { replacement: 'refresh', outcome: 'success' },
  { replacement: 'refresh', outcome: 'failure' },
] as const)('$replacement ignores an obsolete trace $outcome', async ({ replacement, outcome }) => {
  const before = observation('1');
  const after =
    replacement === 'reset'
      ? { ...before, key: { ...before.key, generation: '1' }, sequence: '2' }
      : before;
  const delayed = Promise.withResolvers<Awaited<ReturnType<WorkerPort['request']>>>();
  const latest: ExecutionTrace = {
    key: after.key,
    enabled: false,
    entries: [],
    discarded: '0',
  };
  let reads = 0;
  let connections = 0;
  const work = createWorkbench(() => ({
    connect: () => Promise.resolve({ ...connection(before), connection: String(++connections) }),
    request: (command) => {
      if (command.type === 'subscribe')
        return Promise.resolve({
          response: { id: '3', result: { type: 'subscribed', data: '3' } },
          payloads: [],
        });
      if (command.type === 'execute' && command.data.action.type === 'read_trace') {
        if (++reads === 1) return delayed.promise;
        return Promise.resolve({
          response: { id: '5', result: { type: 'trace', data: latest } },
          payloads: [],
        });
      }
      if (command.type === 'execute' && command.data.action.type === 'reset')
        return Promise.resolve({
          response: { id: '4', result: { type: 'observed', data: after } },
          payloads: [],
        });
      return Promise.reject(new Error('Unexpected request'));
    },
    acknowledge: () => Promise.resolve(),
    detach: () => {},
  }));
  try {
    await work.connect();
    const request = work.readTrace();
    if (replacement === 'reset') await work.execute({ type: 'reset' });
    if (replacement === 'reconnect') await work.connect(true);
    if (replacement !== 'refresh') expect(work.trace).toBeNull();
    if (replacement === 'refresh') await work.readTrace();
    if (outcome === 'failure') delayed.reject(new Error('Obsolete request failed'));
    else
      delayed.resolve({
        response: {
          id: '2',
          result: {
            type: 'trace',
            data: {
              key: before.key,
              enabled: true,
              entries: [{ instruction: '1', pc: '0x0000000000001000' }],
              discarded: '0',
            },
          },
        },
        payloads: [],
      });
    await request;
    expect(work.trace).toEqual(replacement === 'refresh' ? latest : null);
    expect(work.snapshot?.observation.key).toEqual(after.key);
    expect(work.problem).toBeNull();
  } finally {
    work.dispose();
  }
});

test('trace refreshes for consecutive steps without polling unchanged observations', async () => {
  const before = { ...observation('1'), status: { type: 'stepped' as const }, instructions: '1' };
  const refresh = vi.fn(() => Promise.resolve());
  const record = vi.fn<(enabled: boolean) => void>();
  const view = await render(Trace, {
    trace: { key: before.key, enabled: false, entries: [], discarded: '0' },
    observation: before,
    connected: true,
    busy: false,
    locale: 'en',
    sourceIndex: { lines: new Map(), addresses: new Map() },
    onrefresh: refresh,
    onrecord: record,
    onclear: () => {},
    onrun: () => Promise.resolve(),
    onsource: () => {},
  });
  await expect.poll(() => refresh.mock.calls.length).toBe(1);
  await view.rerender({
    observation: { ...before, status: { type: 'running' }, instructions: '2' },
  });
  expect(refresh).toHaveBeenCalledTimes(1);
  await view.rerender({ observation: { ...before, sequence: '2', instructions: '2' } });
  await expect.poll(() => refresh.mock.calls.length).toBe(2);
  await view.rerender({ observation: { ...before, sequence: '3', instructions: '2' } });
  expect(refresh).toHaveBeenCalledTimes(2);
  const toggle = page.getByRole('button', { name: en.trace_record });
  await toggle.click();
  expect(record).toHaveBeenCalledWith(true);
  await expect.element(toggle).toHaveAttribute('aria-pressed', 'false');
});

import type { WorkerPort } from '$lib/desktop/worker';
import { breakpoint_at, remove_breakpoint } from '$lib/paraglide/messages.js';
import type { Command } from '$lib/protocol/generated/Command';
import type { Reply } from '$lib/protocol/generated/Reply';
import type { StreamEvent } from '$lib/protocol/generated/StreamEvent';
import Workbench from '$lib/workbench/Workbench.svelte';
import { settled } from 'svelte';
import { beforeEach, afterEach, expect, test } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';

import en from '../../messages/en.json';
import zh from '../../messages/zh-CN.json';
import { connection, observation } from '../fixtures/protocol';
import { scratch } from '../fixtures/scratch';

import '$lib/styles/theme.css';

beforeEach(() => {
  localStorage.clear();
  localStorage.setItem('oplab.scratch.v1', JSON.stringify(scratch));
});
afterEach(() => {
  localStorage.clear();
});

test.each([
  { locale: 'en', target: 'x86_64', entry: '0x1002', completion: '0x1006' },
  { locale: 'zh-CN', target: 'aarch64', entry: '0x1004', completion: '0x1008' },
] as const)(
  '$target raw loading validates input, preserves a rejected replacement and captures edits in $locale',
  async ({ locale, target, entry, completion }) => {
    localStorage.setItem('PARAGLIDE_LOCALE', locale);
    const copy = locale === 'en' ? en : zh;
    const bytes = new Uint8Array([0, 255, 127, 128, 1, 2, 3, 4]);
    const initial = observation('1');
    initial.breakpoints = ['0x0000000000001010'];
    const loaded = observation('2', target);
    loaded.key.session = '3';
    const commands: Command[] = [];
    const load = Promise.withResolvers<Awaited<ReturnType<WorkerPort['request']>>>();
    const payloads: (Uint8Array | undefined)[] = [];
    let reject = true;
    await render(Workbench, {
      filePort: { open: () => Promise.resolve(bytes), save: () => Promise.resolve(true) },
      portFactory: (): WorkerPort => ({
        connect: () => Promise.resolve(connection(initial)),
        request: (command, image) => {
          commands.push(command);
          if (command.type === 'load') {
            payloads.push(image);
            return reject
              ? Promise.resolve({
                  response: {
                    id: '2',
                    result: {
                      type: 'error',
                      data: { code: 'invalid_input', address: null, source_offset: null },
                    },
                  },
                  payloads: [],
                })
              : load.promise;
          }
          if (command.type === 'subscribe')
            return Promise.resolve({
              response: { id: '3', result: { type: 'subscribed', data: '3' } },
              payloads: [],
            });
          throw new Error(`Unexpected ${command.type}`);
        },
        acknowledge: () => Promise.resolve(),
        detach: () => {},
      }),
    });
    const editor = page.getByRole('textbox', { name: copy.editor_label });
    await expect.element(editor).toBeVisible();
    const source = editor.element().textContent;
    await page.getByRole('button', { name: copy.files, exact: true }).click();
    await page.getByRole('menuitem', { name: copy.import_binary }).click();
    const pane = page.getByRole('tabpanel', { name: copy.raw_code });
    await expect.element(pane).toBeVisible();
    expect(commands.filter((command) => command.type !== 'subscribe')).toEqual([]);
    await pane.getByRole('combobox', { name: copy.target, exact: true }).selectOptions(target);
    const entryInput = pane.getByRole('textbox', { name: copy.entry_address });
    await entryInput.fill('invalid');
    await pane.getByRole('textbox', { name: copy.completion_address }).fill(completion);
    await pane.getByRole('button', { name: copy.load_raw }).click();
    await expect.element(page.getByRole('alert')).toBeVisible();
    expect(payloads).toEqual([]);
    await entryInput.fill(entry);
    await pane.getByRole('button', { name: copy.load_raw }).click();
    await expect.poll(() => payloads.length).toBe(1);
    await expect.element(pane.getByRole('button', { name: copy.load_raw })).toBeEnabled();
    await expect.element(page.getByRole('alert')).toBeVisible();
    await expect
      .element(
        page.getByRole('button', {
          name: remove_breakpoint({ address: '0x0000000000001010' }, { locale }),
        }),
      )
      .toBeVisible();
    await expect.element(page.getByRole('button', { name: copy.step, exact: true })).toBeEnabled();
    expect(commands.find((command) => command.type === 'load')).toMatchObject({
      type: 'load',
      data: {
        image: {
          type: 'raw',
          data: { base: '0x0000000000001000', entry: `0x${entry.slice(2).padStart(16, '0')}` },
        },
        target,
        completion: `0x${completion.slice(2).padStart(16, '0')}`,
        image_bytes: bytes.length,
        initial: { registers: [], mappings: [] },
        replace: initial.key,
      },
    });
    reject = false;
    await pane.getByRole('button', { name: copy.load_raw }).click();
    await expect.poll(() => payloads.length).toBe(2);
    await expect.element(pane.getByRole('button', { name: copy.load_raw })).toBeDisabled();
    await pane.getByRole('textbox', { name: copy.raw_base }).fill('0x2000');
    load.resolve({
      response: { id: '2', result: { type: 'observed', data: loaded } },
      payloads: [],
    });
    await expect.element(page.getByRole('button', { name: copy.step, exact: true })).toBeEnabled();
    await expect.element(pane.getByRole('textbox', { name: copy.raw_base })).toHaveValue('0x2000');
    await expect.element(page.getByText(copy.raw_changed, { exact: true })).toBeVisible();
    await expect.element(editor).toHaveTextContent(source);
    expect(payloads).toEqual([bytes, bytes]);
    const loads = commands.filter((command) => command.type === 'load');
    expect(loads[1]).toEqual(loads[0]);
    expect(commands.some((command) => command.type === 'execute')).toBe(false);
  },
);

test('controls preserve captures; changed bytes and reset invalidate decoded instructions', async () => {
  localStorage.setItem('PARAGLIDE_LOCALE', 'en');
  const machine = observation('1');
  machine.breakpoints = ['0x0000000000001000'];
  const memory = new Uint8Array([0x90, 0x90]);
  let receive: ((stream: { event: StreamEvent; memory: Uint8Array | null }) => void) | undefined;
  const removed = { ...machine, sequence: '3', breakpoints: [] };
  const added = { ...machine, sequence: '5', breakpoints: ['0x0000000000001001'] };
  const reset = { ...added, sequence: '7', key: { ...machine.key, generation: '1' } };
  const rejected: Reply = {
    type: 'error',
    data: { code: 'invalid_state', address: null, source_offset: null },
  };
  const replies: Reply[] = [
    rejected,
    { type: 'observed', data: removed },
    { type: 'observed', data: added },
    rejected,
    { type: 'observed', data: reset },
  ];
  const controls: Command[] = [];
  const subscriptions: Command[] = [];
  await render(Workbench, {
    portFactory: (onstream: NonNullable<typeof receive>): WorkerPort => {
      receive = onstream;
      return {
        connect: () => Promise.resolve(connection(structuredClone(machine))),
        request: (command) => {
          if (command.type === 'subscribe') {
            subscriptions.push(command);
            return Promise.resolve({
              response: { id: '3', result: { type: 'subscribed', data: '3' } },
              payloads: [],
            });
          }
          if (command.type === 'decode') {
            expect(command.data.bytes).toEqual([...memory]);
            return Promise.resolve({
              response: {
                id: '4',
                result: {
                  type: 'decoded',
                  data: [
                    { address: '0x0000000000001000', bytes: [0x90], text: 'nop' },
                    { address: '0x0000000000001001', bytes: [0x90], text: 'nop' },
                  ],
                },
              },
              payloads: [],
            });
          }
          if (command.type !== 'execute') throw new Error(`Unexpected ${command.type}`);
          controls.push(command);
          const result = replies.shift();
          if (result === undefined) throw new Error('Unexpected execution request');
          return Promise.resolve({ response: { id: '5', result }, payloads: [] });
        },
        acknowledge: () => Promise.resolve(),
        detach: () => {},
      };
    },
  });
  const add = page.getByRole('button', { name: en.add_breakpoint });
  await expect.element(add).toBeEnabled();
  await expect.poll(() => subscriptions.length).toBe(1);
  const length = page.getByRole('spinbutton', { name: en.window_size });
  const inspectPC = page.getByRole('button', { name: en.inspect_pc });
  for (const invalid of ['4097', '']) {
    await length.fill(invalid);
    await inspectPC.click();
    await settled();
    expect(subscriptions).toHaveLength(1);
  }
  await length.fill('2');
  await inspectPC.click();
  await expect.poll(() => subscriptions.length).toBe(2);
  expect(subscriptions[1]).toEqual({
    type: 'subscribe',
    data: { session: machine.key, memory: { address: '0x0000000000001000', length: 2 } },
  });
  const captured = structuredClone(machine);
  captured.sequence = '2';
  captured.memory = { address: '0x0000000000001000', length: 2 };
  if (receive === undefined) throw new Error('Missing stream receiver');
  receive({ event: { subscription: '3', update: { type: 'full', data: captured } }, memory });
  await page.getByRole('tab', { name: en.instructions, exact: true }).click();
  await page.getByRole('button', { name: en.disassemble, exact: true }).click();
  const rowBreakpoint = page.getByRole('button', {
    name: breakpoint_at({ address: '0x0000000000001000' }, { locale: 'en' }),
  });
  await expect.element(rowBreakpoint).toHaveAttribute('aria-pressed', 'true');
  await rowBreakpoint.click();
  await expect.element(page.getByRole('alert')).toBeVisible();
  await expect.element(rowBreakpoint).toHaveAttribute('aria-pressed', 'true');
  await rowBreakpoint.click();
  await expect.element(rowBreakpoint).toHaveAttribute('aria-pressed', 'false');
  await expect.element(page.getByRole('alert')).not.toBeInTheDocument();
  const next = structuredClone(captured);
  next.sequence = '4';
  next.breakpoints = [];
  receive({
    event: { subscription: '3', update: { type: 'full', data: next } },
    memory: memory.slice(),
  });
  await settled();
  await expect.element(rowBreakpoint).toBeVisible();
  await page.getByRole('textbox', { name: en.breakpoint_address }).fill('1001');
  await add.click();
  await expect
    .element(
      page.getByRole('button', {
        name: remove_breakpoint({ address: '0x0000000000001001' }, { locale: 'en' }),
      }),
    )
    .toBeVisible();
  await expect
    .element(page.getByRole('table', { name: en.instructions, exact: true }))
    .toBeVisible();
  await page.getByRole('tab', { name: en.memory, exact: true }).click();
  const pane = page.getByRole('tabpanel', { name: en.memory });
  await expect.element(pane.getByRole('cell', { name: '90 90', exact: true })).toBeVisible();

  await page.getByRole('button', { name: en.reset, exact: true }).click();
  await expect.element(page.getByRole('alert')).toBeVisible();
  await expect.element(pane.getByRole('cell', { name: '90 90', exact: true })).toBeVisible();
  receive({
    event: {
      subscription: '3',
      update: { type: 'full', data: { ...added, sequence: '6', memory: captured.memory } },
    },
    memory: new Uint8Array([0x90, 0xc3]),
  });
  await settled();
  await expect.element(pane.getByRole('cell', { name: '90 c3', exact: true })).toBeVisible();
  await page.getByRole('tab', { name: en.instructions, exact: true }).click();
  await expect
    .element(page.getByRole('table', { name: en.instructions, exact: true }))
    .not.toBeInTheDocument();
  await page.getByRole('tab', { name: en.memory, exact: true }).click();
  await page.getByRole('button', { name: en.reset, exact: true }).click();
  await settled();
  await expect.element(page.getByRole('alert')).not.toBeInTheDocument();
  await expect.element(pane.getByRole('table')).not.toBeInTheDocument();
  expect(controls).toEqual([
    ...Array.from({ length: 2 }, () => ({
      type: 'execute',
      data: {
        session: machine.key,
        action: { type: 'breakpoint', data: { address: '0x0000000000001000', enabled: false } },
      },
    })),
    {
      type: 'execute',
      data: {
        session: machine.key,
        action: { type: 'breakpoint', data: { address: '0x0000000000001001', enabled: true } },
      },
    },
    ...Array.from({ length: 2 }, () => ({
      type: 'execute',
      data: { session: machine.key, action: { type: 'reset' } },
    })),
  ]);
  expect(replies).toEqual([]);
});

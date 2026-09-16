import type { WorkerPort } from '$lib/desktop/worker';
import { breakpoint_at, remove_breakpoint, set_pc_at } from '$lib/paraglide/messages.js';
import type { Command } from '$lib/protocol/generated/Command';
import type { Reply } from '$lib/protocol/generated/Reply';
import type { StreamEvent } from '$lib/protocol/generated/StreamEvent';
import Workbench from '$lib/workbench/Workbench.svelte';
import { settled } from 'svelte';
import { beforeEach, afterEach, expect, test } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page, userEvent } from 'vitest/browser';

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
  await page.getByRole('textbox', { name: en.address, exact: true }).fill('invalid');
  await page.getByRole('button', { name: en.inspect_memory }).click();
  await expect.element(page.getByRole('alert')).toBeVisible();
  expect(subscriptions).toHaveLength(1);
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
  await expect.element(page.getByRole('alert')).not.toBeInTheDocument();
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

test.each([
  { locale: 'en', target: 'x86_64', name: 'rsp' },
  { locale: 'zh-CN', target: 'aarch64', name: 'sp' },
] as const)(
  'live editing in $locale waits for authoritative replies and refreshes patched memory',
  async ({ locale, target, name }) => {
    localStorage.setItem('PARAGLIDE_LOCALE', locale);
    const copy = locale === 'en' ? en : zh;
    let machine = observation('1', target);
    const writes: { command: Command; bytes: Uint8Array | undefined }[] = [];
    const registerReply = Promise.withResolvers<Awaited<ReturnType<WorkerPort['request']>>>();
    let rejectPatch = true;
    let patched: Uint8Array | undefined;
    await render(Workbench, {
      portFactory: (
        receive: (stream: { event: StreamEvent; memory: Uint8Array | null }) => void,
      ): WorkerPort => ({
        connect: () => Promise.resolve(connection(machine)),
        request: (command, bytes) => {
          if (command.type === 'subscribe') {
            if (patched !== undefined)
              receive({
                event: {
                  subscription: '3',
                  update: {
                    type: 'full',
                    data: { ...machine, sequence: '6', memory: command.data.memory },
                  },
                },
                memory: patched,
              });
            return Promise.resolve({
              response: { id: '3', result: { type: 'subscribed', data: '3' } },
              payloads: [],
            });
          }
          writes.push({ command, bytes });
          if (command.type !== 'execute') throw new Error('Unexpected command');
          if (command.data.action.type === 'write_register') return registerReply.promise;
          if (command.data.action.type !== 'write_memory') throw new Error('Unexpected action');
          if (rejectPatch) {
            rejectPatch = false;
            return Promise.resolve({
              response: {
                id: '4',
                result: {
                  type: 'error',
                  data: { code: 'invalid_input', address: null, source_offset: null },
                },
              },
              payloads: [],
            });
          }
          patched = bytes;
          return Promise.resolve({
            response: {
              id: '5',
              result: {
                type: 'observed',
                data: { ...machine, sequence: '5' },
              },
            },
            payloads: [],
          });
        },
        acknowledge: () => Promise.resolve(),
        detach: () => {},
      }),
    });
    const edit = page.getByRole('button', { name: copy.edit_register, exact: true });
    await expect.element(edit).toBeEnabled();
    await edit.click();
    const register = page.getByRole('combobox', { name: copy.register_name });
    await register.selectOptions(name);
    const value = page.getByRole('textbox', { name: copy.register_value });
    const write = page.getByRole('button', { name: copy.write_register, exact: true });
    await value.fill('18446744073709551616');
    await userEvent.keyboard('{Enter}');
    await expect.element(page.getByRole('alert')).toBeVisible();
    expect(writes).toEqual([]);
    await value.fill('0xffffffffffffffff');
    await userEvent.keyboard('{Enter}');
    await expect.element(write).toBeDisabled();
    await expect
      .element(page.getByText('ffffffffffffffff', { exact: true }))
      .not.toBeInTheDocument();
    expect(writes).toEqual([
      {
        command: {
          type: 'execute',
          data: {
            session: machine.key,
            action: { type: 'write_register', data: { name, value: '18446744073709551615' } },
          },
        },
        bytes: undefined,
      },
    ]);
    const next = structuredClone(machine);
    next.sequence = '4';
    if (next.registers?.type === 'x86_64') next.registers.data.gpr[4] = '18446744073709551615';
    if (next.registers?.type === 'aarch64') next.registers.data.sp = '18446744073709551615';
    machine = next;
    registerReply.resolve({
      response: { id: '4', result: { type: 'observed', data: next } },
      payloads: [],
    });
    await expect.element(write).toBeEnabled();
    await expect.element(register).toHaveValue(name);
    await page.getByRole('button', { name: copy.close, exact: true }).click();
    await expect.element(page.getByText('ffffffffffffffff', { exact: true })).toBeVisible();
    await page.getByRole('button', { name: copy.write_memory, exact: true }).click();
    await page.getByRole('textbox', { name: copy.patch_address }).fill('1000');
    const hex = page.getByRole('textbox', { name: copy.patch_bytes });
    await hex.fill('a');
    const apply = page
      .getByRole('form', { name: copy.write_memory })
      .getByRole('button', { name: copy.write_memory, exact: true });
    await apply.click();
    await expect.element(page.getByRole('alert')).toBeVisible();
    expect(writes).toHaveLength(1);
    await hex.fill('2a 00 ff');
    await apply.click();
    await expect.poll(() => writes.length).toBe(2);
    await expect.element(apply).toBeEnabled();
    await expect.element(page.getByRole('alert')).toBeVisible();
    await expect.element(hex).toHaveValue('2a 00 ff');
    // Retry only after a new user action; a rejected mutation must not replay itself.
    await settled();
    expect(writes).toHaveLength(2);
    await apply.click();
    await expect.element(page.getByRole('alert')).not.toBeInTheDocument();
    await page.getByRole('button', { name: copy.close, exact: true }).click();
    await expect.element(page.getByRole('cell', { name: '2a 00 ff', exact: true })).toBeVisible();
    expect(writes.slice(1)).toEqual(
      Array.from({ length: 2 }, () => ({
        command: {
          type: 'execute',
          data: {
            session: machine.key,
            action: { type: 'write_memory', data: { address: '0x0000000000001000', length: 3 } },
          },
        },
        bytes: new Uint8Array([42, 0, 255]),
      })),
    );
  },
);

test.each([
  { locale: 'en', target: 'x86_64', pc: 'rip', bytes: [0x90], address: '0x0000000000001001' },
  {
    locale: 'zh-CN',
    target: 'aarch64',
    pc: 'pc',
    bytes: [0x1f, 0x20, 0x03, 0xd5],
    address: '0x0000000000001004',
  },
] as const)(
  'setting the next decoded instruction waits for the $target machine in $locale',
  async ({ locale, target, pc, bytes, address }) => {
    localStorage.setItem('PARAGLIDE_LOCALE', locale);
    const copy = locale === 'en' ? en : zh;
    const machine = observation('1', target);
    const commands: Command[] = [];
    const pending = Promise.withResolvers<Awaited<ReturnType<WorkerPort['request']>>>();
    const accepted = Promise.withResolvers<Awaited<ReturnType<WorkerPort['request']>>>();
    let receiveCapture:
      | ((stream: { event: StreamEvent; memory: Uint8Array | null }) => void)
      | undefined;
    await render(Workbench, {
      portFactory: (
        receive: (stream: { event: StreamEvent; memory: Uint8Array | null }) => void,
      ): WorkerPort => ({
        connect: () => {
          receiveCapture = receive;
          return Promise.resolve(connection(machine));
        },
        request: (command) => {
          if (command.type === 'subscribe') {
            receive({
              event: {
                subscription: '1',
                update: {
                  type: 'full',
                  data: {
                    ...machine,
                    sequence: '2',
                    memory: { address: '0x0000000000001000', length: bytes.length * 2 },
                  },
                },
              },
              memory: new Uint8Array([...bytes, ...bytes]),
            });
            return Promise.resolve({
              response: { id: '1', result: { type: 'subscribed', data: '1' } },
              payloads: [],
            });
          }
          if (command.type === 'decode')
            return Promise.resolve({
              response: {
                id: '2',
                result: {
                  type: 'decoded',
                  data: [{ address, bytes: [...bytes], text: 'nop' }],
                },
              },
              payloads: [],
            });
          if (command.type !== 'execute') throw new Error(`Unexpected ${command.type}`);
          commands.push(command);
          return commands.length === 1 ? pending.promise : accepted.promise;
        },
        acknowledge: () => Promise.resolve(),
        detach: () => {},
      }),
    });
    await page.getByRole('tab', { name: copy.instructions, exact: true }).click();
    await page.getByRole('button', { name: copy.disassemble, exact: true }).click();
    const move = page.getByRole('button', {
      name: set_pc_at({ address }, { locale }),
    });
    const row = page.getByRole('row').filter({ has: move });
    await move.click();
    await expect.element(move).toBeDisabled();
    await expect.element(row).not.toHaveAttribute('aria-current', 'step');
    expect(commands).toEqual([
      {
        type: 'execute',
        data: {
          session: machine.key,
          action: { type: 'write_register', data: { name: pc, value: BigInt(address).toString() } },
        },
      },
    ]);
    pending.resolve({
      response: {
        id: '3',
        result: {
          type: 'error',
          data: { code: 'stale_session', address: null, source_offset: null },
        },
      },
      payloads: [],
    });
    await expect.element(page.getByRole('alert')).toBeVisible();
    await expect.element(move).toBeEnabled();
    await expect
      .element(page.getByRole('table', { name: copy.instructions, exact: true }))
      .toBeVisible();
    await expect.element(row).not.toHaveAttribute('aria-current', 'step');
    expect(commands).toHaveLength(1);

    await move.click();
    await expect.element(move).toBeDisabled();
    await expect.element(row).not.toHaveAttribute('aria-current', 'step');
    const next = structuredClone(machine);
    next.sequence = '3';
    if (next.registers?.type === 'x86_64') next.registers.data.rip = address;
    if (next.registers?.type === 'aarch64') next.registers.data.pc = address;
    accepted.resolve({
      response: { id: '4', result: { type: 'observed', data: next } },
      payloads: [],
    });
    await expect.element(move).toBeEnabled();
    await expect.element(page.getByRole('alert')).not.toBeInTheDocument();
    await expect.element(page.getByText(address.slice(2), { exact: true })).toBeVisible();
    await expect.element(row).not.toHaveAttribute('aria-current', 'step');
    if (receiveCapture === undefined) throw new Error('Missing stream receiver');
    receiveCapture({
      event: {
        subscription: '1',
        update: {
          type: 'full',
          data: {
            ...next,
            sequence: '4',
            memory: { address: '0x0000000000001000', length: bytes.length * 2 },
          },
        },
      },
      memory: new Uint8Array([...bytes, ...bytes]),
    });
    await expect.element(row).toHaveAttribute('aria-current', 'step');
    expect(commands).toEqual([commands[0], commands[0]]);
  },
);

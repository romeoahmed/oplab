import type { WorkerPort } from '$lib/desktop/worker';
import {
  memory_region,
  remove_mapping,
  remove_register,
  show_diagnostic,
} from '$lib/paraglide/messages.js';
import type { BuildIdentity } from '$lib/protocol/generated/BuildIdentity';
import type { Command } from '$lib/protocol/generated/Command';
import type { FileFormat } from '$lib/protocol/generated/FileFormat';
import { createWorkbench } from '$lib/workbench/controller.svelte';
import Workbench from '$lib/workbench/Workbench.svelte';
import { afterEach, beforeEach, expect, onTestFinished, test, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page, userEvent } from 'vitest/browser';

import en from '../../messages/en.json';
import zh from '../../messages/zh-CN.json';
import { assembled, connection, observation, nopAnalysis } from '../fixtures/protocol';
import { scratch } from '../fixtures/scratch';

import '$lib/styles/theme.css';

beforeEach(() => {
  localStorage.clear();
  localStorage.setItem('PARAGLIDE_LOCALE', 'en');
  localStorage.setItem('oplab.scratch.v1', JSON.stringify(scratch));
});
afterEach(() => {
  localStorage.clear();
});

test('an oversized edit preserves the last recoverable draft', () => {
  const work = createWorkbench(() => null);
  onTestFinished(() => {
    work.dispose();
    localStorage.clear();
  });
  work.initialize();
  const oversized = '\u4e2d'.repeat(87382);
  work.setSource(oversized);
  // Closing flushes pending storage synchronously; it must not replace a valid draft.
  work.dispose();
  expect(work.source).toBe(oversized);
  expect(work.problem).toBe('storage');
  const recovered = createWorkbench(() => null);
  onTestFinished(() => {
    recovered.dispose();
    localStorage.clear();
  });
  recovered.initialize();
  expect(recovered.source).toBe(scratch.source);
});

test('switching locale retains edits and undo history', async () => {
  await render(Workbench);
  const editor = page.getByRole('textbox', { name: en.editor_label });
  await expect.element(editor).toBeVisible();
  const originalText = editor.element().textContent;
  await editor.click();
  await userEvent.keyboard('{ControlOrMeta>}a{/ControlOrMeta}mov rax, 99');
  await page.getByRole('combobox', { name: en.language }).selectOptions('zh-CN');
  const chinese = page.getByRole('textbox', { name: zh.editor_label });
  await expect.element(chinese).toHaveTextContent('mov rax, 99');
  expect(document.documentElement.lang).toBe('zh-CN');
  await chinese.click();
  await userEvent.keyboard('{ControlOrMeta>}z{/ControlOrMeta}');
  await expect.element(chinese).toHaveTextContent(originalText);
});

test('open editor panels retain their drafts and actions across language changes', async () => {
  await render(Workbench);
  await page.getByRole('textbox', { name: en.editor_label }).click();
  await userEvent.keyboard('{ControlOrMeta>}f{/ControlOrMeta}');
  await userEvent.keyboard('rax');
  await page.getByRole('button', { name: en.go_to_line, exact: true }).click();
  await page.getByRole('textbox', { name: `${en.go_to_line}:` }).fill('2:4');
  await page.getByRole('combobox', { name: en.language }).selectOptions('zh-CN');
  await expect
    .element(page.getByRole('textbox', { name: zh.find, exact: true }))
    .toHaveValue('rax');
  const line = page.getByRole('textbox', { name: `${zh.go_to_line}:` });
  await expect.element(line).toHaveValue('2:4');
  await page.getByRole('button', { name: zh.go, exact: true }).click();
  await expect.element(line).not.toBeInTheDocument();
  await expect.element(page.getByRole('textbox', { name: zh.editor_label })).toHaveFocus();
});

test.each(['artifact', 'diagnostic'] as const)(
  'a late build %s cannot replace edited source',
  async (outcome) => {
    const { promise, resolve } =
      Promise.withResolvers<Awaited<ReturnType<WorkerPort['request']>>>();
    let build: BuildIdentity | undefined;
    await render(Workbench, {
      portFactory: (): WorkerPort => ({
        connect: () => Promise.resolve(connection()),
        request: (command: Command) => {
          if (command.type !== 'assemble') throw new Error(`Unexpected ${command.type}`);
          build = command.data.identity;
          return promise;
        },
        acknowledge: () => Promise.resolve(),
        detach: () => {},
      }),
    });
    const assemble = page.getByRole('button', { name: en.assemble, exact: true });
    await expect.element(assemble).toBeEnabled();
    await assemble.click();
    const editor = page.getByRole('textbox', { name: en.editor_label });
    await editor.click();
    await userEvent.keyboard('{ControlOrMeta>}a{/ControlOrMeta}nop');
    if (build === undefined) throw new Error('Assembly request missing');
    resolve(
      outcome === 'artifact'
        ? assembled(build)
        : {
            response: {
              id: '1',
              result: {
                type: 'error',
                data: {
                  code: 'assembly',
                  address: null,
                  source_offset: 0,
                },
              },
            },
            payloads: [],
          },
    );
    await expect.element(assemble).toBeEnabled();
    await expect.element(page.getByRole('button', { name: en.load_artifact })).toBeDisabled();
    await expect.element(editor).toHaveTextContent('nop');
    await expect.element(page.getByRole('alert')).not.toBeInTheDocument();
  },
);

test('corrupt scratch recovery preserves the attached machine without replaying a mutation', async () => {
  localStorage.setItem('oplab.scratch.v1', '{');
  const commands: Command[] = [];
  await render(Workbench, {
    portFactory: (): WorkerPort => ({
      connect: () => Promise.resolve(connection(observation())),
      request: (command) => {
        commands.push(command);
        if (command.type !== 'subscribe') throw new Error(`Unexpected ${command.type}`);
        return Promise.resolve({
          response: { id: '3', result: { type: 'subscribed', data: '3' } },
          payloads: [],
        });
      },
      acknowledge: () => Promise.resolve(),
      detach: () => {},
    }),
  });
  await expect.element(page.getByRole('button', { name: en.step, exact: true })).toBeEnabled();
  await expect.element(page.getByRole('alert')).toMatchTextContent(en.error_storage);
  expect(commands.some((command) => command.type === 'subscribe')).toBe(true);
  expect(commands.every((command) => command.type === 'subscribe')).toBe(true);
});

test('appearance changes and focus mode preserve edits and undo history', async () => {
  await render(Workbench);
  const editor = page.getByRole('textbox', { name: en.editor_label });
  await expect.element(editor).toBeVisible();
  const original = editor.element().textContent;
  await editor.click();
  await userEvent.keyboard('{ControlOrMeta>}a{/ControlOrMeta}mov rax, 77');
  await page.getByRole('button', { name: en.appearance, exact: true }).click();
  await page.getByRole('combobox', { name: en.editor_font }).selectOptions('system');
  await page.getByRole('checkbox', { name: en.word_wrap }).click();
  const size = page.getByRole('slider', { name: en.font_size });
  await size.click();
  await userEvent.keyboard('{End}');
  await expect.element(size).toHaveValue(size.element().getAttribute('max'));
  const width = page.getByRole('slider', { name: en.inspector_width });
  const height = page.getByRole('slider', { name: en.memory_height });
  const [originalWidth, originalHeight, fontSize] = [width, height, size].map((slider) => {
    const input = slider.element();
    if (!(input instanceof HTMLInputElement)) throw new Error('Expected a range input');
    return input.value;
  });
  for (const slider of [width, height]) {
    await slider.click();
    await userEvent.keyboard('{End}');
  }
  await expect.element(width).not.toHaveValue(originalWidth);
  await expect.element(height).not.toHaveValue(originalHeight);
  const resetLayout = page.getByRole('button', { name: en.reset_layout });
  await resetLayout.click();
  await expect.element(width).toHaveValue(originalWidth);
  await expect.element(height).toHaveValue(originalHeight);
  await expect.element(resetLayout).toBeDisabled();
  await expect.element(page.getByRole('combobox', { name: en.editor_font })).toHaveValue('system');
  await expect.element(page.getByRole('checkbox', { name: en.word_wrap })).toBeChecked();
  await expect.element(size).toHaveValue(fontSize);
  await userEvent.keyboard('{Escape}');
  await expect
    .element(page.getByRole('button', { name: en.appearance, exact: true }))
    .toHaveFocus();
  await expect.element(editor).toHaveTextContent('mov rax, 77');
  await page.getByRole('button', { name: en.focus_editor, exact: true }).click();
  await expect
    .element(page.getByRole('complementary', { name: en.machine }))
    .not.toBeInTheDocument();
  await editor.click();
  await userEvent.keyboard('{ControlOrMeta>}z{/ControlOrMeta}');
  await expect.element(editor).toHaveTextContent(original);
});

test('architecture changes reconfigure completion and comment commands without changing source', async () => {
  await render(Workbench);
  const editor = page.getByRole('textbox', { name: en.editor_label });
  await page.getByRole('combobox', { name: en.target }).selectOptions('aarch64');
  await editor.click();
  await userEvent.keyboard('{ControlOrMeta>}a{/ControlOrMeta}mov x0, #42');
  await userEvent.keyboard('{Home}{ControlOrMeta>}/{/ControlOrMeta}');
  await expect.element(editor).toHaveTextContent('// mov x0, #42');
  await userEvent.keyboard('{ControlOrMeta>}/{/ControlOrMeta}');
  await expect.element(editor).toHaveTextContent('mov x0, #42');
  await userEvent.keyboard('{End}{Enter}mov x');
  await userEvent.keyboard('{Control>} {/Control}');
  await expect.element(page.getByRole('option', { name: 'x0', exact: true })).toBeVisible();
  await userEvent.keyboard('{Escape}');
  await userEvent.keyboard('{ControlOrMeta>}a{/ControlOrMeta}// x');
  await userEvent.keyboard('{Control>} {/Control}');
  await expect.element(page.getByRole('listbox')).not.toBeInTheDocument();
  await userEvent.keyboard('{ControlOrMeta>}a{/ControlOrMeta}.ascii "x');
  await userEvent.keyboard('{Control>} {/Control}');
  await expect.element(page.getByRole('listbox')).not.toBeInTheDocument();
});

test('a saved draft restores source and architecture when the workbench is reopened', async () => {
  const first = await render(Workbench);
  await page.getByRole('combobox', { name: en.target }).selectOptions('aarch64');
  await page.getByRole('textbox', { name: en.editor_label }).click();
  await userEvent.keyboard('{ControlOrMeta>}a{/ControlOrMeta}// \u4e2d\u6587{Enter}mov x0, #7');
  await first.unmount();
  await render(Workbench);
  await expect.element(page.getByRole('combobox', { name: en.target })).toHaveValue('aarch64');
  await expect
    .element(page.getByRole('textbox', { name: en.editor_label }))
    .toHaveTextContent('// \u4e2d\u6587mov x0, #7');
});

test('assembly, loading and execution stay separate; a rejected reset retains the machine', async () => {
  const commands: Command[] = [];
  const machine = observation();
  await render(Workbench, {
    portFactory: (): WorkerPort => ({
      connect: () => Promise.resolve(connection()),
      request: (command, image) => {
        commands.push(command);
        if (command.type === 'assemble') return Promise.resolve(assembled(command.data.identity));
        if (command.type === 'subscribe')
          return Promise.resolve({
            response: { id: '3', result: { type: 'subscribed', data: '3' } },
            payloads: [],
          });
        if (command.type === 'load') {
          expect(image).toEqual(new Uint8Array([2]));
          expect(command.data).toMatchObject({
            target: 'x86_64',
            completion: '0x0000000000001008',
            replace: null,
          });
          return Promise.resolve({
            response: { id: '2', result: { type: 'observed', data: structuredClone(machine) } },
            payloads: [],
          });
        }
        if (command.type === 'execute') {
          expect(command.data.session).toEqual(machine.key);
          if (command.data.action.type === 'reset')
            return Promise.resolve({
              response: {
                id: '5',
                result: {
                  type: 'error',
                  data: { code: 'invalid_input', address: null, source_offset: null },
                },
              },
              payloads: [],
            });
          if (command.data.action.type !== 'run') throw new Error('Unexpected execution action');
          machine.sequence = String(BigInt(machine.sequence) + 1n);
          machine.status = { type: 'terminated', data: 'completed' };
          return Promise.resolve({
            response: { id: '4', result: { type: 'observed', data: structuredClone(machine) } },
            payloads: [],
          });
        }
        throw new Error(`Unexpected ${command.type}`);
      },
      acknowledge: () => Promise.resolve(),
      detach: () => {},
    }),
  });
  await page.getByRole('button', { name: en.assemble, exact: true }).click();
  const load = page.getByRole('button', { name: en.load_artifact, exact: true });
  await expect.element(load).toBeEnabled();
  expect(commands.every((command) => command.type === 'assemble')).toBe(true);
  await load.click();
  const run = page.getByRole('button', { name: en.run, exact: true });
  await expect.element(run).toBeEnabled();
  expect(commands.some((command) => command.type === 'execute')).toBe(false);
  await run.click();
  await expect.element(run).toBeDisabled();
  const reset = page.getByRole('button', { name: en.reset, exact: true });
  await reset.click();
  await expect.element(page.getByRole('alert')).toBeVisible();
  await expect.element(reset).toBeEnabled();
  await expect
    .element(page.getByRole('complementary', { name: en.machine }))
    .toMatchTextContent(en.state_completed);
});

test('build diagnostics locate Unicode source, follow locale and expire on edits', async () => {
  await render(Workbench, {
    portFactory: (): WorkerPort => ({
      connect: () => Promise.resolve(connection()),
      request: (command) => {
        if (command.type !== 'assemble') throw new Error(`Unexpected ${command.type}`);
        const source = command.data.source;
        return Promise.resolve({
          response: {
            id: '1',
            result: {
              type: 'error',
              data: {
                code: 'assembly',
                address: null,
                source_offset: new TextEncoder().encode(source.slice(0, source.indexOf('invalid')))
                  .length,
              },
            },
          },
          payloads: [],
        });
      },
      acknowledge: () => Promise.resolve(),
      detach: () => {},
    }),
  });
  const editor = page.getByRole('textbox', { name: en.editor_label });
  await editor.click();
  await userEvent.keyboard('{ControlOrMeta>}a{/ControlOrMeta}// \u4e2d\u6587{Enter}  invalid');
  await page.getByRole('button', { name: en.assemble, exact: true }).click();
  await page
    .getByRole('button', { name: show_diagnostic({ line: '2', column: '3' }, { locale: 'en' }) })
    .click();
  await expect.element(editor).toHaveFocus();
  await userEvent.keyboard('X');
  await expect.element(editor).toMatchTextContent('Xinvalid');
  await expect.element(page.getByRole('alert')).not.toBeInTheDocument();
  await userEvent.keyboard('{ControlOrMeta>}z{/ControlOrMeta}');
  await page.getByRole('button', { name: en.assemble, exact: true }).click();
  await page.getByRole('combobox', { name: en.language }).selectOptions('zh-CN');
  await page
    .getByRole('button', { name: show_diagnostic({ line: '2', column: '3' }, { locale: 'zh-CN' }) })
    .click();
  await expect.element(page.getByRole('textbox', { name: zh.editor_label })).toHaveFocus();
  await page.getByRole('combobox', { name: zh.target, exact: true }).selectOptions('aarch64');
  await expect.element(page.getByRole('alert')).not.toBeInTheDocument();
});

test('imported source reaches assembly unchanged and binary inspection never loads a machine', async () => {
  const bytes = new Uint8Array([0x90, 0xc3]);
  const source = '\uFEFF# \u4e2d\u6587 \u{1f600}\r\n' + '  nop\r\n'.repeat(512);
  const saved: { format: string; bytes: Uint8Array }[] = [];
  await render(Workbench, {
    filePort: {
      open: (format) =>
        Promise.resolve(format === 'source' ? new TextEncoder().encode(source) : bytes),
      save: (format: FileFormat, _title: string, contents: Uint8Array) => {
        saved.push({ format, bytes: contents });
        return Promise.resolve(true);
      },
    },
    portFactory: (): WorkerPort => ({
      connect: () => Promise.resolve(connection()),
      request: (command) => {
        if (command.type === 'assemble') {
          expect(command.data.source).toBe(source);
          return Promise.resolve(assembled(command.data.identity));
        }
        if (command.type === 'analyze') {
          expect(command.data).toEqual({
            target: 'x86_64',
            base: '0x0000000000001000',
            bytes: [0x90],
          });
          return Promise.resolve({
            response: { id: '3', result: { type: 'analyzed', data: nopAnalysis } },
            payloads: [],
          });
        }
        if (command.type !== 'decode') throw new Error(`Unexpected ${command.type}`);
        expect(command.data).toMatchObject({
          target: 'x86_64',
          base: '0x0000000000001000',
          bytes: [0x90, 0xc3],
        });
        return Promise.resolve({
          response: {
            id: '2',
            result: {
              type: 'decoded',
              data: [
                { address: '0x0000000000001000', bytes: [0x90], text: 'nop' },
                { address: '0x0000000000001001', bytes: [0xc3], text: 'ret' },
              ],
            },
          },
          payloads: [],
        });
      },
      acknowledge: () => Promise.resolve(),
      detach: () => {},
    }),
  });
  const files = page.getByRole('button', { name: en.files, exact: true });
  await files.click();
  await page.getByRole('menuitem', { name: en.import_source }).click();
  await expect.element(page.getByText('UTF-8 BOM · CRLF', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: en.assemble, exact: true }).click();
  await files.click();
  await page.getByRole('menuitem', { name: en.export_source }).click();
  await expect.poll(() => saved.length).toBe(1);
  expect(saved[0]).toEqual({ format: 'source', bytes: new TextEncoder().encode(source) });
  await files.click();
  await page.getByRole('menuitem', { name: en.import_binary }).click();
  await page.getByRole('tab', { name: en.instructions, exact: true }).click();
  await page.getByRole('button', { name: en.disassemble, exact: true }).click();
  const instructions = page.getByRole('table', { name: en.instructions, exact: true });
  await expect.element(instructions).toMatchTextContent('0x0000000000001001');
  await expect.element(instructions).toMatchTextContent('ret');
  await files.click();
  await page.getByRole('menuitem', { name: en.export_binary }).click();
  await expect.poll(() => saved.length).toBe(2);
  expect(saved[1]).toEqual({ format: 'binary', bytes });
  await page.getByRole('tab', { name: en.memory, exact: true }).click();
  await page.getByRole('tab', { name: en.instructions, exact: true }).click();
  await expect.element(instructions).toMatchTextContent('ret');
  await page.getByRole('button', { name: 'nop', exact: true }).click();
  await expect
    .element(page.getByRole('region', { name: en.instruction_analysis }))
    .toMatchTextContent('LONGMODE');
  await page.getByRole('combobox', { name: en.language }).selectOptions('zh-CN');
  await expect
    .element(page.getByRole('region', { name: zh.instruction_analysis }))
    .toMatchTextContent('LONGMODE');
  await expect.element(page.getByRole('alert')).not.toBeInTheDocument();
});

test('initial setup is target-specific, survives locale changes and applies only on load', async () => {
  const disclosure = (configuration: string, label: string) =>
    page
      .getByLabelText(configuration, { exact: true })
      .getByText(new RegExp(`^${RegExp.escape(label)}(?:\\s|$)`));
  const commands: Command[] = [];
  await render(Workbench, {
    portFactory: (): WorkerPort => ({
      connect: () => Promise.resolve(connection()),
      request: (command) => {
        commands.push(command);
        if (command.type === 'assemble') return Promise.resolve(assembled(command.data.identity));
        if (command.type === 'load')
          return Promise.resolve({
            response: { id: '2', result: { type: 'observed', data: observation() } },
            payloads: [],
          });
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
  await page.getByRole('button', { name: en.configuration, exact: true }).click();
  await disclosure(en.configuration, en.initial_registers).click();
  await page.getByRole('button', { name: en.add_register }).click();
  await page.getByRole('textbox', { name: /RAX/ }).fill('0xffffffffffffffff');
  await page.getByRole('button', { name: en.add_register }).click();
  await page
    .getByRole('button', { name: remove_register({ name: 'RCX' }, { locale: 'en' }) })
    .click();
  await disclosure(en.configuration, en.extra_memory).click();
  await page.getByRole('button', { name: en.add_mapping }).click();
  await page
    .getByRole('group', { name: memory_region({ number: 1 }, { locale: 'en' }) })
    .getByRole('textbox', { name: en.address, exact: true })
    .fill('0x80000');
  await page.getByRole('button', { name: en.add_mapping }).click();
  await page.getByRole('button', { name: remove_mapping({ number: 2 }, { locale: 'en' }) }).click();
  await page.getByRole('button', { name: en.close, exact: true }).click();
  expect(commands).toEqual([]);
  await page.getByRole('combobox', { name: en.target }).selectOptions('aarch64');
  await page.getByRole('button', { name: en.configuration, exact: true }).click();
  await disclosure(en.configuration, en.initial_registers).click();
  await expect
    .element(page.getByRole('combobox', { name: en.register_name, exact: true }))
    .not.toBeInTheDocument();
  await page.getByRole('button', { name: en.close, exact: true }).click();
  await page.getByRole('combobox', { name: en.target }).selectOptions('x86_64');
  await page.getByRole('combobox', { name: en.language }).selectOptions('zh-CN');
  await page.getByRole('button', { name: zh.configuration, exact: true }).click();
  await disclosure(zh.configuration, zh.initial_registers).click();
  await expect
    .element(page.getByRole('textbox', { name: /RAX/ }))
    .toHaveValue('0xffffffffffffffff');
  await page.getByRole('textbox', { name: /RAX/ }).fill('0x10000000000000000');
  await page.getByRole('button', { name: zh.close, exact: true }).click();
  await page.getByRole('button', { name: zh.assemble, exact: true }).click();
  await page.getByRole('button', { name: zh.load_artifact, exact: true }).click();
  await expect.element(page.getByRole('alert')).toMatchTextContent(zh.error_input);
  expect(commands.some((command) => command.type === 'load')).toBe(false);
  await page.getByRole('button', { name: zh.configuration, exact: true }).click();
  await disclosure(zh.configuration, zh.initial_registers).click();
  await page.getByRole('textbox', { name: /RAX/ }).fill('0xffffffffffffffff');
  await page.getByRole('button', { name: zh.close, exact: true }).click();
  await page.getByRole('button', { name: zh.load_artifact, exact: true }).click();
  await expect.element(page.getByRole('button', { name: zh.step, exact: true })).toBeEnabled();
  const load = commands.find((command) => command.type === 'load');
  if (load?.type !== 'load') throw new Error('Missing load request');
  expect(load.data.initial).toEqual({
    registers: [{ name: 'rax', value: '18446744073709551615' }],
    mappings: [{ address: '0x0000000000080000', length: 4096, flags: 6 }],
  });
});

test.each(['en', 'zh-CN'] as const)(
  '%s observation panel opens on assembly and retains its draft when hidden',
  async (locale) => {
    localStorage.setItem('PARAGLIDE_LOCALE', locale);
    const copy = locale === 'en' ? en : zh;
    const build = Promise.withResolvers<Awaited<ReturnType<WorkerPort['request']>>>();
    await render(Workbench, {
      portFactory: (): WorkerPort => ({
        connect: () => Promise.resolve(connection()),
        request: () => build.promise,
        acknowledge: () => Promise.resolve(),
        detach: () => {},
      }),
    });
    const toggle = page.getByRole('button', { name: copy.toggle_panel });
    await expect.element(toggle).toHaveAttribute('aria-expanded', 'false');
    await expect
      .element(page.getByRole('tab', { name: copy.memory, exact: true }))
      .not.toBeInTheDocument();
    await page.getByRole('button', { name: copy.assemble, exact: true }).click();
    await expect.element(toggle).toHaveAttribute('aria-expanded', 'true');
    const address = page.getByRole('textbox', { name: copy.address, exact: true });
    await address.fill('0x1234');
    await page.getByRole('button', { name: copy.hide_panel }).click();
    await expect.element(toggle).toHaveAttribute('aria-expanded', 'false');
    await userEvent.keyboard('{ControlOrMeta>}j{/ControlOrMeta}');
    await expect.element(address).toHaveValue('0x1234');
    build.resolve({
      response: {
        id: '1',
        result: { type: 'error', data: { code: 'assembly', address: null, source_offset: null } },
      },
      payloads: [],
    });
    await expect.element(page.getByRole('alert')).toBeVisible();
    await expect.element(toggle).toHaveAttribute('aria-expanded', 'true');
  },
);

test.each(['en', 'zh-CN'] as const)(
  '%s starts blank, loads examples explicitly and restores a deliberately empty draft',
  async (locale) => {
    localStorage.removeItem('oplab.scratch.v1');
    localStorage.setItem('PARAGLIDE_LOCALE', locale);
    const copy = locale === 'en' ? en : zh;
    const request = vi.fn<WorkerPort['request']>((command) => {
      if (command.type === 'assemble') return Promise.resolve(assembled(command.data.identity));
      throw new Error(`Unexpected ${command.type}`);
    });
    const saved: Uint8Array[] = [];
    const props = {
      portFactory: (): WorkerPort => ({
        connect: () => Promise.resolve(connection()),
        request,
        acknowledge: () => Promise.resolve(),
        detach: () => {},
      }),
      filePort: {
        open: () => Promise.resolve(null),
        save: (_format: FileFormat, _title: string, bytes: Uint8Array) => {
          saved.push(bytes);
          return Promise.resolve(true);
        },
      },
    };
    const first = await render(Workbench, props);
    const editor = page.getByRole('textbox', { name: copy.editor_label });
    const assemble = page.getByRole('button', { name: copy.assemble, exact: true });
    await expect.element(editor).toBeVisible();
    await expect.element(assemble).toBeDisabled();
    await editor.click();
    await userEvent.keyboard('{ControlOrMeta>}{Enter}{/ControlOrMeta}');
    await expect
      .element(page.getByRole('button', { name: copy.toggle_panel }))
      .toHaveAttribute('aria-expanded', 'false');
    expect(request).not.toHaveBeenCalled();
    const exportSource = async () => {
      await page.getByRole('button', { name: copy.files, exact: true }).click();
      await page.getByRole('menuitem', { name: copy.export_source }).click();
    };
    await exportSource();
    await expect.poll(() => saved.at(-1)).toEqual(new Uint8Array());
    await page.getByRole('button', { name: copy.load_example }).click();
    await expect.element(assemble).toBeEnabled();
    await exportSource();
    await expect.poll(() => saved.at(-1)?.length ?? 0).toBeGreaterThan(0);
    const example = saved.at(-1);
    await editor.click();
    await userEvent.keyboard('{ControlOrMeta>}{Enter}{/ControlOrMeta}');
    await expect.poll(() => request.mock.calls.length).toBe(1);
    const command = request.mock.calls[0]?.[0];
    if (command?.type !== 'assemble') throw new Error('Missing assembly request');
    expect(new TextEncoder().encode(command.data.source)).toEqual(example);
    await editor.click();
    await userEvent.keyboard('{ControlOrMeta>}a{/ControlOrMeta}{Backspace}');
    await expect.element(assemble).toBeDisabled();
    await first.unmount();
    await render(Workbench, props);
    await expect.element(assemble).toBeDisabled();
    await exportSource();
    await expect.poll(() => saved.at(-1)).toEqual(new Uint8Array());
  },
);

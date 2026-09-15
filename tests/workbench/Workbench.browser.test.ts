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
import Workbench from '$lib/workbench/Workbench.svelte';
import { afterEach, beforeEach, expect, test } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page, userEvent } from 'vitest/browser';

import en from '../../messages/en.json';
import zh from '../../messages/zh-CN.json';
import { assembled, connection, observation, nopAnalysis } from '../fixtures/protocol';

import '$lib/styles/theme.css';

beforeEach(() => {
  localStorage.clear();
  localStorage.setItem('PARAGLIDE_LOCALE', 'en');
});
afterEach(() => {
  localStorage.clear();
});

test('switching locale retains edits and undo history', async () => {
  await render(Workbench);
  const editor = page.getByRole('textbox', { name: en.editor_label });
  await expect.element(editor).toBeVisible();
  const originalText = editor.element().textContent;
  expect(originalText).toContain('mov rax, 40');
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

test('an open search panel adopts the new language without losing its query', async () => {
  await render(Workbench);
  await page.getByRole('textbox', { name: en.editor_label }).click();
  await userEvent.keyboard('{ControlOrMeta>}f{/ControlOrMeta}');
  await userEvent.keyboard('rax');
  await page.getByRole('combobox', { name: en.language }).selectOptions('zh-CN');
  await expect
    .element(page.getByRole('textbox', { name: zh.find, exact: true }))
    .toHaveValue('rax');
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
  await userEvent.keyboard('{ControlOrMeta>}a{/ControlOrMeta}// 中文{Enter}mov x0, #7');
  await first.unmount();
  await render(Workbench);
  await expect.element(page.getByRole('combobox', { name: en.target })).toHaveValue('aarch64');
  await expect
    .element(page.getByRole('textbox', { name: en.editor_label }))
    .toHaveTextContent('// 中文mov x0, #7');
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
  await userEvent.keyboard('{ControlOrMeta>}a{/ControlOrMeta}// 中文{Enter}  invalid');
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
  const source = '\uFEFF// 中文 😀\r\n  nop\r\n';
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
  await page.getByRole('menuitem', { name: en.import_binary }).click();
  await expect
    .element(page.getByRole('tab', { name: en.instructions, exact: true }))
    .toHaveAttribute('aria-selected', 'true');
  await page.getByRole('button', { name: en.disassemble, exact: true }).click();
  const instructions = page.getByRole('table', { name: en.instructions, exact: true });
  await expect.element(instructions).toMatchTextContent('0x0000000000001001');
  await expect.element(instructions).toMatchTextContent('ret');
  await expect
    .element(page.getByRole('tabpanel', { name: en.memory, exact: true }))
    .not.toBeInTheDocument();
  await files.click();
  await page.getByRole('menuitem', { name: en.export_binary }).click();
  await expect.poll(() => saved.length).toBe(1);
  expect(saved[0]).toEqual({ format: 'binary', bytes });
  await page.getByRole('tab', { name: en.memory, exact: true }).click();
  await page.getByRole('tab', { name: en.instructions, exact: true }).click();
  await expect.element(instructions).toMatchTextContent('ret');
  await page.getByRole('button', { name: 'nop', exact: true }).click();
  await expect
    .element(page.getByRole('region', { name: en.instruction_analysis }))
    .toMatchTextContent('X64');
  await page.getByRole('combobox', { name: en.language }).selectOptions('zh-CN');
  await expect
    .element(page.getByRole('region', { name: zh.instruction_analysis }))
    .toMatchTextContent('X64');
  await expect.element(page.getByRole('alert')).not.toBeInTheDocument();
});

test('initial setup is target-specific, survives locale changes and applies only on load', async () => {
  const disclosure = (label: string) => {
    const summary = Array.from(document.querySelectorAll('summary')).find((element) =>
      element.textContent.includes(label),
    );
    if (summary === undefined) throw new Error('Missing setup disclosure');
    return page.elementLocator(summary);
  };
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
  await disclosure(en.initial_registers).click();
  await page.getByRole('button', { name: en.add_register }).click();
  await page.getByRole('textbox', { name: /RAX/ }).fill('0xffffffffffffffff');
  await page.getByRole('button', { name: en.add_register }).click();
  await page
    .getByRole('button', { name: remove_register({ name: 'RCX' }, { locale: 'en' }) })
    .click();
  await disclosure(en.extra_memory).click();
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
  await disclosure(en.initial_registers).click();
  await expect
    .element(page.getByRole('combobox', { name: en.register_name, exact: true }))
    .not.toBeInTheDocument();
  await page.getByRole('button', { name: en.close, exact: true }).click();
  await page.getByRole('combobox', { name: en.target }).selectOptions('x86_64');
  await page.getByRole('combobox', { name: en.language }).selectOptions('zh-CN');
  await page.getByRole('button', { name: zh.configuration, exact: true }).click();
  await disclosure(zh.initial_registers).click();
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
  await disclosure(zh.initial_registers).click();
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

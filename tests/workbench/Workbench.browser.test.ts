import { afterEach, beforeEach, expect, test } from 'vitest';
import { page, userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';
import Workbench from '$lib/workbench/Workbench.svelte';
import type { WorkerPort } from '$lib/desktop/worker';
import type { BuildIdentity } from '$lib/protocol/generated/BuildIdentity';
import type { Command } from '$lib/protocol/generated/Command';
import { assembled, connection, observation } from '../fixtures/protocol';
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
  const editor = page.getByRole('textbox', { name: 'Assembly source editor' });
  await expect.element(editor).toBeVisible();
  const originalText = editor.element().textContent;
  expect(originalText).toContain('mov rax, 40');
  await editor.click();
  await userEvent.keyboard('{ControlOrMeta>}a{/ControlOrMeta}mov rax, 99');
  await page.getByRole('combobox', { name: 'Language' }).selectOptions('zh-CN');
  const chinese = page.getByRole('textbox', { name: '汇编源码编辑器' });
  await expect.element(chinese).toHaveTextContent('mov rax, 99');
  expect(document.documentElement.lang).toBe('zh-CN');
  await chinese.click();
  await userEvent.keyboard('{ControlOrMeta>}z{/ControlOrMeta}');
  await expect.element(chinese).toHaveTextContent(originalText);
});

test('an open search panel adopts the new language without losing its query', async () => {
  await render(Workbench);
  await page.getByRole('textbox', { name: 'Assembly source editor' }).click();
  await userEvent.keyboard('{ControlOrMeta>}f{/ControlOrMeta}');
  await userEvent.keyboard('rax');
  await page.getByRole('combobox', { name: 'Language' }).selectOptions('zh-CN');
  await expect.element(page.getByRole('textbox', { name: '查找', exact: true })).toHaveValue('rax');
  await expect.element(page.getByRole('button', { name: '全部替换', exact: true })).toBeVisible();
});

test('a late build for edited source cannot become loadable', async () => {
  const { promise, resolve } = Promise.withResolvers<Awaited<ReturnType<WorkerPort['request']>>>();
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
  const assemble = page.getByRole('button', { name: 'Assemble', exact: true });
  await expect.element(assemble).toBeEnabled();
  await assemble.click();
  const editor = page.getByRole('textbox', { name: 'Assembly source editor' });
  await editor.click();
  await userEvent.keyboard('{ControlOrMeta>}a{/ControlOrMeta}nop');
  if (build === undefined) throw new Error('Assembly request missing');
  resolve(assembled(build));
  await expect.element(assemble).toBeEnabled();
  await expect.element(page.getByRole('button', { name: 'Load artifact' })).toBeDisabled();
  await expect.element(editor).toHaveTextContent('nop');
});

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
  await expect.element(page.getByRole('button', { name: 'Step', exact: true })).toBeEnabled();
  await expect.element(page.getByRole('alert')).toMatchTextContent('Draft recovery is unavailable');
  expect(commands.some((command) => command.type === 'subscribe')).toBe(true);
  expect(commands.every((command) => command.type === 'subscribe')).toBe(true);
});

test('appearance changes and focus mode preserve edits and undo history', async () => {
  await render(Workbench);
  const editor = page.getByRole('textbox', { name: 'Assembly source editor' });
  await expect.element(editor).toBeVisible();
  const original = editor.element().textContent;
  await editor.click();
  await userEvent.keyboard('{ControlOrMeta>}a{/ControlOrMeta}mov rax, 77');
  await page.getByRole('button', { name: 'Appearance', exact: true }).click();
  await page.getByRole('combobox', { name: 'Editor font' }).selectOptions('system');
  await page.getByRole('checkbox', { name: 'Wrap long lines' }).click();
  const size = page.getByRole('slider', { name: 'Font size' });
  await size.click();
  await userEvent.keyboard('{End}');
  await expect.element(size).toHaveValue(size.element().getAttribute('max'));
  await userEvent.keyboard('{Escape}');
  await expect.element(page.getByRole('button', { name: 'Appearance', exact: true })).toHaveFocus();
  await expect.element(editor).toHaveTextContent('mov rax, 77');
  await page.getByRole('button', { name: 'Focus mode', exact: true }).click();
  await expect
    .element(page.getByRole('complementary', { name: 'Machine' }))
    .not.toBeInTheDocument();
  await editor.click();
  await userEvent.keyboard('{ControlOrMeta>}z{/ControlOrMeta}');
  await expect.element(editor).toHaveTextContent(original);
});

test('architecture changes reconfigure completion and comment commands without changing source', async () => {
  await render(Workbench);
  const editor = page.getByRole('textbox', { name: 'Assembly source editor' });
  await page.getByRole('combobox', { name: 'Architecture' }).selectOptions('aarch64');
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
  await page.getByRole('combobox', { name: 'Architecture' }).selectOptions('aarch64');
  await page.getByRole('textbox', { name: 'Assembly source editor' }).click();
  await userEvent.keyboard('{ControlOrMeta>}a{/ControlOrMeta}// 中文{Enter}mov x0, #7');
  await first.unmount();
  await render(Workbench);
  await expect.element(page.getByRole('combobox', { name: 'Architecture' })).toHaveValue('aarch64');
  await expect
    .element(page.getByRole('textbox', { name: 'Assembly source editor' }))
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
  await page.getByRole('button', { name: 'Assemble', exact: true }).click();
  const load = page.getByRole('button', { name: 'Load artifact', exact: true });
  await expect.element(load).toBeEnabled();
  expect(commands.every((command) => command.type === 'assemble')).toBe(true);
  await load.click();
  const run = page.getByRole('button', { name: 'Run', exact: true });
  await expect.element(run).toBeEnabled();
  expect(commands.some((command) => command.type === 'execute')).toBe(false);
  await run.click();
  await expect.element(run).toBeDisabled();
  const reset = page.getByRole('button', { name: 'Reset', exact: true });
  await reset.click();
  await expect.element(page.getByRole('alert')).toBeVisible();
  await expect.element(reset).toBeEnabled();
  await expect
    .element(page.getByRole('complementary', { name: 'Machine' }))
    .toMatchTextContent('Completed');
});

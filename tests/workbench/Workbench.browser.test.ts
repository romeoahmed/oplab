import { afterEach, beforeEach, expect, test } from 'vitest';
import { page, userEvent } from 'vitest/browser';
import { cleanup, render } from 'vitest-browser-svelte';
import Workbench from '$lib/workbench/Workbench.svelte';
import type { WorkerPort } from '$lib/desktop/worker';
import type { BuildIdentity } from '$lib/protocol/generated/BuildIdentity';
import type { Command } from '$lib/protocol/generated/Command';
import type { ConnectionInfo } from '$lib/protocol/generated/ConnectionInfo';
import '$lib/styles/theme.css';

beforeEach(() => {
  localStorage.clear();
  localStorage.setItem('PARAGLIDE_LOCALE', 'en');
});
afterEach(() => {
  cleanup();
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

test('a build for an edited revision cannot replace the current artifact', async () => {
  const { promise, resolve } = Promise.withResolvers<Awaited<ReturnType<WorkerPort['request']>>>();
  let build: BuildIdentity | undefined;
  await render(Workbench, {
    portFactory: () => ({
      connect: () =>
        Promise.resolve({
          connection: '1',
          view: '1',
          session: null,
          artifact: null,
          capabilities: {
            version: 1,
            targets: ['x86_64', 'aarch64'],
            assembler: { name: 'LLVM MC', version: '23' },
            source_mapping: false,
            execution: true,
          },
        }),
      request: (command: Command) => {
        if (command.type === 'assemble') build = command.data.identity;
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
  resolve({
    response: {
      id: '1',
      result: {
        type: 'assembled',
        data: {
          identity: build,
          object_bytes: 1,
          image_bytes: 1,
          image: {
            entry: '0x0000000000001000',
            segments: [],
            symbols: [],
            symbols_truncated: false,
          },
        },
      },
    },
    payloads: [new Uint8Array(1), new Uint8Array(1)],
  });
  await expect.element(assemble).toBeEnabled();
  await expect.element(page.getByRole('button', { name: 'Load artifact' })).toBeDisabled();
  await expect.element(editor).toHaveTextContent('nop');
});

test.each([false, true])(
  'reattachment preserves the session when scratch recovery is corrupt: %s',
  async (corrupt) => {
    if (corrupt) localStorage.setItem('oplab.scratch.v1', '{');
    const commands: Command[] = [];
    const info: ConnectionInfo = {
      connection: '1',
      view: '2',
      artifact: null,
      capabilities: {
        version: 1,
        targets: ['x86_64', 'aarch64'],
        assembler: { name: 'LLVM MC', version: '23' },
        source_mapping: false,
        execution: true,
      },
      session: {
        key: { session: '2', generation: '0' },
        sequence: '4',
        status: { type: 'ready' },
        instructions: '0',
        dispatches: '0',
        fault: null,
        memory: null,
        registers: {
          type: 'x86_64',
          data: {
            gpr: ['0', '0', '0', '0', '0', '0', '0', '0', '0', '0', '0', '0', '0', '0', '0', '0'],
            rip: '0x0000000000010000',
            rflags: '2',
          },
        },
      },
    };
    await render(Workbench, {
      portFactory: () => ({
        connect: () => Promise.resolve(info),
        request: (command: Command) => {
          commands.push(command);
          return Promise.resolve({
            response: { id: '3', result: { type: 'subscribed', data: '3' } },
            payloads: [],
          });
        },
        acknowledge: () => Promise.resolve(),
        detach: () => {},
      }),
    });
    await expect
      .poll(() => commands)
      .toEqual([
        { type: 'subscribe', data: { session: { session: '2', generation: '0' }, memory: null } },
      ]);
    await expect.element(page.getByRole('button', { name: 'Step', exact: true })).toBeEnabled();
    if (corrupt) {
      await expect
        .element(page.getByRole('alert'))
        .toMatchTextContent('Scratch recovery is unavailable');
    } else {
      await expect.element(page.getByRole('alert')).not.toBeInTheDocument();
    }
  },
);

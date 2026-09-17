import type { WorkerPort } from '$lib/desktop/worker';
import type { Command } from '$lib/protocol/generated/Command';
import Workbench from '$lib/workbench/Workbench.svelte';
import { settled } from 'svelte';
import { afterEach, beforeEach, expect, test } from 'vitest';
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
  { locale: 'en', target: 'x86_64', name: 'xmm0' },
  { locale: 'zh-CN', target: 'aarch64', name: 'v0' },
] as const)(
  '$target SIMD mutations wait for replies and never replay rejected writes',
  async ({ locale, target, name }) => {
    localStorage.setItem('PARAGLIDE_LOCALE', locale);
    const copy = locale === 'en' ? en : zh;
    const machine = observation('1', target);
    const commands: Command[] = [];
    const pending = Array.from({ length: 3 }, () =>
      Promise.withResolvers<Awaited<ReturnType<WorkerPort['request']>>>(),
    );
    await render(Workbench, {
      portFactory: (): WorkerPort => ({
        connect: () => Promise.resolve(connection(machine)),
        request: (command) => {
          if (command.type === 'subscribe')
            return Promise.resolve({
              response: { id: '1', result: { type: 'subscribed', data: '1' } },
              payloads: [],
            });
          commands.push(command);
          const reply = pending[commands.length - 1];
          if (reply === undefined) throw new Error('Unexpected request');
          return reply.promise;
        },
        acknowledge: () => Promise.resolve(),
        detach: () => {},
      }),
    });
    await page.getByRole('tab', { name: 'SIMD', exact: true }).click();
    await page.getByRole('combobox', { name: copy.vector_bank }).selectOptions('low');
    await page.getByRole('button', { name: copy.edit_vector }).click();
    const form = page.getByRole('form', { name: copy.edit_vector });
    await form
      .getByRole('textbox', { name: copy.register_value })
      .fill('7fc123457f800000800000003fc00000');
    const write = form.getByRole('button', { name: copy.write_register });
    await write.click();
    await expect.element(write).toBeDisabled();
    await form.getByRole('textbox', { name: copy.register_value }).click();
    await userEvent.keyboard('{Enter}');
    await settled();
    expect(commands).toEqual([
      {
        type: 'execute',
        data: {
          session: machine.key,
          action: {
            type: 'write_vector',
            data: { name, width: 128, lane: 0, value: '0x7fc123457f800000800000003fc00000' },
          },
        },
      },
    ]);
    pending[0]?.resolve({
      response: {
        id: '2',
        result: {
          type: 'error',
          data: { code: 'stale_session', address: null, source_offset: null },
        },
      },
      payloads: [],
    });
    await expect.element(write).toBeEnabled();
    await expect.element(page.getByRole('alert')).toBeVisible();
    expect(commands).toHaveLength(1);
    await write.click();
    await expect.element(write).toBeDisabled();
    const changed = structuredClone(machine);
    changed.sequence = '3';
    if (changed.registers === null) throw new Error('Missing bank');
    const values =
      changed.registers.type === 'x86_64' ? changed.registers.data.ymm : changed.registers.data.z;
    values[0] =
      '0x' + '7fc123457f800000800000003fc00000'.padStart(target === 'x86_64' ? 64 : 512, '0');
    pending[1]?.resolve({
      response: { id: '3', result: { type: 'observed', data: changed } },
      payloads: [],
    });
    await expect.element(write).toBeEnabled();
    await page.getByRole('button', { name: copy.close, exact: true }).click();
    await expect.element(page.getByText(values[0].slice(-32), { exact: true })).toBeVisible();
    await expect.element(page.getByRole('alert')).not.toBeInTheDocument();
    const rounding = page.getByRole('combobox', { name: copy.rounding_mode });
    await rounding.selectOptions('up');
    await page.getByRole('button', { name: copy.set_rounding }).click();
    await expect.element(rounding).toBeDisabled();
    expect(commands[2]).toEqual({
      type: 'execute',
      data: { session: machine.key, action: { type: 'set_rounding', data: 'up' } },
    });
    const rounded = structuredClone(changed);
    rounded.sequence = '4';
    if (rounded.registers === null) throw new Error('Missing bank');
    if (rounded.registers.type === 'x86_64') rounded.registers.data.mxcsr = 0x5f80;
    else rounded.registers.data.fpcr = 0x00400000;
    pending[2]?.resolve({
      response: { id: '4', result: { type: 'observed', data: rounded } },
      payloads: [],
    });
    await expect.element(rounding).toBeEnabled();
    await expect.element(rounding).toHaveValue('up');
    await expect.element(page.getByRole('button', { name: copy.set_rounding })).toBeDisabled();
    await expect
      .element(page.getByText(target === 'x86_64' ? '00005f80' : '00400000', { exact: true }))
      .toBeVisible();
    expect(commands).toHaveLength(3);
  },
);

import type { WorkerPort } from '$lib/desktop/worker';
import { source_breakpoint, reveal_source } from '$lib/paraglide/messages.js';
import type { Command } from '$lib/protocol/generated/Command';
import type { SessionAction } from '$lib/protocol/generated/SessionAction';
import Workbench from '$lib/workbench/Workbench.svelte';
import { afterEach, expect, test, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page, userEvent } from 'vitest/browser';

import en from '../../messages/en.json';
import zh from '../../messages/zh-CN.json';
import { assembled, connection, observation } from '../fixtures/protocol';
import { scratch } from '../fixtures/scratch';

import '$lib/styles/theme.css';

afterEach(() => {
  localStorage.clear();
});

test.each(['en', 'zh-CN'] as const)(
  '%s grouped breakpoints retry without losing state; source navigation expires on edits',
  async (locale) => {
    const copy = locale === 'en' ? en : zh;
    localStorage.clear();
    localStorage.setItem('PARAGLIDE_LOCALE', locale);
    localStorage.setItem(
      'oplab.scratch.v1',
      JSON.stringify({ ...scratch, source: 'nop; nop\ndone: nop' }),
    );
    const first = '0x0000000000001000';
    const addresses = [first, '0x0000000000001001'];
    const machine = observation('1');
    const pending = Promise.withResolvers<Awaited<ReturnType<WorkerPort['request']>>>();
    const breakpointReply = vi
      .fn<WorkerPort['request']>(() => Promise.reject(new Error('Unexpected breakpoint request')))
      .mockResolvedValueOnce({
        response: {
          id: '5',
          result: { type: 'observed', data: { ...machine, sequence: '2', breakpoints: [first] } },
        },
        payloads: [],
      })
      .mockReturnValueOnce(pending.promise)
      .mockResolvedValueOnce({
        response: {
          id: '6',
          result: { type: 'observed', data: { ...machine, sequence: '3', breakpoints: addresses } },
        },
        payloads: [],
      })
      .mockResolvedValueOnce({
        response: {
          id: '7',
          result: { type: 'observed', data: { ...machine, sequence: '4', breakpoints: [] } },
        },
        payloads: [],
      });
    const controls: SessionAction[] = [];
    await render(Workbench, {
      portFactory: (): WorkerPort => ({
        connect: () => Promise.resolve(connection()),
        request: (command: Command) => {
          if (command.type === 'assemble') {
            const result = assembled(command.data.identity);
            if (result.response.result.type !== 'assembled') throw new Error('Invalid fixture');
            result.response.result.data.source_map.locations = addresses.map((address) => ({
              address,
              line: 1,
            }));
            result.response.result.data.image.segments = [
              {
                address: first,
                file_offset: 0,
                file_bytes: 2,
                memory_bytes: '2',
                flags: 5,
              },
            ];
            result.payloads[1] = new Uint8Array([0x90, 0x90]);
            result.response.result.data.image_bytes = 2;
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
          if (command.type === 'decode')
            return Promise.resolve({
              response: {
                id: '4',
                result: {
                  type: 'decoded',
                  data: addresses.map((address) => ({ address, bytes: [0x90], text: 'nop' })),
                },
              },
              payloads: [],
            });
          if (command.type === 'execute' && command.data.action.type === 'breakpoint') {
            controls.push(command.data.action);
            return breakpointReply(command);
          }
          throw new Error(`Unexpected ${command.type}`);
        },
        acknowledge: () => Promise.resolve(),
        detach: () => {},
      }),
    });
    await page.getByRole('button', { name: copy.assemble, exact: true }).click();
    const breakpoint = page.getByRole('button', {
      name: source_breakpoint({ line: 1 }, { locale }),
    });
    await expect.element(breakpoint).toBeDisabled();
    await page.getByRole('button', { name: copy.load_artifact, exact: true }).click();
    await expect.element(breakpoint).toBeEnabled();
    await page.getByRole('tab', { name: copy.breakpoint_view, exact: false }).click();
    await page.getByRole('textbox', { name: copy.breakpoint_address }).fill(first);
    await page.getByRole('button', { name: copy.add_breakpoint, exact: true }).click();
    await expect.element(breakpoint).toHaveAttribute('aria-pressed', 'mixed');
    await page.getByTitle(copy.add_source_breakpoint).first().hover();
    await page.getByTitle(copy.add_source_breakpoint).first().click();
    await expect.element(breakpoint).toBeDisabled();
    const editor = page.getByRole('textbox', { name: copy.editor_label });
    await editor.click();
    await userEvent.keyboard('{ControlOrMeta>}{Home}{/ControlOrMeta}{F9}');
    expect(controls).toEqual([
      { type: 'breakpoint', data: { addresses: [first], enabled: true } },
      { type: 'breakpoint', data: { addresses, enabled: true } },
    ]);
    pending.resolve({
      response: {
        id: '5',
        result: {
          type: 'error',
          data: { code: 'resource_limit', source_offset: null, address: null },
        },
      },
      payloads: [],
    });
    await expect.element(breakpoint).toBeEnabled();
    await expect.element(breakpoint).toHaveAttribute('aria-pressed', 'mixed');
    await breakpoint.click();
    await expect.element(breakpoint).toHaveAttribute('aria-pressed', 'true');
    await page.getByRole('button', { name: copy.reveal_execution }).click();
    await userEvent.keyboard('{F9}');
    await expect.element(breakpoint).toHaveAttribute('aria-pressed', 'false');
    expect(controls).toEqual([
      { type: 'breakpoint', data: { addresses: [first], enabled: true } },
      { type: 'breakpoint', data: { addresses, enabled: true } },
      { type: 'breakpoint', data: { addresses, enabled: true } },
      { type: 'breakpoint', data: { addresses, enabled: false } },
    ]);
    await page.getByRole('button', { name: copy.reveal_instruction }).click();
    const sourceLinks = page.getByRole('button', { name: reveal_source({ line: 1 }, { locale }) });
    await expect.element(sourceLinks.first()).toBeVisible();
    await sourceLinks.first().click();
    await expect.element(editor).toHaveFocus();
    await userEvent.keyboard(' ');
    await expect.element(breakpoint).not.toBeInTheDocument();
    await expect
      .element(page.getByRole('button', { name: copy.reveal_execution }))
      .not.toBeInTheDocument();
    await expect.element(sourceLinks.first()).not.toBeInTheDocument();
  },
);

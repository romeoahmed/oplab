import type { WorkerPort } from '$lib/desktop/worker';
import { close_document_named } from '$lib/paraglide/messages.js';
import type { BuildIdentity } from '$lib/protocol/generated/BuildIdentity';
import Workbench from '$lib/workbench/Workbench.svelte';
import { settled } from 'svelte';
import { afterEach, beforeEach, expect, onTestFinished, test, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page, userEvent } from 'vitest/browser';

import en from '../../messages/en.json';
import zh from '../../messages/zh-CN.json';
import { assembled, connection, observation } from '../fixtures/protocol';
import { scratch } from '../fixtures/scratch';

import '$lib/styles/theme.css';

beforeEach(async () => {
  localStorage.clear();
  localStorage.setItem('PARAGLIDE_LOCALE', 'en');
  await page.viewport(1440, 900);
});
afterEach(() => {
  localStorage.clear();
});

test.each(['en', 'zh-CN'] as const)(
  '%s documents retain independent undo, search, target and recovery',
  async (locale) => {
    localStorage.setItem('PARAGLIDE_LOCALE', locale);
    const copy = locale === 'en' ? en : zh;
    const first = await render(Workbench, { portFactory: () => null });
    const editor = page.getByRole('textbox', { name: copy.editor_label });
    await expect.element(editor).toBeVisible();
    await editor.click();
    await userEvent.keyboard('nop');
    await page.getByRole('button', { name: copy.find, exact: true }).click();
    await page.getByRole('textbox', { name: copy.find, exact: true }).fill('nop');
    await page.getByRole('button', { name: copy.new_document }).click();
    await expect.element(editor).not.toMatchTextContent('nop');
    await page.getByRole('combobox', { name: copy.target, exact: true }).selectOptions('aarch64');
    await editor.click();
    await userEvent.keyboard('yield');
    await page.getByRole('tab', { name: 'untitled-1.s', exact: true }).click();
    await expect.element(editor).toHaveTextContent('nop');
    await expect
      .element(page.getByRole('combobox', { name: copy.target, exact: true }))
      .toHaveValue('x86_64');
    await expect
      .element(page.getByRole('textbox', { name: copy.find, exact: true }))
      .toHaveValue('nop');
    await editor.click();
    await userEvent.keyboard('{ControlOrMeta>}z{/ControlOrMeta}');
    await expect.element(editor).not.toMatchTextContent('nop');
    await page.getByRole('tab', { name: 'untitled-2.s', exact: true }).click();
    await expect.element(editor).toHaveTextContent('yield');
    await expect
      .element(page.getByRole('combobox', { name: copy.target, exact: true }))
      .toHaveValue('aarch64');
    await first.unmount();
    await render(Workbench, { portFactory: () => null });
    await expect
      .element(page.getByRole('tab', { name: 'untitled-2.s', exact: true }))
      .toHaveAttribute('aria-selected', 'true');
    await expect.element(editor).toHaveTextContent('yield');
    await page.getByRole('tab', { name: 'untitled-1.s', exact: true }).click();
    await expect.element(editor).not.toMatchTextContent('nop');
    const title = document.title;
    const other = locale === 'en' ? zh : en;
    await page
      .getByRole('combobox', { name: copy.language, exact: true })
      .selectOptions(locale === 'en' ? 'zh-CN' : 'en');
    await expect.element(page.getByRole('textbox', { name: other.editor_label })).toBeVisible();
    expect(document.title).toBe(title);
    await page.getByRole('button', { name: other.go_to_line, exact: true }).click();
    await expect
      .element(page.getByRole('textbox', { name: `${other.go_to_line}:`, exact: true }))
      .toBeVisible();
  },
);

test.each([
  { outcome: 'artifact', closed: false },
  { outcome: 'diagnostic', closed: false },
  { outcome: 'artifact', closed: true },
  { outcome: 'diagnostic', closed: true },
])(
  'late $outcome stays with its source document (closed: $closed)',
  async ({ outcome, closed }) => {
    const pending = Promise.withResolvers<Awaited<ReturnType<WorkerPort['request']>>>();
    let build: BuildIdentity | undefined;
    await render(Workbench, {
      portFactory: (): WorkerPort => ({
        connect: () => Promise.resolve(connection()),
        request: (command) => {
          if (command.type !== 'assemble') throw new Error(`Unexpected ${command.type}`);
          build = command.data.identity;
          return pending.promise;
        },
        acknowledge: () => Promise.resolve(),
        detach: () => {},
      }),
    });
    const editor = page.getByRole('textbox', { name: en.editor_label });
    await expect.element(editor).toBeVisible();
    const assemble = page.getByRole('button', { name: en.assemble, exact: true });
    const load = page.getByRole('button', { name: en.load_artifact, exact: true });
    await editor.click();
    await userEvent.keyboard('nop');
    await assemble.click();
    await expect
      .element(page.getByRole('button', { name: en.assembling, exact: true }))
      .toBeDisabled();
    if (closed) {
      await page
        .getByRole('button', {
          name: close_document_named({ name: 'untitled-1.s' }, { locale: 'en' }),
          exact: true,
        })
        .click();
      await page
        .getByRole('alertdialog')
        .getByRole('button', { name: en.close_document, exact: true })
        .click();
    } else {
      await page.getByRole('button', { name: en.new_document }).click();
    }
    await editor.click();
    await userEvent.keyboard('yield');
    if (build === undefined) throw new Error('Assembly request missing');
    pending.resolve(
      outcome === 'artifact'
        ? assembled(build)
        : {
            response: {
              id: '1',
              result: {
                type: 'error',
                data: { code: 'assembly', address: null, source_offset: 0 },
              },
            },
            payloads: [],
          },
    );
    await pending.promise;
    await settled();
    await expect.element(assemble).toBeEnabled();
    await expect.element(editor).toHaveTextContent('yield');
    await expect.element(load).toBeDisabled();
    await expect.element(page.getByRole('alert')).not.toBeInTheDocument();
    if (!closed) {
      await page.getByRole('tab', { name: 'untitled-1.s', exact: true }).click();
      await expect.element(editor).toHaveTextContent('nop');
      if (outcome === 'artifact') await expect.element(load).toBeEnabled();
      else await expect.element(page.getByRole('alert')).toBeVisible();
    }
  },
);

test('closing the last nonempty document requires confirmation and recovers a fresh draft', async () => {
  const view = await render(Workbench, { portFactory: () => null });
  const editor = page.getByRole('textbox', { name: en.editor_label });
  await expect.element(editor).toBeVisible();
  const close = page.getByRole('button', {
    name: close_document_named({ name: 'untitled-1.s' }, { locale: 'en' }),
    exact: true,
  });
  const dialog = page.getByRole('alertdialog');
  await editor.click();
  await userEvent.keyboard('nop');
  await close.click();
  await dialog.getByRole('button', { name: en.keep_document }).click();
  await expect.element(editor).toHaveTextContent('nop');
  await expect.element(close).toHaveFocus();
  await userEvent.keyboard('{Enter}');
  await expect.element(dialog).toBeVisible();
  await userEvent.keyboard('{Escape}');
  await expect.element(dialog).not.toBeInTheDocument();
  await expect.element(close).toHaveFocus();
  await userEvent.keyboard('{Enter}');
  await dialog.getByRole('button', { name: en.close_document, exact: true }).click();
  await expect.element(editor).toBeVisible();
  await editor.click();
  await userEvent.keyboard('int3');
  await expect.element(editor).toHaveTextContent('int3');
  await userEvent.keyboard('{ControlOrMeta>}z{/ControlOrMeta}');
  await expect.element(editor).not.toMatchTextContent('nop');
  await view.unmount();
  await render(Workbench, { portFactory: () => null });
  await expect.element(editor).toBeVisible();
  await editor.click();
  await userEvent.keyboard('yield');
  await expect.element(editor).toHaveTextContent('yield');
});

test('separators follow their advertised keyboard bounds and restore both panel sizes', async () => {
  const first = await render(Workbench, { portFactory: () => null });
  const machine = page.getByRole('separator', { name: en.resize_inspector });
  await expect.element(machine).toBeVisible();
  const initial = Number(machine.element().getAttribute('aria-valuenow'));
  const maximum = machine.element().getAttribute('aria-valuemax');
  await machine.click();
  await userEvent.keyboard('{ArrowLeft}');
  await expect.element(machine).toHaveAttribute('aria-valuenow', String(initial + 1));
  await userEvent.keyboard('{End}{ArrowLeft}');
  await expect.element(machine).toHaveAttribute('aria-valuenow', maximum);
  await page.getByRole('button', { name: en.toggle_panel }).click();
  const observations = page.getByRole('separator', { name: en.resize_observations });
  await expect.element(observations).toBeVisible();
  const minimum = observations.element().getAttribute('aria-valuemin');
  await observations.click();
  await userEvent.keyboard('{Home}{ArrowDown}');
  await expect.element(observations).toHaveAttribute('aria-valuenow', minimum);
  await page.getByRole('button', { name: en.hide_panel }).click();
  await expect.element(observations).not.toBeInTheDocument();
  await first.unmount();
  await render(Workbench, { portFactory: () => null });
  await expect.element(machine).toHaveAttribute('aria-valuenow', maximum);
  await page.getByRole('button', { name: en.toggle_panel }).click();
  await expect.element(observations).toHaveAttribute('aria-valuenow', minimum);
});

test('loading captures its document and closing that document leaves the machine available', async () => {
  const pending = Promise.withResolvers<Awaited<ReturnType<WorkerPort['request']>>>();
  const requests: string[] = [];
  await render(Workbench, {
    portFactory: (): WorkerPort => ({
      connect: () => Promise.resolve(connection()),
      request: (command) => {
        requests.push(command.type);
        if (command.type === 'assemble') return Promise.resolve(assembled(command.data.identity));
        if (command.type === 'load') return pending.promise;
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
  const editor = page.getByRole('textbox', { name: en.editor_label });
  await expect.element(editor).toBeVisible();
  await editor.click();
  await userEvent.keyboard('nop');
  await page.getByRole('button', { name: en.assemble, exact: true }).click();
  await page.getByRole('button', { name: en.load_artifact, exact: true }).click();
  await page.getByRole('button', { name: en.new_document }).click();
  await editor.click();
  await userEvent.keyboard('yield');
  pending.resolve({
    response: { id: '2', result: { type: 'observed', data: observation('1') } },
    payloads: [],
  });
  const machineSource = page.getByRole('button', { name: 'untitled-1.s', exact: true });
  await machineSource.click();
  await expect.element(editor).toHaveTextContent('nop');
  await page
    .getByRole('button', {
      name: close_document_named({ name: 'untitled-1.s' }, { locale: 'en' }),
      exact: true,
    })
    .click();
  await page
    .getByRole('alertdialog')
    .getByRole('button', { name: en.close_document, exact: true })
    .click();
  await expect.element(editor).toHaveTextContent('yield');
  await expect.element(page.getByRole('button', { name: en.step, exact: true })).toBeEnabled();
  await expect.element(machineSource).not.toBeInTheDocument();
  expect(requests.filter((type) => type !== 'subscribe')).toEqual(['assemble', 'load']);
});

test('opening and closing an unreadable workspace preserves its recovery record', async () => {
  const stored = JSON.stringify({
    active: 'missing',
    documents: [{ ...scratch, name: 'retained.s' }],
  });
  localStorage.setItem('oplab.workspace.v1', stored);
  const view = await render(Workbench, { portFactory: () => null });
  await expect.element(page.getByRole('alert')).toBeVisible();
  await view.unmount();
  expect(localStorage.getItem('oplab.workspace.v1')).toBe(stored);
});

test('a quota failure preserves every saved document and the in-memory edit', async () => {
  const stored = JSON.stringify({
    active: scratch.documentId,
    documents: [
      { ...scratch, name: 'first.s', source: 'nop' },
      { ...scratch, documentId: 'second', name: 'second.s', source: 'yield' },
    ],
  });
  localStorage.setItem('oplab.workspace.v1', stored);
  const write = localStorage.setItem.bind(localStorage);
  const storage = vi.spyOn(Storage.prototype, 'setItem').mockImplementation((key, value) => {
    if (key === 'oplab.workspace.v1') throw new DOMException('Full', 'QuotaExceededError');
    write(key, value);
  });
  onTestFinished(() => {
    storage.mockRestore();
  });
  const view = await render(Workbench, { portFactory: () => null });
  const editor = page.getByRole('textbox', { name: en.editor_label });
  await expect.element(editor).toBeVisible();
  await editor.click();
  await userEvent.keyboard('{ControlOrMeta>}a{/ControlOrMeta}int3');
  await expect.element(page.getByRole('alert')).toBeVisible();
  await expect.element(editor).toHaveTextContent('int3');
  await view.unmount();
  expect(localStorage.getItem('oplab.workspace.v1')).toBe(stored);
  storage.mockRestore();
  await render(Workbench, { portFactory: () => null });
  await expect.element(editor).toHaveTextContent('nop');
  await page.getByRole('tab', { name: 'second.s', exact: true }).click();
  await expect.element(editor).toHaveTextContent('yield');
});

test('many documents remain reachable through the list, keyboard and recovery', async () => {
  const documents = Array.from({ length: 12 }, (_, index) => ({
    ...scratch,
    documentId: `source-${String(index)}`,
    name: `source-${String(index)}.s`,
    source: `mov eax, ${String(index)}`,
  }));
  localStorage.setItem('oplab.workspace.v1', JSON.stringify({ active: 'source-0', documents }));
  const view = await render(Workbench, { portFactory: () => null });
  const editor = page.getByRole('textbox', { name: en.editor_label });
  await expect.element(editor).toBeVisible();
  await expect.element(editor).toHaveTextContent('mov eax, 0');
  await page.getByRole('button', { name: en.new_document }).click();
  await page.getByRole('button', { name: en.rename_document }).click();
  await page.getByRole('textbox', { name: en.document_name }).fill('working.s');
  await userEvent.keyboard('{Enter}');
  await expect.poll(() => document.title).toContain('working.s');
  await page.getByRole('button', { name: en.source_documents }).click();
  await page.getByRole('menuitemradio', { name: 'source-11.s', exact: true }).click();
  await expect.element(editor).toHaveTextContent('mov eax, 11');
  await editor.click();
  await userEvent.keyboard('{Control>}{PageUp}{/Control}');
  await expect.element(editor).toHaveTextContent('mov eax, 10');
  await userEvent.keyboard('{Control>}{PageDown}{/Control}');
  await expect.element(editor).toHaveTextContent('mov eax, 11');
  await view.unmount();
  await render(Workbench, { portFactory: () => null });
  await expect.element(editor).toHaveTextContent('mov eax, 11');
  await page.getByRole('tab', { name: 'working.s', exact: true }).click();
  await expect.element(editor).not.toMatchTextContent('mov eax');
  await page
    .getByRole('button', {
      name: close_document_named({ name: 'working.s' }, { locale: 'en' }),
      exact: true,
    })
    .click();
  await expect.element(page.getByRole('alertdialog')).not.toBeInTheDocument();
  await expect
    .element(page.getByRole('tab', { name: 'working.s', exact: true }))
    .not.toBeInTheDocument();
  await expect.element(editor).toHaveTextContent('mov eax, 11');
});

test.each(['en', 'zh-CN'] as const)(
  '%s closes background tabs without selecting them or losing the active edit',
  async (locale) => {
    localStorage.setItem('PARAGLIDE_LOCALE', locale);
    const copy = locale === 'en' ? en : zh;
    localStorage.setItem(
      'oplab.workspace.v1',
      JSON.stringify({
        active: 'current',
        documents: [
          { ...scratch, documentId: 'background', name: 'background.s', source: 'nop' },
          { ...scratch, documentId: 'current', name: 'current.s', source: 'int3' },
          { ...scratch, documentId: 'empty', name: 'empty.s', source: '' },
        ],
      }),
    );
    await render(Workbench, { portFactory: () => null });
    const editor = page.getByRole('textbox', { name: copy.editor_label });
    const current = page.getByRole('tab', { name: 'current.s', exact: true });
    const background = page.getByRole('tab', { name: 'background.s', exact: true });
    const closeBackground = page.getByRole('button', {
      name: close_document_named({ name: 'background.s' }, { locale }),
      exact: true,
    });
    const dialog = page.getByRole('alertdialog', {
      name: close_document_named({ name: 'background.s' }, { locale }),
      exact: true,
    });
    await expect.element(editor).toHaveTextContent('int3');
    await editor.click();
    await userEvent.keyboard('{ControlOrMeta>}a{/ControlOrMeta}hlt');
    await closeBackground.click();
    await expect.element(dialog).toBeVisible();
    await dialog.getByRole('button', { name: copy.keep_document }).click();
    await expect.element(current).toHaveAttribute('aria-selected', 'true');
    await expect.element(closeBackground).toHaveFocus();
    await background.click({ button: 'middle' });
    await dialog.getByRole('button', { name: copy.close_document, exact: true }).click();
    await expect.element(dialog).not.toBeInTheDocument();
    await expect.element(background).not.toBeInTheDocument();
    await expect.element(current).toHaveAttribute('aria-selected', 'true');
    await expect.element(editor).toHaveTextContent('hlt');
    await editor.click();
    await userEvent.keyboard('{ControlOrMeta>}z{/ControlOrMeta}');
    await expect.element(editor).toHaveTextContent('int3');
    await page
      .getByRole('button', {
        name: close_document_named({ name: 'empty.s' }, { locale }),
        exact: true,
      })
      .click();
    await expect
      .element(page.getByRole('tab', { name: 'empty.s', exact: true }))
      .not.toBeInTheDocument();
    await expect.element(page.getByRole('alertdialog')).not.toBeInTheDocument();
    await expect.element(current).toHaveAttribute('aria-selected', 'true');
    await current.click();
    await userEvent.keyboard('{Delete}');
    await expect
      .element(
        page.getByRole('alertdialog', {
          name: close_document_named({ name: 'current.s' }, { locale }),
          exact: true,
        }),
      )
      .toBeVisible();
    await userEvent.keyboard('{Escape}');
    await expect.element(editor).toHaveTextContent('int3');
  },
);

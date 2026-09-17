import type { FilePort } from '$lib/desktop/files';
import FileMenu from '$lib/workbench/FileMenu.svelte';
import { settled } from 'svelte';
import { expect, test, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page, userEvent } from 'vitest/browser';

import en from '../../messages/en.json';

import '$lib/styles/theme.css';
import '$lib/workbench/workbench.css';

function props() {
  return {
    locale: 'en' as const,
    source: '\uFEFF// \u4e2d\u6587 \u{1f600}\r\nnop\r\n',
    object: undefined,
    image: undefined,
    binary: undefined,
    port: {
      open: vi.fn<FilePort['open']>().mockResolvedValue(null),
      save: vi.fn<FilePort['save']>().mockResolvedValue(true),
    },
    onsource: vi.fn<(source: string) => void>(),
    onbinary: vi.fn<(bytes: Uint8Array) => void>(),
    onerror: vi.fn<(code: string | null) => void>(),
  };
}

async function choose(name: string) {
  await page.getByRole('button', { name: en.files, exact: true }).click();
  await page.getByRole('menuitem', { name, exact: true }).click();
}

test('exports preserve the selected format and complete bytes', async () => {
  const input = {
    ...props(),
    binary: new Uint8Array([0, 255, 144]),
    object: new Uint8Array([127, 69, 76, 70, 1]),
    image: new Uint8Array([127, 69, 76, 70, 2]),
  };
  const view = await render(FileMenu, props());
  await page.getByRole('button', { name: en.files, exact: true }).click();
  for (const name of [en.export_binary, en.export_object, en.export_image])
    await expect
      .element(page.getByRole('menuitem', { name, exact: true }))
      .toHaveAttribute('aria-disabled', 'true');
  await userEvent.keyboard('{Escape}');
  await view.rerender(input);
  for (const [name, format, bytes] of [
    [en.export_source, 'source', new TextEncoder().encode(input.source)],
    [en.export_binary, 'binary', input.binary],
    [en.export_object, 'object', input.object],
    [en.export_image, 'image', input.image],
  ] as const) {
    await choose(name);
    await expect.poll(() => input.port.save).toHaveBeenLastCalledWith(format, name, bytes);
  }
  expect(input.onsource).not.toHaveBeenCalled();
  expect(input.onbinary).not.toHaveBeenCalled();
});

test('keyboard import handles cancellation and failure, then accepts a retry', async () => {
  const input = props();
  input.port.open
    .mockResolvedValueOnce(null)
    .mockRejectedValueOnce(new Error('Transport failed'))
    .mockResolvedValueOnce(new Uint8Array([0xff]))
    .mockResolvedValueOnce(new TextEncoder().encode(input.source));
  await render(FileMenu, input);
  for (const error of [null, 'file_read', 'file_encoding', null]) {
    await page.getByRole('button', { name: en.files, exact: true }).click();
    await userEvent.keyboard('{Home}{Enter}');
    await expect.element(page.getByRole('button', { name: en.files, exact: true })).toBeEnabled();
    expect(input.onerror).toHaveBeenLastCalledWith(error);
    if (error !== null) expect(input.onsource).not.toHaveBeenCalled();
  }
  expect(input.onsource).toHaveBeenCalledExactlyOnceWith(input.source);
  expect(input.onbinary).not.toHaveBeenCalled();
});

test('a pending import cannot overwrite edits', async () => {
  const input = props();
  const first = Promise.withResolvers<Uint8Array | null>();
  input.port.open.mockReturnValueOnce(first.promise);
  const view = await render(FileMenu, input);
  await choose(en.import_source);
  await expect.element(page.getByRole('button', { name: en.files, exact: true })).toBeDisabled();
  await view.rerender({ source: 'edited' });
  first.resolve(new TextEncoder().encode('replacement'));
  await expect.poll(() => input.onerror).toHaveBeenLastCalledWith('file_conflict');
  expect(input.onsource).not.toHaveBeenCalled();
});

test.each(['contents', 'failure'])('an unmounted file view ignores late %s', async (outcome) => {
  const input = props();
  const reply = Promise.withResolvers<Uint8Array | null>();
  input.port.open.mockReturnValueOnce(reply.promise);
  const view = await render(FileMenu, input);
  await choose(en.import_binary);
  await view.unmount();
  if (outcome === 'contents') reply.resolve(new Uint8Array([0xc3]));
  else reply.reject(new Error('Picker failed'));
  await reply.promise.catch(() => {});
  await settled();
  expect(input.onbinary).not.toHaveBeenCalled();
  expect(input.onerror).toHaveBeenLastCalledWith(null);
});

test('failed exports release the menu and preserve the next export payload', async () => {
  const input = props();
  input.port.save.mockRejectedValueOnce(new Error('Disk failed')).mockResolvedValueOnce(false);
  await render(FileMenu, input);
  await choose(en.export_source);
  await expect.poll(() => input.onerror).toHaveBeenLastCalledWith('file_write');
  await choose(en.export_source);
  await expect.element(page.getByRole('button', { name: en.files, exact: true })).toBeEnabled();
  expect(input.onerror).toHaveBeenLastCalledWith(null);
  expect(input.port.save).toHaveBeenLastCalledWith(
    'source',
    en.export_source,
    new TextEncoder().encode(input.source),
  );
});

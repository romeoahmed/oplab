import en from '../../../messages/en.json';
import zh from '../../../messages/zh-CN.json';
import { expect, test, vi } from 'vitest';
import { page, userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';
import Instructions from '$lib/workbench/instructions/Instructions.svelte';
import type { ComponentProps } from 'svelte';
import type { DecodedInstruction } from '$lib/protocol/generated/DecodedInstruction';
import '$lib/styles/theme.css';

type Props = ComponentProps<typeof Instructions>;
const initial = {
  bytes: new Uint8Array([0x48, 0x83, 0xc0, 1, 0xc3]),
  target: 'x86_64',
  base: '0x1000',
  connected: true,
  locale: 'en',
} satisfies Omit<Props, 'decode'>;
const add = { address: '0x0000000000001000', bytes: [0x48, 0x83, 0xc0, 1], text: 'add rax, 1' };
const ret = { address: '0x0000000000001004', bytes: [0xc3], text: 'ret' };

test('pagination advances by instruction byte lengths, not row count or edited offset', async () => {
  const decode = vi.fn<Props['decode']>().mockResolvedValueOnce([add]).mockResolvedValueOnce([ret]);
  const view = await render(Instructions, { props: { ...initial, decode } });
  await page.getByRole('button', { name: en.disassemble, exact: true }).click();
  await expect.element(page.getByRole('table')).toMatchTextContent('add rax, 1');
  await page.getByRole('spinbutton', { name: en.byte_offset }).fill('2');
  await page.getByRole('button', { name: en.next_instructions }).click();
  await expect.element(page.getByRole('table')).toMatchTextContent('ret');
  expect(decode).toHaveBeenLastCalledWith(new Uint8Array([0xc3]), 'x86_64', ret.address);
  await expect.element(page.getByRole('button', { name: en.next_instructions })).toBeDisabled();
  await view.rerender({ locale: 'zh-CN' });
  await expect
    .element(page.getByRole('table', { name: zh.instructions }))
    .toMatchTextContent('ret');
  expect(decode).toHaveBeenCalledTimes(2);
});

test('invalid offsets never reach the decoder and a corrected offset can be submitted', async () => {
  const decode = vi.fn<Props['decode']>().mockResolvedValue([ret]);
  await render(Instructions, { props: { ...initial, decode } });
  const offset = page.getByRole('spinbutton', { name: en.byte_offset });
  for (const invalid of ['', '-1', '0.5', '5']) {
    await offset.fill(invalid);
    await page.getByRole('button', { name: en.disassemble, exact: true }).click();
    expect(decode).not.toHaveBeenCalled();
  }
  await offset.fill('4');
  await userEvent.keyboard('{Enter}');
  await expect.element(page.getByRole('table')).toMatchTextContent('ret');
  expect(decode).toHaveBeenCalledExactlyOnceWith(new Uint8Array([0xc3]), 'x86_64', ret.address);
});

test.each([
  { name: 'bytes', change: { bytes: new Uint8Array([0xc3]) } },
  { name: 'architecture', change: { target: 'aarch64' } },
  { name: 'base', change: { base: '0x2000' } },
  { name: 'connection', change: { connected: false } },
] satisfies { name: string; change: Partial<Props> }[])(
  'a changed $name rejects pending success and failure results',
  async ({ change }) => {
    for (const fails of [false, true]) {
      const reply = Promise.withResolvers<DecodedInstruction[]>();
      const view = await render(Instructions, {
        props: { ...initial, decode: () => reply.promise },
      });
      await page.getByRole('button', { name: en.disassemble, exact: true }).click();
      await view.rerender(change);
      if (fails) reply.reject(new Error('Old request failed'));
      else reply.resolve([add]);
      await reply.promise.catch(() => {});
      await view.rerender({ locale: 'zh-CN' });
      await expect.element(page.getByRole('table')).not.toBeInTheDocument();
      await expect.element(page.getByRole('alert')).not.toBeInTheDocument();
      await view.unmount();
    }
  },
);

test.each([
  { failure: { code: 'decode', address: ret.address }, message: en.error_decode },
  {
    failure: { code: 'deadline', address: null },
    message: en.error_deadline,
  },
])('inspection reports $failure.code and recovers on retry', async ({ failure, message }) => {
  const decode = vi
    .fn<Props['decode']>()
    .mockRejectedValueOnce(Object.assign(new Error(failure.code), failure))
    .mockResolvedValueOnce([add]);
  await render(Instructions, { props: { ...initial, decode } });
  const disassemble = page.getByRole('button', { name: en.disassemble, exact: true });
  await disassemble.click();
  await expect.element(page.getByRole('alert')).toMatchTextContent(message);
  if (failure.address !== null)
    await expect.element(page.getByRole('alert')).toMatchTextContent(failure.address);
  await disassemble.click();
  await expect.element(page.getByRole('table')).toMatchTextContent('add rax, 1');
  await expect.element(page.getByRole('alert')).not.toBeInTheDocument();
});

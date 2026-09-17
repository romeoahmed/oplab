import type { DecodedInstruction } from '$lib/protocol/generated/DecodedInstruction';
import type { InstructionAnalysis } from '$lib/protocol/generated/InstructionAnalysis';
import Instructions from '$lib/workbench/instructions/Instructions.svelte';
import { settled, type ComponentProps } from 'svelte';
import { expect, test, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page, userEvent } from 'vitest/browser';

import en from '../../../messages/en.json';
import zh from '../../../messages/zh-CN.json';
import { nopAnalysis } from '../../fixtures/protocol';

import '$lib/styles/theme.css';

type Props = ComponentProps<typeof Instructions>;
const initial = {
  bytes: new Uint8Array([0x48, 0x83, 0xc0, 1, 0xc3]),
  target: 'x86_64',
  base: '0x1000',
  connected: true,
  locale: 'en',
  analyze: () => Promise.reject(new Error('Unexpected analysis request')),
} satisfies Omit<Props, 'decode'>;
const add = { address: '0x0000000000001000', bytes: [0x48, 0x83, 0xc0, 1], text: 'add rax, 1' };
const ret = { address: '0x0000000000001004', bytes: [0xc3], text: 'ret' };
const addAnalysis: InstructionAnalysis = {
  ...nopAnalysis,
  registers: [{ name: 'rax', access: 'read_write' }],
  architecture: {
    type: 'x86',
    data: {
      ...nopAnalysis.architecture.data,
      flags: {
        ...nopAnalysis.architecture.data.flags,
        written: ['OF', 'SF', 'ZF', 'AF', 'CF', 'PF'],
      },
    },
  },
};

test.each(
  (['success', 'failure'] as const).flatMap((outcome) =>
    (['older', 'newer'] as const).map((first) => ({ outcome, first })),
  ),
)(
  'source navigation ignores an older $outcome when $first decode settles first',
  async ({ outcome, first }) => {
    const older = Promise.withResolvers<DecodedInstruction[]>();
    const newer = Promise.withResolvers<DecodedInstruction[]>();
    const decode = vi
      .fn<Props['decode']>()
      .mockReturnValueOnce(older.promise)
      .mockReturnValueOnce(newer.promise);
    const view = await render(Instructions, { props: { ...initial, decode } });
    const component = view.component as { reveal: (address: string) => Promise<void> };
    const previous = component.reveal(add.address);
    const latest = component.reveal(ret.address);
    expect(decode).toHaveBeenLastCalledWith(new Uint8Array(ret.bytes), initial.target, ret.address);
    const disassemble = page.getByRole('button', { name: en.disassemble, exact: true });
    const table = page.getByRole('table', { name: en.instructions, exact: true });
    if (first === 'newer') {
      newer.resolve([ret]);
      await latest;
      await expect.element(table).toMatchTextContent(ret.text);
    }
    if (outcome === 'failure') older.reject(new Error('Superseded decode'));
    else older.resolve([add]);
    await previous;
    await settled();
    if (first === 'older') {
      await expect.element(disassemble).toBeDisabled();
      await expect.element(table).not.toBeInTheDocument();
      await expect.element(page.getByRole('alert')).not.toBeInTheDocument();
      newer.resolve([ret]);
      await latest;
    }
    await expect.element(table).toMatchTextContent(ret.text);
    await expect.element(table).not.toMatchTextContent(add.text);
    await expect.element(page.getByRole('alert')).not.toBeInTheDocument();
    await expect.element(disassemble).toBeEnabled();
  },
);

test('analysis survives locale changes but expires when its input is replaced', async () => {
  const analyze = vi.fn<Props['analyze']>().mockResolvedValue(addAnalysis);
  const view = await render(Instructions, {
    props: { ...initial, analyze, decode: vi.fn<Props['decode']>().mockResolvedValue([add, ret]) },
  });
  view.container.style.cssText = 'width: 1000px; height: 400px';
  await page.getByRole('button', { name: en.disassemble, exact: true }).click();
  await expect.element(page.getByRole('button', { name: add.text, exact: true })).toBeVisible();
  expect(analyze).not.toHaveBeenCalled();
  await page.getByRole('button', { name: add.text, exact: true }).click();
  const panel = page.getByRole('region', { name: en.instruction_analysis });
  await expect.element(panel).toMatchTextContent('rax');
  await expect.element(panel).toMatchTextContent('ZF');
  expect(analyze).toHaveBeenCalledExactlyOnceWith(
    new Uint8Array(add.bytes),
    initial.target,
    add.address,
  );
  await view.rerender({ locale: 'zh-CN' });
  await expect
    .element(page.getByRole('region', { name: zh.instruction_analysis }))
    .toMatchTextContent('rax');
  expect(analyze).toHaveBeenCalledTimes(1);
  await view.rerender({ bytes: new Uint8Array([0x90]) });
  await expect
    .element(page.getByRole('region', { name: zh.instruction_analysis }))
    .not.toBeInTheDocument();
  await view.rerender({ bytes: initial.bytes });
  await expect.element(page.getByRole('table')).not.toBeInTheDocument();
  await expect
    .element(page.getByRole('region', { name: zh.instruction_analysis }))
    .not.toBeInTheDocument();
});

test('a narrow panel returns from analysis to its decoded page without another request', async () => {
  const decode = vi.fn<Props['decode']>().mockResolvedValue([add, ret]);
  const analyze = vi.fn<Props['analyze']>().mockResolvedValue(addAnalysis);
  const view = await render(Instructions, { props: { ...initial, decode, analyze } });
  view.container.style.cssText = 'width: 600px; height: 240px';
  const disassemble = page.getByRole('button', {
    name: en.disassemble,
    exact: true,
    includeHidden: true,
  });
  await disassemble.click();
  const table = page.getByRole('table', {
    name: en.instructions,
    exact: true,
    includeHidden: true,
  });
  await page.getByRole('button', { name: add.text, exact: true }).click();
  const panel = page.getByRole('region', { name: en.instruction_analysis });
  await expect.element(panel).toMatchTextContent('rax');
  await expect.element(table).not.toBeVisible();
  await expect.element(disassemble).not.toBeVisible();
  await panel.getByRole('button', { name: en.close, exact: true }).click();
  await expect.element(panel).not.toBeInTheDocument();
  await expect.element(page.getByRole('table')).toMatchTextContent(ret.text);
  await expect.element(disassemble).toBeVisible();
  expect(decode).toHaveBeenCalledTimes(1);
  expect(analyze).toHaveBeenCalledTimes(1);
});

test.each(['success', 'failure'] as const)(
  'late analysis %s cannot replace a newer selection',
  async (outcome) => {
    const pending = Promise.withResolvers<InstructionAnalysis>();
    const returnAnalysis: InstructionAnalysis = {
      ...nopAnalysis,
      registers: [{ name: 'rsp', access: 'read_write' }],
      memory: [{ access: 'read', bytes: 8 }],
      architecture: { type: 'x86', data: { ...nopAnalysis.architecture.data, flow: 'return' } },
    };
    const analyze = vi
      .fn<Props['analyze']>()
      .mockReturnValueOnce(pending.promise)
      .mockResolvedValue(returnAnalysis);
    const view = await render(Instructions, {
      props: {
        ...initial,
        analyze,
        decode: vi.fn<Props['decode']>().mockResolvedValue([add, ret]),
      },
    });
    view.container.style.cssText = 'width: 1000px; height: 400px';
    await page.getByRole('button', { name: en.disassemble, exact: true }).click();
    await page.getByRole('button', { name: add.text, exact: true }).click();
    await page.getByRole('button', { name: ret.text, exact: true }).click();
    const panel = page.getByRole('region', { name: en.instruction_analysis });
    await expect.element(panel).toMatchTextContent('rsp');
    if (outcome === 'failure') pending.reject(new Error('stale analysis'));
    else pending.resolve(addAnalysis);
    await pending.promise.catch(() => {});
    await settled();
    await expect.element(panel).toMatchTextContent('rsp');
    await expect.element(panel).not.toMatchTextContent('rax');
    await expect.element(page.getByRole('alert')).not.toBeInTheDocument();
  },
);

test('a failed analysis can retry without decoding again and reports unknown memory width', async () => {
  const decode = vi
    .fn<Props['decode']>()
    .mockResolvedValue([
      { address: add.address, text: 'str x0, [x1, #8]!', bytes: [0x20, 0x8c, 0, 0xf8] },
    ]);
  const analysis: InstructionAnalysis = {
    registers: [
      { name: 'x0', access: 'read' },
      { name: 'x1', access: 'read_write' },
    ],
    memory: [{ access: 'unknown', bytes: null }],
    branch_target: null,
    architecture: { type: 'aarch64', data: { groups: [], updates_flags: false, writeback: true } },
  };
  const analyze = vi
    .fn<Props['analyze']>()
    .mockRejectedValueOnce({ code: 'decode' })
    .mockResolvedValue(analysis);
  await render(Instructions, { props: { ...initial, target: 'aarch64', decode, analyze } });
  await page.getByRole('button', { name: en.disassemble, exact: true }).click();
  await page.getByRole('button', { name: 'str x0, [x1, #8]!', exact: true }).click();
  await expect.element(page.getByRole('alert')).toMatchTextContent(en.error_decode);
  const retry = page.getByRole('button', { name: en.analysis_retry });
  await retry.click();
  const panel = page.getByRole('region', { name: en.instruction_analysis });
  await expect.element(panel).toMatchTextContent('x1');
  await expect.element(panel).toMatchTextContent(en.analysis_unknown_size);
  await expect.element(page.getByRole('alert')).not.toBeInTheDocument();
  expect(decode).toHaveBeenCalledTimes(1);
  expect(analyze).toHaveBeenCalledTimes(2);
});

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

const inputChanges = [
  { name: 'bytes', change: { bytes: new Uint8Array([0xc3]) } },
  { name: 'architecture', change: { target: 'aarch64' } },
  { name: 'base', change: { base: '0x2000' } },
  { name: 'connection', change: { connected: false } },
] satisfies { name: string; change: Partial<Props> }[];

test.each(
  inputChanges.flatMap((input) =>
    (['success', 'failure'] as const).map((outcome) => ({ ...input, outcome })),
  ),
)('a changed $name invalidates decode $outcome even when restored', async ({ change, outcome }) => {
  const reply = Promise.withResolvers<DecodedInstruction[]>();
  const view = await render(Instructions, {
    props: { ...initial, decode: () => reply.promise },
  });
  const disassemble = page.getByRole('button', { name: en.disassemble, exact: true });
  await disassemble.click();
  await view.rerender(change);
  await view.rerender(initial);
  if (outcome === 'failure') reply.reject(new Error('Old request failed'));
  else reply.resolve([add]);
  await reply.promise.catch(() => {});
  await settled();
  await expect.element(disassemble).toBeEnabled();
  await expect.element(page.getByRole('table')).not.toBeInTheDocument();
  await expect.element(page.getByRole('alert')).not.toBeInTheDocument();
});

test.each(['success', 'failure'] as const)(
  'input replacement discards pending analysis %s even when restored',
  async (outcome) => {
    const reply = Promise.withResolvers<InstructionAnalysis>();
    const decode = vi.fn<Props['decode']>().mockResolvedValue([add, ret]);
    const view = await render(Instructions, {
      props: { ...initial, decode, analyze: () => reply.promise },
    });
    const disassemble = page.getByRole('button', { name: en.disassemble, exact: true });
    await disassemble.click();
    await page.getByRole('button', { name: add.text, exact: true }).click();
    await expect.element(page.getByRole('status')).toBeVisible();
    await view.rerender({ bytes: new Uint8Array([0xc3]) });
    await view.rerender({ bytes: initial.bytes });
    if (outcome === 'failure') reply.reject(new Error('Old analysis failed'));
    else reply.resolve(addAnalysis);
    await reply.promise.catch(() => {});
    await settled();
    await expect
      .element(page.getByRole('region', { name: en.instruction_analysis }))
      .not.toBeInTheDocument();
    await expect.element(page.getByRole('alert')).not.toBeInTheDocument();
    await expect.element(page.getByRole('table')).not.toBeInTheDocument();
    await disassemble.click();
    await expect.element(page.getByRole('button', { name: ret.text, exact: true })).toBeVisible();
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

import Machine from '$lib/workbench/machine/Machine.svelte';
import { expect, test } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page, userEvent } from 'vitest/browser';

import en from '../../../messages/en.json';
import zh from '../../../messages/zh-CN.json';
import { observation } from '../../fixtures/protocol';

import '$lib/styles/theme.css';
import '$lib/workbench/workbench.css';

test.each([
  { target: 'x86_64', locale: 'en' },
  { target: 'aarch64', locale: 'zh-CN' },
] as const)(
  '$target SIMD views retain format, update values and clear lost state in $locale',
  async ({ target, locale }) => {
    const copy = locale === 'en' ? en : zh;
    const current = observation('1', target);
    if (current.registers === null) throw new Error('Missing fixture bank');
    const vectors =
      current.registers.type === 'x86_64' ? current.registers.data.xmm : current.registers.data.v;
    vectors[0] = '0x7fc123457f800000800000003fc00000';
    const view = await render(Machine, {
      observation: current,
      loadedCurrent: true,
      loadedRevision: '1',
      loadedKind: 'source',
      editable: true,
      onbreakpoint: () => {},
      onregister: () => {},
      locale,
    });
    const integerTab = page.getByRole('tab', { name: copy.integer_view, exact: true });
    await integerTab.click();
    await userEvent.keyboard('{ArrowRight}');
    const format = page.getByRole('combobox', { name: copy.vector_format });
    await format.selectOptions('f32');
    await expect.element(page.getByText('1.5', { exact: true })).toBeVisible();
    await expect.element(page.getByText('-0', { exact: true })).toBeVisible();
    await expect.element(page.getByText('NaN', { exact: true })).toBeVisible();
    await expect
      .element(page.getByText('7fc123457f800000800000003fc00000', { exact: true }))
      .toBeVisible();
    await page.getByRole('tab', { name: copy.breakpoint_view, exact: false }).click();
    const address = page.getByRole('textbox', { name: copy.breakpoint_address });
    await address.fill('0x1000');
    await page.getByRole('tab', { name: 'SIMD', exact: true }).click();
    await expect.element(format).toHaveValue('f32');
    await userEvent.keyboard('{ArrowRight}');
    await expect.element(address).toHaveValue('0x1000');
    await userEvent.keyboard('{ArrowLeft}');
    await expect.element(format).toHaveValue('f32');
    const next = observation('2', target);
    if (next.registers === null) throw new Error('Missing fixture bank');
    const updated =
      next.registers.type === 'x86_64' ? next.registers.data.xmm : next.registers.data.v;
    updated[0] = '0x00000000000000000000000042280000';
    await view.rerender({ observation: next });
    await expect.element(page.getByText('42', { exact: true })).toBeVisible();
    await expect
      .element(page.getByText('00000000000000000000000042280000', { exact: true }))
      .toBeVisible();
    await expect.element(page.getByText('NaN', { exact: true })).not.toBeInTheDocument();
    await expect.element(format).toHaveValue('f32');
    await view.rerender({ observation: { ...next, registers: null, status: { type: 'crashed' } } });
    await expect.element(format).not.toBeInTheDocument();
    await expect
      .element(page.getByText('00000000000000000000000042280000', { exact: true }))
      .not.toBeInTheDocument();
  },
);

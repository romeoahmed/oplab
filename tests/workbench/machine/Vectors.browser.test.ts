import type { VectorWrite } from '$lib/protocol/generated/VectorWrite';
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
      current.registers.type === 'x86_64' ? current.registers.data.ymm : current.registers.data.z;
    vectors[0] =
      '0x' + '7fc123457f800000800000003fc00000'.padStart(target === 'x86_64' ? 64 : 512, '0');
    const view = await render(Machine, {
      observation: current,
      loadedCurrent: true,
      loadedRevision: '1',
      loadedKind: 'source',
      editable: true,
      onbreakpoint: () => {},
      onregister: () => {},
      onvector: () => {},
      onrounding: () => {},
      locale,
    });
    const integerTab = page.getByRole('tab', { name: copy.integer_view, exact: true });
    await integerTab.click();
    await userEvent.keyboard('{ArrowRight}');
    await page.getByRole('combobox', { name: copy.vector_bank }).selectOptions('low');
    await page.getByText(target === 'x86_64' ? 'XMM0' : 'V0', { exact: true }).click();
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
      next.registers.type === 'x86_64' ? next.registers.data.ymm : next.registers.data.z;
    updated[0] =
      '0x' + '00000000000000000000000042280000'.padStart(target === 'x86_64' ? 64 : 512, '0');
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

test.each([
  { target: 'x86_64', locale: 'en', name: 'xmm15', index: '15' },
  { target: 'aarch64', locale: 'zh-CN', name: 'v31', index: '31' },
] as const)(
  'SIMD editing validates lanes and preserves drafts in $locale',
  async ({ target, locale, name, index }) => {
    const copy = locale === 'en' ? en : zh;
    const writes: VectorWrite[] = [];
    const view = await render(Machine, {
      observation: observation('1', target),
      loadedCurrent: true,
      loadedRevision: '1',
      loadedKind: 'source',
      editable: true,
      onbreakpoint: () => {},
      onregister: () => {},
      onrounding: () => {},
      onvector: (write: VectorWrite) => {
        writes.push(write);
      },
      locale,
    });
    await page.getByRole('tab', { name: 'SIMD', exact: true }).click();
    await page.getByRole('combobox', { name: copy.vector_bank }).selectOptions('low');
    const edit = page.getByRole('button', { name: copy.edit_vector });
    await edit.click();
    const popover = page.getByRole('form', { name: copy.edit_vector });
    const register = popover.getByRole('combobox', { name: copy.register_name });
    const format = popover.getByRole('combobox', { name: copy.vector_format });
    const input = popover.getByRole('textbox', { name: copy.register_value });
    const submit = popover.getByRole('button', { name: copy.write_register });
    await register.selectOptions(index);
    await format.selectOptions('i64');
    await popover.getByRole('combobox', { name: copy.vector_lane }).selectOptions('1');
    await input.click();
    await expect.element(input).toHaveFocus();
    await input.fill('-9223372036854775809');
    await userEvent.keyboard('{Enter}');
    await expect.element(input).toBeInvalid();
    expect(writes).toEqual([]);
    await input.fill('-9223372036854775808');
    await view.rerender({ observation: observation('2', target) });
    await expect.element(input).toHaveValue('-9223372036854775808');
    await userEvent.keyboard('{Enter}');
    expect(writes).toEqual([{ name, width: 64, lane: 1, value: '0x8000000000000000' }]);
    await format.selectOptions('f32');
    await expect
      .element(popover.getByRole('combobox', { name: copy.vector_lane }))
      .toHaveValue('0');
    await input.fill('-0');
    await userEvent.keyboard('{Enter}');
    expect(writes.at(-1)).toEqual({
      name,
      width: 32,
      lane: 0,
      value: '0x80000000',
    });
    await format.selectOptions('hex');
    await expect
      .element(popover.getByRole('combobox', { name: copy.vector_lane }))
      .not.toBeInTheDocument();
    await input.fill('7fc123457f800000800000003fc00000');
    await userEvent.keyboard('{Enter}');
    expect(writes.at(-1)).toEqual({
      name,
      width: 128,
      lane: 0,
      value: '0x7fc123457f800000800000003fc00000',
    });
    await view.rerender({ editable: false });
    await expect.element(submit).toBeDisabled();
    await userEvent.keyboard('{Enter}');
    expect(writes).toHaveLength(3);
    await view.rerender({ editable: true });
    await userEvent.keyboard('{Escape}');
    await expect.element(edit).toHaveFocus();
  },
);

test.each([
  { target: 'x86_64', locale: 'en', register: 'ymm15', index: '15', bytes: 32 },
  { target: 'aarch64', locale: 'zh-CN', register: 'z31', index: '31', bytes: 256 },
] as const)(
  'full $target registers expose and edit their highest lane',
  async ({ target, locale, register, index, bytes }) => {
    const copy = locale === 'en' ? en : zh;
    const current = observation('1', target);
    if (current.registers === null) throw new Error('Missing bank');
    const values =
      current.registers.type === 'x86_64' ? current.registers.data.ymm : current.registers.data.z;
    const raw = 'ab' + '00'.repeat(bytes - 1);
    values[Number(index)] = `0x${raw}`;
    const writes: VectorWrite[] = [];
    await render(Machine, {
      observation: current,
      loadedCurrent: true,
      loadedRevision: '1',
      loadedKind: 'source',
      editable: true,
      onbreakpoint: () => {},
      onregister: () => {},
      onrounding: () => {},
      onvector: (write: VectorWrite) => {
        writes.push(write);
      },
      locale,
    });
    await page.getByRole('tab', { name: 'SIMD', exact: true }).click();
    await page.getByText(register.toUpperCase(), { exact: true }).click();
    await expect.element(page.getByText(raw, { exact: true })).toBeVisible();
    await page.getByRole('button', { name: copy.edit_vector }).click();
    const form = page.getByRole('form', { name: copy.edit_vector });
    await form.getByRole('combobox', { name: copy.register_name }).selectOptions(index);
    const input = form.getByRole('textbox', { name: copy.register_value });
    await expect.element(input).toHaveValue(raw);
    await form.getByRole('combobox', { name: copy.vector_format }).selectOptions('u8');
    await form.getByRole('combobox', { name: copy.vector_lane }).selectOptions(String(bytes - 1));
    await expect.element(input).toHaveValue('171');
    await input.fill('255');
    await input.click();
    await userEvent.keyboard('{Enter}');
    expect(writes).toEqual([{ name: register, width: 8, lane: bytes - 1, value: '0xff' }]);
    await userEvent.keyboard('{Escape}');
    if (target === 'aarch64') {
      await page.getByRole('combobox', { name: copy.vector_bank }).selectOptions('predicates');
      await page.getByText('FFR', { exact: true }).click();
      for (const stride of [1, 2, 4, 8]) {
        await page
          .getByRole('combobox', { name: copy.vector_element })
          .selectOptions(String(stride));
        // Native ordinals identify predicate bits, not positions in the filtered view.
        const indices = page
          .getByRole('listitem')
          .elements()
          .map((lane) => {
            if (!(lane instanceof HTMLLIElement)) throw new Error('Missing native list ordinal');
            return lane.value;
          });
        expect(indices).toEqual(Array.from({ length: 256 / stride }, (_, index) => index * stride));
      }
      await page.getByRole('button', { name: copy.edit_vector }).click();
      await form.getByRole('combobox', { name: copy.register_name }).selectOptions('16');
      await form.getByRole('combobox', { name: copy.vector_format }).selectOptions('bit');
      await form.getByRole('combobox', { name: copy.vector_lane }).selectOptions('255');
      await input.fill('1');
      await input.click();
      await userEvent.keyboard('{Enter}');
      expect(writes.at(-1)).toEqual({ name: 'ffr', width: 1, lane: 255, value: '0x01' });
    }
  },
);

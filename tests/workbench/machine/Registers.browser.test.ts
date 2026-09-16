import Registers from '$lib/workbench/machine/Registers.svelte';
import { settled } from 'svelte';
import { expect, test, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page, userEvent } from 'vitest/browser';

import en from '../../../messages/en.json';
import zh from '../../../messages/zh-CN.json';
import { observation } from '../../fixtures/protocol';

import '$lib/styles/theme.css';

test.each([
  { locale: 'en', target: 'x86_64', alias: 'ah', pc: 'rip', flag: 'zf' },
  { locale: 'zh-CN', target: 'aarch64', alias: 'w30', pc: 'pc', flag: 'z' },
] as const)(
  '$target editing applies the selected input constraints in $locale',
  async ({ locale, target, alias, pc, flag }) => {
    const copy = locale === 'en' ? en : zh;
    const onwrite = vi.fn<(name: string, value: string) => void>();
    const view = await render(Registers, {
      bank: observation('1', target).registers,
      locale,
      editable: true,
      onwrite,
    });
    const edit = page.getByRole('button', { name: copy.edit_register, exact: true });
    await edit.click();
    const selector = page.getByRole('combobox', { name: copy.register_name });
    const input = page.getByRole('textbox', { name: copy.register_value });
    const submit = page.getByRole('button', { name: copy.write_register, exact: true });
    await selector.selectOptions(flag);
    for (const invalid of ['', '2', '-1', '0x1']) {
      await input.click();
      await input.fill(invalid);
      await expect.element(input).toHaveFocus();
      await userEvent.keyboard('{Enter}');
      await expect.element(input).toBeInvalid();
    }
    expect(onwrite).not.toHaveBeenCalled();

    for (const [name, value] of [
      [flag, '1'],
      [flag, '0'],
      [alias, '0xff'],
      [pc, '0x1004'],
    ] as const) {
      await selector.selectOptions(name);
      await input.fill(value);
      await userEvent.keyboard('{Enter}');
      expect(onwrite).toHaveBeenLastCalledWith(name, value);
      await expect.element(selector).toHaveValue(name);
    }
    expect(onwrite).toHaveBeenCalledTimes(4);

    await view.rerender({ editable: false });
    await expect.element(submit).toBeDisabled();
    await input.fill('0x1008');
    await userEvent.keyboard('{Enter}');
    await settled();
    expect(onwrite).toHaveBeenCalledTimes(4);
    await view.rerender({ editable: true });
    await userEvent.keyboard('{Escape}');
    await expect.element(edit).toHaveFocus();
  },
);

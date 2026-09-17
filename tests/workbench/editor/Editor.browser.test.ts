import Editor from '$lib/workbench/editor/Editor.svelte';
import { expect, test, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page, userEvent } from 'vitest/browser';

import en from '../../../messages/en.json';
import zh from '../../../messages/zh-CN.json';

import '$lib/styles/theme.css';
import '$lib/workbench/workbench.css';

test.each(['en', 'zh-CN'] as const)(
  '%s editor exposes search, folding and label navigation without changing source',
  async (locale) => {
    const copy = locale === 'en' ? en : zh;
    const source = '.macro increment reg\nadd \\reg, 1\n.endm\nanswer: nop\njmp answer';
    const onchange = vi.fn();
    const oncursor = vi.fn();
    await render(Editor, {
      props: {
        value: source,
        locale,
        target: 'x86_64',
        wrap: false,
        diagnostic: null,
        onchange,
        oncursor,
      },
    });
    const editor = page.getByRole('textbox', { name: copy.editor_label });
    await editor.click();
    await userEvent.keyboard('{ControlOrMeta>}{End}{/ControlOrMeta}{F12}');
    await expect.poll(() => oncursor.mock.lastCall).toEqual([4, 7]);
    await page.getByRole('button', { name: copy.editor_actions }).click();
    await page.getByRole('menuitem', { name: copy.fold_all }).click();
    await expect.element(page.getByLabelText(copy.folded_code)).toBeVisible();
    await page.getByRole('button', { name: copy.editor_actions }).click();
    await page.getByRole('menuitem', { name: copy.unfold_all }).click();
    await expect.element(page.getByLabelText(copy.folded_code)).not.toBeInTheDocument();
    await page.getByRole('button', { name: copy.find, exact: true }).click();
    await expect.element(page.getByRole('textbox', { name: copy.find, exact: true })).toBeVisible();
    await userEvent.keyboard('answer');
    await expect
      .element(page.getByRole('textbox', { name: copy.find, exact: true }))
      .toHaveValue('answer');
    await page.getByRole('textbox', { name: copy.find, exact: true }).fill('\\reg');
    await page.getByRole('button', { name: copy.next, exact: true }).click();
    await expect.poll(() => oncursor.mock.lastCall).toEqual([2, 9]);
    expect(onchange).not.toHaveBeenCalled();
  },
);

test.each(['en', 'zh-CN'] as const)(
  '%s compact search validates regex, replaces exact matches and keeps undo',
  async (locale) => {
    const copy = locale === 'en' ? en : zh;
    const onchange = vi.fn();
    await render(Editor, {
      props: {
        value: 'mov rax, 1\nmov rbx, 2',
        locale,
        target: 'x86_64',
        wrap: false,
        diagnostic: null,
        onchange,
        oncursor: () => {},
      },
    });
    const editor = page.getByRole('textbox', { name: copy.editor_label });
    await page.getByRole('button', { name: copy.find, exact: true }).click();
    const find = page.getByRole('textbox', { name: copy.find, exact: true });
    await expect.element(find).toHaveFocus();
    await find.fill('missing');
    await expect.element(page.getByRole('button', { name: copy.next, exact: true })).toBeDisabled();
    await expect.element(find).toHaveAttribute('aria-invalid', 'false');
    await find.fill('[');
    await page.getByRole('button', { name: copy.regexp, exact: true }).click();
    await expect.element(find).toHaveAttribute('aria-invalid', 'true');
    await expect.element(page.getByRole('button', { name: copy.next, exact: true })).toBeDisabled();
    await find.fill('r(ax|bx)');
    await page.getByRole('button', { name: copy.toggle_replace }).click();
    await page.getByRole('textbox', { name: copy.replace, exact: true }).fill('r10');
    await page.getByRole('button', { name: copy.replace_all, exact: true }).click();
    await expect.poll(() => onchange.mock.lastCall).toEqual(['mov r10, 1\nmov r10, 2']);
    await expect
      .element(page.getByRole('button', { name: copy.replace_all, exact: true }))
      .toBeDisabled();
    await page.getByRole('button', { name: copy.close, exact: true }).click();
    await editor.click();
    await userEvent.keyboard('{ControlOrMeta>}z{/ControlOrMeta}');
    await expect.poll(() => onchange.mock.lastCall).toEqual(['mov rax, 1\nmov rbx, 2']);
  },
);

test.each(['en', 'zh-CN'] as const)(
  '%s search options and keyboard replacement use the current query',
  async (locale) => {
    const copy = locale === 'en' ? en : zh;
    const source = 'rax RAX rax64';
    const onchange = vi.fn();
    const oncursor = vi.fn();
    await render(Editor, {
      props: {
        value: source,
        locale,
        target: 'x86_64',
        wrap: false,
        diagnostic: null,
        onchange,
        oncursor,
      },
    });
    await page.getByRole('button', { name: copy.find, exact: true }).click();
    const find = page.getByRole('textbox', { name: copy.find, exact: true });
    await find.fill('rax');
    await page.getByRole('button', { name: copy.whole_word, exact: true }).click();
    await page.getByRole('button', { name: copy.match_case, exact: true }).click();
    await find.click();
    await userEvent.keyboard('{Enter}');
    await expect.poll(() => oncursor.mock.lastCall).toEqual([1, 4]);
    await page.getByRole('button', { name: copy.toggle_replace }).click();
    const replace = page.getByRole('textbox', { name: copy.replace, exact: true });
    await replace.fill('rbx');
    await userEvent.keyboard('{Enter}');
    await expect.poll(() => onchange.mock.lastCall).toEqual(['rbx RAX rax64']);
    await expect.element(page.getByRole('button', { name: copy.next, exact: true })).toBeDisabled();
    await page.getByRole('button', { name: copy.match_case, exact: true }).click();
    await find.click();
    await userEvent.keyboard('{Shift>}{Enter}{/Shift}');
    await expect.poll(() => oncursor.mock.lastCall).toEqual([1, 8]);
    await replace.click();
    await userEvent.keyboard('{Enter}');
    await expect.poll(() => onchange.mock.lastCall).toEqual(['rbx rbx rax64']);
    await userEvent.keyboard('{Escape}');
    await expect.element(page.getByRole('textbox', { name: copy.editor_label })).toHaveFocus();
    await page.getByRole('button', { name: copy.find, exact: true }).click();
    await expect.element(find).toHaveValue('rbx');
    await expect
      .element(page.getByRole('button', { name: copy.whole_word, exact: true }))
      .toHaveAttribute('aria-pressed', 'true');
  },
);

test.each(['en', 'zh-CN'] as const)(
  '%s line navigation preserves source, search and native keyboard behavior',
  async (locale) => {
    const copy = locale === 'en' ? en : zh;
    const onchange = vi.fn();
    const oncursor = vi.fn();
    await render(Editor, {
      props: {
        value: 'start: nop\nmov rax, 1\njmp start',
        locale,
        target: 'x86_64',
        wrap: false,
        diagnostic: null,
        onchange,
        oncursor,
      },
    });
    await page.getByRole('button', { name: copy.find, exact: true }).click();
    const find = page.getByRole('textbox', { name: copy.find, exact: true });
    await find.fill('start');
    await page.getByRole('button', { name: copy.go_to_line, exact: true }).click();
    const line = page.getByRole('textbox', { name: `${copy.go_to_line}:` });
    await expect.element(line).toHaveFocus();
    await line.fill('2:4');
    await page.getByRole('button', { name: copy.go, exact: true }).click();
    await expect.poll(() => oncursor.mock.lastCall).toEqual([2, 5]);
    await expect.element(page.getByRole('textbox', { name: copy.editor_label })).toHaveFocus();
    await userEvent.keyboard('{ControlOrMeta>}{Alt>}g{/Alt}{/ControlOrMeta}');
    await expect.element(line).toHaveFocus();
    await line.fill('+1');
    await userEvent.keyboard('{Enter}');
    await expect.poll(() => oncursor.mock.lastCall).toEqual([3, 1]);
    await expect.element(find).toHaveValue('start');
    expect(onchange).not.toHaveBeenCalled();
  },
);

test.each([
  { target: 'x86_64', prefix: 'vpaddus', mnemonic: 'vpaddusb' },
  { target: 'aarch64', prefix: 'whilel', mnemonic: 'whilelo' },
] as const)(
  '$target offers modern vector instruction completion',
  async ({ target, prefix, mnemonic }) => {
    const onchange = vi.fn();
    await render(Editor, {
      props: {
        value: '',
        locale: 'en',
        target,
        wrap: false,
        diagnostic: null,
        onchange,
        oncursor: () => {},
      },
    });
    await page.getByRole('textbox', { name: en.editor_label }).click();
    await userEvent.keyboard(prefix);
    await page.getByRole('option', { name: mnemonic, exact: true }).click();
    await expect.poll(() => onchange.mock.lastCall).toEqual([mnemonic]);
  },
);

test.each(['en', 'zh-CN'] as const)(
  '%s editor menu reflects available undo and redo without rebuilding history',
  async (locale) => {
    const copy = locale === 'en' ? en : zh;
    await render(Editor, {
      props: {
        value: '',
        locale,
        target: 'x86_64',
        wrap: false,
        diagnostic: null,
        onchange: () => {},
        oncursor: () => {},
      },
    });
    const menu = page.getByRole('button', { name: copy.editor_actions });
    const undo = page.getByRole('menuitem', { name: copy.undo, exact: true });
    const redo = page.getByRole('menuitem', { name: copy.redo, exact: true });
    await menu.click();
    await expect.element(undo).toHaveAttribute('aria-disabled', 'true');
    await expect.element(redo).toHaveAttribute('aria-disabled', 'true');
    await userEvent.keyboard('{Escape}');
    const editor = page.getByRole('textbox', { name: copy.editor_label });
    await editor.click();
    await userEvent.keyboard('nop');
    await menu.click();
    await undo.click();
    await expect.element(editor).toHaveFocus();
    await menu.click();
    await expect.element(undo).toHaveAttribute('aria-disabled', 'true');
    await redo.click();
    await expect.element(editor).toHaveTextContent('nop');
    await expect.element(editor).toHaveFocus();
  },
);

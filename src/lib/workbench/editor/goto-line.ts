import { gotoLine } from '@codemirror/search';
import { getDialog, type Command, type EditorView } from '@codemirror/view';
import { CornerDownLeft } from '@lucide/svelte';
import { mount, unmount } from 'svelte';

/** Keep CodeMirror's location syntax, submission and focus handling. */
export const goToLine: Command = (view) => {
  const handled = gotoLine(view);
  decorateDialog(view);
  return handled;
};

/** Preserve the line draft and decoration when phrase changes rebuild CodeMirror's dialog. */
export function retainLineDialog(view: EditorView, reconfigure: () => void) {
  const draft = lineInput(view)?.value;
  reconfigure();
  if (draft !== undefined) {
    const input = lineInput(view);
    if (input !== null && input !== undefined) input.value = draft;
    decorateDialog(view);
  }
}

function lineInput(view: EditorView) {
  return getDialog(view, 'cm-goto-line')?.dom.querySelector<HTMLInputElement>('input[name="line"]');
}

function decorateDialog(view: EditorView) {
  const panel = getDialog(view, 'cm-goto-line');
  const submit = panel?.dom.querySelector<HTMLButtonElement>('button[type="submit"]');
  if (panel === null || submit == null || submit.ariaLabel) return;
  const label = panel.dom.querySelector('label')?.firstChild;
  if (label?.nodeType === Node.TEXT_NODE)
    label.textContent = `${view.state.phrase('Go to line')}: `;
  submit.ariaLabel = view.state.phrase('go');
  submit.title = view.state.phrase('go');
  submit.replaceChildren();
  const icon = mount(CornerDownLeft, {
    target: submit,
    props: { size: 16, 'aria-hidden': 'true' },
  });
  const destroy = panel.destroy?.bind(panel);
  panel.destroy = () => {
    void unmount(icon);
    destroy?.();
  };
}

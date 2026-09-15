<script lang="ts">
  import { untrack } from 'svelte';
  import './editor.css';
  import { Annotation, Compartment, EditorState } from '@codemirror/state';
  import {
    EditorView,
    keymap,
    lineNumbers,
    highlightActiveLine,
    highlightActiveLineGutter,
    drawSelection,
    highlightSpecialChars,
    rectangularSelection,
    crosshairCursor,
    dropCursor,
  } from '@codemirror/view';
  import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
  import {
    searchKeymap,
    searchPanelOpen,
    closeSearchPanel,
    openSearchPanel,
    getSearchQuery,
    setSearchQuery,
    highlightSelectionMatches,
  } from '@codemirror/search';
  import { bracketMatching, indentUnit } from '@codemirror/language';
  import { autocompletion, closeBrackets, closeBracketsKeymap } from '@codemirror/autocomplete';
  import { lintGutter, nextDiagnostic, setDiagnostics, type Diagnostic } from '@codemirror/lint';
  import { assembly, assemblyHighlighting } from './language';
  import type { Target } from '$lib/protocol/generated/Target';
  import type { Locale } from '$lib/paraglide/runtime.js';
  import * as m from '$lib/paraglide/messages.js';

  const {
    value,
    locale,
    target,
    wrap,
    diagnostic,
    oncursor,
    onchange,
  }: {
    value: string;
    locale: Locale;
    target: Target;
    wrap: boolean;
    diagnostic: Diagnostic | null;
    oncursor: (line: number, column: number) => void;
    onchange: (source: string) => void;
  } = $props();

  let currentView: EditorView | undefined;

  /** Reveal the current build diagnostic without replacing editor state or history. */
  export function revealDiagnostic() {
    if (currentView === undefined) return;
    nextDiagnostic(currentView);
    currentView.dispatch({
      effects: EditorView.scrollIntoView(currentView.state.selection.main.head),
    });
    currentView.focus();
  }

  function translations() {
    const options = { locale };
    return EditorState.phrases.of({
      Completions: m.completions({}, options),
      'Selection deleted': m.selection_deleted({}, options),
      Find: m.find({}, options),
      'Go to line': m.go_to_line({}, options),
      go: m.go({}, options),
      'replaced match on line $': m.replaced_line({ line: '$' }, options),
      'replaced $ matches': m.replaced_matches({ count: '$' }, options),
      'current match': m.current_match({}, options),
      'on line': m.on_line({}, options),
      'Control character': m.control_character({}, options),
      replace: m.replace({}, options),
      Replace: m.replace({}, options),
      next: m.next({}, options),
      previous: m.previous({}, options),
      all: m.all({}, options),
      'match case': m.match_case({}, options),
      regexp: m.regexp({}, options),
      'by word': m.whole_word({}, options),
      'replace all': m.replace_all({}, options),
      close: m.close({}, options),
    });
  }

  /** Own the editor for this attachment; compartment updates preserve editing state. */
  function attachEditor(element: HTMLElement) {
    const externalSource = Annotation.define<boolean>();
    const language = new Compartment();
    const syntax = new Compartment();
    const wrapping = new Compartment();
    const accessibility = new Compartment();
    const view = new EditorView({
      parent: element,
      state: EditorState.create({
        doc: untrack(() => value),
        extensions: [
          lineNumbers(),
          lintGutter(),
          history(),
          drawSelection(),
          highlightSpecialChars(),
          rectangularSelection(),
          crosshairCursor(),
          dropCursor(),
          EditorState.allowMultipleSelections.of(true),
          highlightActiveLine(),
          highlightActiveLineGutter(),
          keymap.of([
            ...closeBracketsKeymap,
            ...defaultKeymap,
            ...historyKeymap,
            ...searchKeymap,
            { key: 'F8', run: nextDiagnostic },
          ]),
          bracketMatching(),
          closeBrackets(),
          autocompletion(),
          highlightSelectionMatches(),
          indentUnit.of('    '),
          assemblyHighlighting,
          syntax.of([]),
          wrapping.of([]),
          language.of([]),
          accessibility.of([]),
          EditorView.updateListener.of((update) => {
            if (
              update.transactions.some(
                (transaction) => transaction.docChanged && !transaction.annotation(externalSource),
              )
            )
              onchange(update.state.doc.toString());
            if (update.docChanged || update.selectionSet) {
              const position = update.state.selection.main.head;
              const line = update.state.doc.lineAt(position);
              oncursor(line.number, position - line.from + 1);
            }
          }),
          EditorView.darkTheme.of(true),
        ],
      }),
    });
    currentView = view;
    $effect(() => {
      view.dispatch({ effects: syntax.reconfigure(assembly(target)) });
    });
    $effect(() => {
      view.dispatch({ effects: wrapping.reconfigure(wrap ? EditorView.lineWrapping : []) });
    });
    $effect(() => {
      const next = value;
      if (!view.state.doc.eq(view.state.toText(next))) {
        // Preserve original BOM/newlines in the document owner until the user edits.
        view.dispatch({
          changes: { from: 0, to: view.state.doc.length, insert: next },
          annotations: externalSource.of(true),
        });
      }
    });
    $effect(() => {
      view.dispatch(setDiagnostics(view.state, diagnostic === null ? [] : [diagnostic]));
    });
    $effect(() => {
      // CodeMirror's built-in panel reads phrases only when constructed.
      // Recreate the panel, preserving its query and the user's external focus.
      const reopen = searchPanelOpen(view.state);
      const query = getSearchQuery(view.state);
      const focused = document.activeElement;
      if (reopen) closeSearchPanel(view);
      view.dispatch({
        effects: [
          language.reconfigure(translations()),
          accessibility.reconfigure(
            EditorView.contentAttributes.of({ 'aria-label': m.editor_label({}, { locale }) }),
          ),
        ],
      });
      if (reopen) {
        openSearchPanel(view);
        view.dispatch({ effects: setSearchQuery.of(query) });
        if (focused instanceof HTMLElement && focused.isConnected)
          focused.focus({ preventScroll: true });
      }
    });
    return () => {
      currentView = undefined;
      view.destroy();
    };
  }
</script>

<div class="editor" {@attach attachEditor}></div>

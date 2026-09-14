<script lang="ts">
  import { untrack } from 'svelte';
  import './editor.css';
  import { Compartment, EditorState } from '@codemirror/state';
  import {
    EditorView,
    keymap,
    lineNumbers,
    highlightActiveLine,
    highlightActiveLineGutter,
    drawSelection,
  } from '@codemirror/view';
  import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
  import {
    searchKeymap,
    searchPanelOpen,
    closeSearchPanel,
    openSearchPanel,
    getSearchQuery,
    setSearchQuery,
  } from '@codemirror/search';
  import type { Locale } from '$lib/paraglide/runtime.js';
  import * as m from '$lib/paraglide/messages.js';

  const {
    value,
    locale,
    onchange,
  }: { value: string; locale: Locale; onchange: (source: string) => void } = $props();

  function translations() {
    const options = { locale };
    return EditorState.phrases.of({
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

  /** Mount once; reactive configuration updates keep text, selection, and history. */
  function attachEditor(element: HTMLElement) {
    const language = new Compartment();
    const accessibility = new Compartment();
    const view = new EditorView({
      parent: element,
      state: EditorState.create({
        doc: untrack(() => value),
        extensions: [
          lineNumbers(),
          history(),
          drawSelection(),
          highlightActiveLine(),
          highlightActiveLineGutter(),
          keymap.of([...defaultKeymap, ...historyKeymap, ...searchKeymap, indentWithTab]),
          language.of([]),
          accessibility.of([]),
          EditorView.updateListener.of((update) => {
            if (update.docChanged) onchange(update.state.doc.toString());
          }),
          EditorView.darkTheme.of(true),
        ],
      }),
    });
    $effect(() => {
      const next = value;
      if (view.state.doc.toString() !== next) {
        view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: next } });
      }
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
      view.destroy();
    };
  }
</script>

<div class="editor" {@attach attachEditor}></div>

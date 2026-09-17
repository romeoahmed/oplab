import type { Locale } from '$lib/paraglide/runtime';
import { getSearchQuery, search } from '@codemirror/search';
import { EditorView, runScopeHandlers } from '@codemirror/view';
import { flushSync, mount, unmount } from 'svelte';

import Search from './Search.svelte';

/** CodeMirror owns query/match state and panel lifecycle; Svelte renders the form. */
export function searchPanel(locale: () => Locale) {
  return search({
    // GNU macro parameters and string escapes are literal source text.
    literal: true,
    scrollToMatch: (range) => EditorView.scrollIntoView(range, { y: 'center' }),
    createPanel(view) {
      const dom = document.createElement('div');
      dom.className = 'editor-search-panel';
      dom.addEventListener('keydown', (event) => {
        if (runScopeHandlers(view, event, 'search-panel')) event.preventDefault();
      });
      let query = $state(getSearchQuery(view.state));
      const matches = () => query.valid && !query.getCursor(view.state).next().done;
      let hasMatch = $state(matches());
      // Mount before CodeMirror synchronously queries [main-field] to focus the search input.
      const component = flushSync(() =>
        mount(Search, {
          target: dom,
          props: {
            view,
            get locale() {
              return locale();
            },
            get query() {
              return query;
            },
            get hasMatch() {
              return hasMatch;
            },
          },
        }),
      );
      return {
        dom,
        top: true,
        mount() {
          dom.querySelector<HTMLInputElement>('[main-field]')?.select();
        },
        update(update) {
          const next = getSearchQuery(view.state);
          if (update.docChanged || !query.eq(next)) {
            query = next;
            hasMatch = matches();
          }
        },
        destroy() {
          void unmount(component);
        },
      };
    },
  });
}

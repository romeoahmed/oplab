<script lang="ts">
  import * as m from '$lib/paraglide/messages.js';
  import type { Locale } from '$lib/paraglide/runtime';
  import {
    SearchQuery,
    getSearchQuery,
    setSearchQuery,
    findNext,
    findPrevious,
    replaceNext,
    replaceAll,
    closeSearchPanel,
  } from '@codemirror/search';
  import type { EditorView } from '@codemirror/view';
  import {
    ArrowUp,
    ArrowDown,
    X,
    ChevronRight,
    Replace,
    ReplaceAll,
    CaseSensitive,
    Regex,
    WholeWord,
  } from '@lucide/svelte';
  import { Collapsible, Toggle } from 'bits-ui';
  import { untrack } from 'svelte';

  const {
    view,
    query,
    hasMatch,
    locale,
  }: {
    view: EditorView;
    query: SearchQuery;
    hasMatch: boolean;
    locale: Locale;
  } = $props();
  const options = $derived({ locale });
  const errorId = $props.id();
  const invalid = $derived(query.search.length > 0 && !query.valid);
  const initiallyReplacing = untrack(() => query.replace.length > 0);
  const switches = $derived([
    { key: 'caseSensitive', icon: CaseSensitive, label: m.match_case({}, options) },
    { key: 'wholeWord', icon: WholeWord, label: m.whole_word({}, options) },
    { key: 'regexp', icon: Regex, label: m.regexp({}, options) },
  ] as const);
  function change(patch: Partial<ConstructorParameters<typeof SearchQuery>[0]>) {
    const { test, ...current } = getSearchQuery(view.state);
    view.dispatch({
      effects: setSearchQuery.of(
        new SearchQuery({ ...current, ...(test === undefined ? {} : { test }), ...patch }),
      ),
    });
  }
</script>

<Collapsible.Root open={initiallyReplacing}>
  <search class="editor-search" aria-label={m.find({}, options)}>
    <form
      class="search-row"
      autocomplete="off"
      onsubmit={(event) => {
        event.preventDefault();
        findNext(view);
      }}
    >
      <Collapsible.Trigger
        class="icon-button replace-toggle"
        disabled={view.state.readOnly}
        aria-label={m.toggle_replace({}, options)}
        title={m.toggle_replace({}, options)}
        ><ChevronRight size={14} aria-hidden="true" /></Collapsible.Trigger
      >
      <div class="search-field" class:invalid>
        <input
          {...{ 'main-field': 'true' }}
          aria-label={m.find({}, options)}
          placeholder={m.find({}, options)}
          bind:value={
            () => query.search,
            (search: string) => {
              change({ search });
            }
          }
          spellcheck="false"
          aria-invalid={invalid}
          aria-describedby={invalid ? errorId : undefined}
          onkeydown={(event) => {
            if (event.key === 'Enter' && event.shiftKey && !event.isComposing) {
              event.preventDefault();
              findPrevious(view);
            }
          }}
        />
        {#each switches as item (item.key)}
          <Toggle.Root
            class="icon-button"
            aria-label={item.label}
            title={item.label}
            bind:pressed={
              () => query[item.key],
              (pressed: boolean) => {
                change({ [item.key]: pressed });
              }
            }><item.icon size={15} aria-hidden="true" /></Toggle.Root
          >
        {/each}
      </div>
      <button
        type="button"
        class="icon-button"
        aria-label={m.previous({}, options)}
        title={m.previous({}, options)}
        disabled={!hasMatch}
        onclick={() => {
          findPrevious(view);
        }}><ArrowUp size={15} aria-hidden="true" /></button
      >
      <button
        class="icon-button"
        aria-label={m.next({}, options)}
        title={m.next({}, options)}
        disabled={!hasMatch}><ArrowDown size={15} aria-hidden="true" /></button
      >
      <button
        type="button"
        class="icon-button"
        aria-label={m.close({}, options)}
        title={m.close({}, options)}
        onclick={() => {
          closeSearchPanel(view);
        }}><X size={15} aria-hidden="true" /></button
      >
    </form>
    {#if invalid}<p id={errorId} class="search-feedback invalid" role="status">
        {m.search_invalid_regex({}, options)}
      </p>
    {:else if query.valid && !hasMatch}<p class="search-feedback" role="status">
        {m.search_no_matches({}, options)}
      </p>{/if}
    <Collapsible.Content>
      <form
        class="search-row replace-row"
        aria-label={m.replace({}, options)}
        autocomplete="off"
        onsubmit={(event) => {
          event.preventDefault();
          replaceNext(view);
        }}
      >
        <div class="search-field">
          <input
            aria-label={m.replace({}, options)}
            placeholder={m.replace({}, options)}
            bind:value={
              () => query.replace,
              (replace: string) => {
                change({ replace });
              }
            }
            spellcheck="false"
          />
        </div>
        <button
          class="icon-button"
          aria-label={m.replace({}, options)}
          title={m.replace({}, options)}
          disabled={!hasMatch}><Replace size={16} aria-hidden="true" /></button
        >
        <button
          type="button"
          class="icon-button"
          aria-label={m.replace_all({}, options)}
          title={m.replace_all({}, options)}
          disabled={!hasMatch}
          onclick={() => {
            replaceAll(view);
          }}><ReplaceAll size={16} aria-hidden="true" /></button
        >
      </form>
    </Collapsible.Content>
  </search>
</Collapsible.Root>

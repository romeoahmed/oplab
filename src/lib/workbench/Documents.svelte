<script lang="ts">
  import * as m from '$lib/paraglide/messages.js';
  import type { Locale } from '$lib/paraglide/runtime';
  import { FileCode, Plus, Pencil, ChevronDown, Check } from '@lucide/svelte';
  import { Popover, Tabs, DropdownMenu } from 'bits-ui';
  import type { Snippet } from 'svelte';

  import DocumentTab from './DocumentTab.svelte';

  import './documents.css';

  const {
    documents,
    active,
    loaded,
    locale,
    onselect,
    oncreate,
    onclose,
    onrename,
    children,
  }: {
    documents: readonly { id: string; name: string; hasSource: boolean }[];
    active: string;
    loaded: string | null;
    locale: Locale;
    onselect: (id: string) => void;
    oncreate: () => void;
    onclose: (id: string) => void;
    onrename: (name: string) => void;
    children: Snippet;
  } = $props();
  const options = $derived({ locale });
  let renaming = $state(false);
  let name = $state('');
</script>

<Tabs.Root
  value={active}
  onValueChange={onselect}
  class="source-pane"
  onkeydown={(event) => {
    if (
      event.defaultPrevented ||
      event.isComposing ||
      !event.ctrlKey ||
      event.altKey ||
      event.metaKey ||
      event.shiftKey
    )
      return;
    const direction = event.key === 'PageDown' ? 1 : event.key === 'PageUp' ? -1 : 0;
    if (direction === 0) return;
    const index = documents.findIndex((document) => document.id === active);
    const next = documents[(index + direction + documents.length) % documents.length];
    if (next === undefined) return;
    event.preventDefault();
    onselect(next.id);
  }}
>
  <header class="document-header">
    <Tabs.List class="document-tabs" aria-label={m.source_documents({}, options)}>
      {#each documents as document (document.id)}
        <DocumentTab
          {...document}
          active={document.id === active}
          loaded={document.id === loaded}
          {locale}
          {onclose}
        />
      {/each}
    </Tabs.List>
    <div class="document-actions">
      <DropdownMenu.Root>
        <DropdownMenu.Trigger
          class="icon-button"
          aria-label={m.source_documents({}, options)}
          title={m.source_documents({}, options)}
          ><ChevronDown size={16} aria-hidden="true" /></DropdownMenu.Trigger
        >
        <DropdownMenu.Portal>
          <DropdownMenu.Content
            class="command-menu document-menu"
            align="end"
            sideOffset={6}
            collisionPadding={12}
          >
            <DropdownMenu.RadioGroup value={active} onValueChange={onselect}>
              {#each documents as document (document.id)}
                <DropdownMenu.RadioItem value={document.id} textValue={document.name}>
                  {#snippet children({ checked })}
                    <FileCode size={15} aria-hidden="true" />
                    <span>{document.name}</span>
                    {#if checked}<Check size={15} aria-hidden="true" />{/if}
                  {/snippet}
                </DropdownMenu.RadioItem>
              {/each}
            </DropdownMenu.RadioGroup>
          </DropdownMenu.Content>
        </DropdownMenu.Portal>
      </DropdownMenu.Root>
      <button
        class="icon-button"
        aria-label={m.new_document({}, options)}
        title={m.new_document({}, options)}
        onclick={oncreate}><Plus size={15} aria-hidden="true" /></button
      >
      <Popover.Root
        bind:open={renaming}
        onOpenChange={(open) => {
          if (open) name = documents.find((document) => document.id === active)?.name ?? '';
        }}
      >
        <Popover.Trigger
          class="icon-button"
          aria-label={m.rename_document({}, options)}
          title={m.rename_document({}, options)}
          ><Pencil size={14} aria-hidden="true" /></Popover.Trigger
        >
        <Popover.Portal>
          <Popover.Content
            class="settings-popover"
            sideOffset={6}
            collisionPadding={12}
            align="end"
          >
            <form
              class="document-rename"
              onsubmit={(event) => {
                event.preventDefault();
                if (name.trim().length === 0) return;
                onrename(name);
                renaming = false;
              }}
            >
              <label
                >{m.document_name({}, options)}<input
                  bind:value={name}
                  required
                  maxlength="80"
                  spellcheck="false"
                /></label
              >
              <button>{m.rename_document({}, options)}</button>
            </form>
          </Popover.Content>
        </Popover.Portal>
      </Popover.Root>
    </div>
  </header>
  <Tabs.Content value={active} class="source-content" tabindex={-1}>
    {@render children()}
  </Tabs.Content>
</Tabs.Root>

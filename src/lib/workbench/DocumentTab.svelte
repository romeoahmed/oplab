<script lang="ts">
  import * as m from '$lib/paraglide/messages.js';
  import type { Locale } from '$lib/paraglide/runtime';
  import { Cpu, FileCode, X } from '@lucide/svelte';
  import { AlertDialog, Tabs } from 'bits-ui';

  const {
    id,
    name,
    active,
    loaded,
    hasSource,
    locale,
    onclose,
  }: {
    id: string;
    name: string;
    active: boolean;
    loaded: boolean;
    hasSource: boolean;
    locale: Locale;
    onclose: (id: string) => void;
  } = $props();
  const options = $derived({ locale });
  const closeLabel = $derived(m.close_document_named({ name }, options));
  let confirming = $state(false);

  function requestClose() {
    if (hasSource) confirming = true;
    else onclose(id);
  }
</script>

<AlertDialog.Root
  bind:open={
    () => confirming,
    (open: boolean) => {
      if (open) requestClose();
      else confirming = false;
    }
  }
>
  <div class="document-tab">
    <Tabs.Trigger
      value={id}
      title={name}
      onmousedown={(event) => {
        if (event.button === 1) event.preventDefault();
      }}
      onauxclick={(event) => {
        if (event.button !== 1) return;
        event.preventDefault();
        requestClose();
      }}
      onkeydown={(event) => {
        if (event.key !== 'Delete' || event.isComposing) return;
        event.preventDefault();
        requestClose();
      }}
      {@attach (element: HTMLElement) => {
        const list = element.closest('[role="tablist"]');
        if (!active || list === null) return;
        const observer = new ResizeObserver(() => {
          element.scrollIntoView({ block: 'nearest', inline: 'nearest' });
        });
        observer.observe(list);
        return () => {
          observer.disconnect();
        };
      }}
    >
      <FileCode size={14} aria-hidden="true" />
      <span>{name}</span>
      {#if loaded}<Cpu size={13} aria-label={m.loaded_document({}, options)} />{/if}
    </Tabs.Trigger>
    <AlertDialog.Trigger
      class="document-close icon-button"
      tabindex={active ? 0 : -1}
      aria-label={closeLabel}
      title={closeLabel}><X size={14} aria-hidden="true" /></AlertDialog.Trigger
    >
  </div>
  <AlertDialog.Portal>
    <AlertDialog.Overlay class="document-overlay" />
    <AlertDialog.Content class="document-dialog">
      <AlertDialog.Title class="document-dialog-title">{closeLabel}</AlertDialog.Title>
      <AlertDialog.Description class="document-dialog-description"
        >{m.close_document_hint({}, options)}</AlertDialog.Description
      >
      <div class="document-dialog-actions">
        <AlertDialog.Cancel>{m.keep_document({}, options)}</AlertDialog.Cancel>
        <AlertDialog.Action
          onclick={() => {
            confirming = false;
            onclose(id);
          }}>{m.close_document({}, options)}</AlertDialog.Action
        >
      </div>
    </AlertDialog.Content>
  </AlertDialog.Portal>
</AlertDialog.Root>

<script lang="ts">
  import * as m from '$lib/paraglide/messages.js';
  import type { Locale } from '$lib/paraglide/runtime';
  import { Check, Pencil, X } from '@lucide/svelte';
  import { Popover } from 'bits-ui';

  import { patchLimit } from './memory';

  import './editing.css';

  const {
    disabled,
    locale,
    onwrite,
  }: {
    disabled: boolean;
    locale: Locale;
    onwrite: (address: string, bytes: string) => void;
  } = $props();
  const options = $derived({ locale });
  let address = $state('');
  let bytes = $state('');
</script>

<Popover.Root>
  <Popover.Trigger {disabled}
    ><Pencil size={14} aria-hidden="true" />{m.write_memory({}, options)}</Popover.Trigger
  >
  <Popover.Portal>
    <Popover.Content
      class="settings-popover"
      sideOffset={8}
      align="end"
      aria-label={m.write_memory({}, options)}
    >
      <div class="popover-header">
        <h2>{m.write_memory({}, options)}</h2>
        <Popover.Close class="icon-button" aria-label={m.close({}, options)}
          ><X size={16} aria-hidden="true" /></Popover.Close
        >
      </div>
      <form
        class="machine-edit"
        aria-label={m.write_memory({}, options)}
        onsubmit={(event) => {
          event.preventDefault();
          onwrite(address, bytes);
        }}
      >
        <label
          >{m.patch_address({}, options)}<input
            required
            bind:value={address}
            spellcheck="false"
            placeholder="0x1000"
          /></label
        >
        <label
          >{m.patch_bytes({}, options)}<textarea
            required
            maxlength={patchLimit * 3}
            rows="3"
            bind:value={bytes}
            spellcheck="false"
            placeholder="2a 00 00 00"></textarea></label
        >
        <p class="muted-note">{m.patch_hint({}, options)}</p>
        <button {disabled}
          ><Check size={14} aria-hidden="true" />{m.write_memory({}, options)}</button
        >
      </form>
    </Popover.Content>
  </Popover.Portal>
</Popover.Root>

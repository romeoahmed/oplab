<script lang="ts">
  import * as m from '$lib/paraglide/messages.js';
  import type { Locale } from '$lib/paraglide/runtime';
  import { Circle, Plus, Trash } from '@lucide/svelte';

  import './breakpoints.css';

  const {
    addresses,
    disabled,
    locale,
    onchange,
  }: {
    addresses: readonly string[];
    disabled: boolean;
    locale: Locale;
    onchange: (address: string, enabled: boolean) => void;
  } = $props();
  const options = $derived({ locale });
  let address = $state('');
  const id = $props.id();
</script>

<section class="breakpoints" aria-label={m.breakpoints({}, options)}>
  <h3>
    <Circle size={13} aria-hidden="true" />{m.breakpoints({}, options)}<span
      >{addresses.length} / 256</span
    >
  </h3>
  <form
    onsubmit={(event) => {
      event.preventDefault();
      onchange(address, true);
    }}
  >
    <label class="sr-only" for={id}>{m.breakpoint_address({}, options)}</label>
    <input {id} required bind:value={address} spellcheck="false" placeholder="0x1000" />
    <button
      class="icon-button"
      disabled={disabled || addresses.length >= 256}
      aria-label={m.add_breakpoint({}, options)}
      title={m.add_breakpoint({}, options)}><Plus size={15} aria-hidden="true" /></button
    >
  </form>
  {#if addresses.length === 0}<p class="muted-note">{m.breakpoints_hint({}, options)}</p>{:else}
    <ul>
      {#each addresses as item (item)}<li>
          <code>{item}</code><button
            class="icon-button"
            {disabled}
            onclick={() => {
              onchange(item, false);
            }}
            aria-label={m.remove_breakpoint({ address: item }, options)}
            title={m.remove_breakpoint({ address: item }, options)}
            ><Trash size={14} aria-hidden="true" /></button
          >
        </li>{/each}
    </ul>
  {/if}
</section>

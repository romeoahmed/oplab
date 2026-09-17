<script lang="ts">
  import * as m from '$lib/paraglide/messages.js';
  import type { Locale } from '$lib/paraglide/runtime';
  import type { Target } from '$lib/protocol/generated/Target';
  import { Binary, ArrowDownToLine } from '@lucide/svelte';

  import type { InitialInput } from './initial-state';
  import InitialState from './InitialState.svelte';

  import './raw-code.css';

  let {
    value = $bindable(),
    setup = $bindable(),
    budget = $bindable(),
    bytes,
    disabled,
    locale,
    onload,
  }: {
    value: { target: Target; base: string; entry: string; completion: string };
    setup: InitialInput;
    budget: string;
    bytes: number | undefined;
    disabled: boolean;
    locale: Locale;
    onload: () => void;
  } = $props();
  const options = $derived({ locale });
</script>

<form
  class="raw-code"
  onsubmit={(event) => {
    event.preventDefault();
    onload();
  }}
>
  <header>
    <Binary size={20} aria-hidden="true" />
    <h3>{m.raw_code({}, options)}</h3>
    {#if bytes !== undefined}<span>{bytes} B · RX</span>{/if}
    <button disabled={disabled || bytes === undefined}
      ><ArrowDownToLine size={15} aria-hidden="true" />{m.load_raw({}, options)}</button
    >
  </header>
  <p class="muted-note">
    {bytes === undefined ? m.raw_empty({}, options) : m.raw_hint({}, options)}
  </p>
  <div class="raw-fields">
    <label
      >{m.target({}, options)}<select bind:value={value.target}
        ><option value="x86_64">x86_64</option><option value="aarch64">AArch64</option></select
      ></label
    >
    <label
      >{m.raw_base({}, options)}<input required bind:value={value.base} spellcheck="false" /></label
    >
    <label
      >{m.entry_address({}, options)}<input
        required
        bind:value={value.entry}
        spellcheck="false"
      /></label
    >
    <label
      >{m.completion_address({}, options)}<input
        required
        bind:value={value.completion}
        spellcheck="false"
      /></label
    >
    <label
      >{m.instruction_budget({}, options)}<input
        required
        bind:value={budget}
        inputmode="numeric"
        spellcheck="false"
      /></label
    >
  </div>
  <InitialState bind:value={setup} target={value.target} {locale} />
</form>

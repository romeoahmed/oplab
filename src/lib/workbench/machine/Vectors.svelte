<script lang="ts">
  import * as m from '$lib/paraglide/messages.js';
  import type { Locale } from '$lib/paraglide/runtime';
  import type { Registers } from '$lib/protocol/generated/Registers';
  import type { RoundingMode } from '$lib/protocol/generated/RoundingMode';
  import type { VectorWrite } from '$lib/protocol/generated/VectorWrite';
  import { Check } from '@lucide/svelte';

  import VectorEdit from './VectorEdit.svelte';
  import {
    laneFormats,
    vectorEntries,
    vectorWidth,
    roundingMode,
    type LaneFormat,
    type VectorView,
  } from './vectors';
  import VectorValue from './VectorValue.svelte';

  import './vectors.css';

  const {
    bank,
    locale,
    editable,
    onwrite,
    onrounding,
  }: {
    bank: Registers | null;
    locale: Locale;
    editable: boolean;
    onwrite: (write: VectorWrite) => void;
    onrounding: (mode: RoundingMode) => void;
  } = $props();
  const options = $derived({ locale });
  let format = $state<LaneFormat>('hex');
  const currentRounding = $derived(bank === null ? 'nearest_even' : roundingMode(bank));
  let rounding = $derived(currentRounding);
  let view = $state<VectorView>('full');
  let stride = $state(1);
  const entries = $derived(bank === null ? [] : vectorEntries(bank, view));
  const controls = $derived(
    bank === null
      ? []
      : bank.type === 'x86_64'
        ? [{ name: 'MXCSR', value: bank.data.mxcsr }]
        : [
            { name: 'FPCR', value: bank.data.fpcr },
            { name: 'FPSR', value: bank.data.fpsr },
          ],
  );
</script>

<section class="vector-panel" aria-label={m.simd_registers({}, options)}>
  <div class="section-heading">
    <h2>{m.simd_registers({}, options)}</h2>
    {#if bank !== null}<span
        >{m.register_width(
          { bits: String(entries[0] ? vectorWidth(entries[0].value) : 0) },
          options,
        )}</span
      >{/if}
    {#if bank !== null}{#key `${bank.type}:${view}`}<VectorEdit
          {entries}
          predicate={view === 'predicates'}
          {locale}
          {editable}
          {onwrite}
        />{/key}{/if}
  </div>
  {#if bank === null}
    <p class="muted-note">{m.no_registers({}, options)}</p>
  {:else}
    {#if bank.type === 'aarch64'}
      <p class="muted-note vector-scope">
        {m.vector_length(
          { current: String(bank.data.vl * 8), maximum: String(bank.data.max_vl * 8) },
          options,
        )}
      </p>
    {/if}
    <div class="vector-toolbar">
      <label
        >{m.vector_bank({}, options)}
        <select bind:value={view}>
          <option value="full">{bank.type === 'x86_64' ? 'YMM' : 'Z'}</option>
          <option value="low">{bank.type === 'x86_64' ? 'XMM' : 'V'}</option>
          {#if bank.type === 'aarch64'}<option value="predicates">P · FFR</option>{/if}
        </select>
      </label>
      {#if view === 'predicates'}
        <label
          >{m.vector_element({}, options)}
          <select bind:value={stride}>
            <option value={1}>.b · 8</option><option value={2}>.h · 16</option>
            <option value={4}>.s · 32</option><option value={8}>.d · 64</option>
          </select>
        </label>
      {:else}
        <label
          >{m.vector_format({}, options)}
          <select bind:value={format}>
            {#each laneFormats as choice (choice)}
              <option value={choice}>{choice === 'hex' ? m.vector_hex({}, options) : choice}</option
              >
            {/each}
          </select>
        </label>
      {/if}
    </div>
    <p class="muted-note compact">
      {view === 'predicates'
        ? m.vector_predicate_hint({}, options)
        : m.vector_lanes_hint({}, options)}
    </p>
    <dl class="register-grid">
      {#each controls as control (control.name)}
        <div>
          <dt>{control.name}</dt>
          <dd>{control.value.toString(16).padStart(8, '0')}</dd>
        </div>
      {/each}
    </dl>
    <form
      class="vector-rounding"
      onsubmit={(event) => {
        event.preventDefault();
        if (editable) onrounding(rounding);
      }}
    >
      <label
        >{m.rounding_mode({}, options)}
        <select bind:value={rounding} disabled={!editable}>
          <option value="nearest_even">{m.rounding_nearest({}, options)}</option>
          <option value="down">{m.rounding_down({}, options)}</option>
          <option value="up">{m.rounding_up({}, options)}</option>
          <option value="toward_zero">{m.rounding_zero({}, options)}</option>
        </select>
      </label>
      <button
        class="icon-button"
        disabled={!editable || rounding === currentRounding}
        aria-label={m.set_rounding({}, options)}
        title={m.set_rounding({}, options)}><Check size={16} aria-hidden="true" /></button
      >
    </form>
    <p class="muted-note compact">{m.rounding_hint({}, options)}</p>
    <div class="vector-bank">
      {#each entries as entry (entry.name)}
        <VectorValue
          {entry}
          format={view === 'predicates' ? 'bit' : format}
          stride={view === 'predicates' ? stride : 1}
        />
      {/each}
    </div>
  {/if}
</section>

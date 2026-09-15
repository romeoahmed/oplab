<script lang="ts">
  import './instructions.css';
  import { onDestroy } from 'svelte';
  import { ArrowRight, ListOrdered } from '@lucide/svelte';
  import { normalizeAddress } from '$lib/protocol/scalars';
  import { problemLabel } from '../presentation';
  import { decodeWindow } from './bytes';
  import type { Target } from '$lib/protocol/generated/Target';
  import type { DecodedInstruction } from '$lib/protocol/generated/DecodedInstruction';
  import type { Locale } from '$lib/paraglide/runtime';
  import * as m from '$lib/paraglide/messages.js';

  const {
    bytes,
    target,
    base,
    connected,
    locale,
    decode,
  }: {
    bytes: Uint8Array | undefined;
    target: Target;
    base: string;
    connected: boolean;
    locale: Locale;
    decode: (bytes: Uint8Array, target: Target, base: string) => Promise<DecodedInstruction[]>;
  } = $props();
  const options = $derived({ locale });
  const input = $derived({ bytes, target, base, connected });
  function matches(value: typeof input) {
    return (
      value.bytes === bytes &&
      value.target === target &&
      value.base === base &&
      value.connected === connected
    );
  }
  let query = $state.raw<{ input: typeof input; offset: number } | null>(null);
  const offset = $derived(query !== null && matches(query.input) ? query.offset : 0);
  type Outcome =
    | { type: 'decoded'; rows: DecodedInstruction[]; next: number }
    | { type: 'error'; code: string; address: string | null };
  let completed = $state.raw<{ input: typeof input; value: Outcome } | null>(null);
  const result = $derived(completed !== null && matches(completed.input) ? completed.value : null);
  let busy = $state(false);
  let active = true;
  onDestroy(() => {
    active = false;
  });

  async function inspect(start = offset) {
    if (busy || bytes === undefined || !connected) return;
    const current = input;
    busy = true;
    completed = null;
    try {
      const window = decodeWindow(bytes, normalizeAddress(base), start);
      const rows = await decode(window.bytes, target, window.base);
      if (!active || !matches(current)) return;
      completed = {
        input: current,
        value: {
          type: 'decoded',
          rows,
          next: start + rows.reduce((sum, row) => sum + row.bytes.length, 0),
        },
      };
      query = { input: current, offset: start };
    } catch (failure) {
      if (!active || !matches(current)) return;
      const code =
        typeof failure === 'object' &&
        failure !== null &&
        'code' in failure &&
        typeof failure.code === 'string'
          ? failure.code
          : failure instanceof RangeError
            ? 'decode'
            : 'unavailable';
      let address: string | null = null;
      if (
        typeof failure === 'object' &&
        failure !== null &&
        'address' in failure &&
        typeof failure.address === 'string'
      )
        address = failure.address;
      completed = { input: current, value: { type: 'error', code, address } };
    } finally {
      busy = false;
    }
  }
</script>

<div class="instructions">
  {#if bytes === undefined}
    <p class="muted-note">{m.instructions_empty({}, options)}</p>
  {:else}
    <form
      class="instruction-controls"
      onsubmit={(event) => {
        event.preventDefault();
        void inspect();
      }}
    >
      <span class="instruction-origin">{target} · {base} · {bytes.length} B</span>
      <label
        >{m.byte_offset({}, options)}<input
          type="number"
          required
          min="0"
          max={bytes.length - 1}
          value={offset}
          oninput={(event) => {
            query = { input, offset: event.currentTarget.valueAsNumber };
          }}
        /></label
      >
      <button disabled={!connected || busy}>
        <ListOrdered size={14} aria-hidden="true" />{m.disassemble({}, options)}
      </button>
      <button
        type="button"
        disabled={!connected || busy || result?.type !== 'decoded' || result.next >= bytes.length}
        onclick={() => {
          if (result?.type === 'decoded') void inspect(result.next);
        }}>{m.next_instructions({}, options)}<ArrowRight size={14} aria-hidden="true" /></button
      >
    </form>
    <p class="instruction-note">{m.instructions_hint({}, options)}</p>
    {#if result?.type === 'error'}<p role="alert" class="instruction-error">
        {problemLabel(result.code, locale)}{#if result.address !== null}
          <code>{result.address}</code>{/if}
      </p>{/if}
    {#if result?.type === 'decoded' && result.rows.length > 0}
      <div class="instruction-table">
        <table aria-label={m.instructions({}, options)}>
          <thead
            ><tr
              ><th scope="col">{m.address({}, options)}</th><th scope="col"
                >{m.bytes({}, options)}</th
              ><th scope="col">{m.instruction({}, options)}</th></tr
            ></thead
          >
          <tbody
            >{#each result.rows as row (row.address)}<tr
                ><td>{row.address}</td><td
                  >{row.bytes.map((byte) => byte.toString(16).padStart(2, '0')).join(' ')}</td
                ><td>{row.text}</td></tr
              >{/each}</tbody
          >
        </table>
      </div>
    {/if}
  {/if}
</div>

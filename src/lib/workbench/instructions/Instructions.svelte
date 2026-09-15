<script lang="ts">
  import './instructions.css';
  import * as m from '$lib/paraglide/messages.js';
  import type { Locale } from '$lib/paraglide/runtime';
  import type { DecodedInstruction } from '$lib/protocol/generated/DecodedInstruction';
  import type { InstructionAnalysis } from '$lib/protocol/generated/InstructionAnalysis';
  import type { Target } from '$lib/protocol/generated/Target';
  import { normalizeAddress } from '$lib/protocol/scalars';
  import { ArrowRight, ListOrdered } from '@lucide/svelte';
  import { onDestroy, type Snippet } from 'svelte';

  import { problemLabel } from '../presentation';
  import Analysis from './Analysis.svelte';
  import { decodeWindow } from './bytes';

  const {
    bytes,
    target,
    base,
    connected,
    locale,
    decode,
    analyze,
    source,
  }: {
    source?: Snippet;
    bytes: Uint8Array | undefined;
    target: Target;
    base: string;
    connected: boolean;
    locale: Locale;
    analyze: (bytes: Uint8Array, target: Target, base: string) => Promise<InstructionAnalysis>;
    decode: (bytes: Uint8Array, target: Target, base: string) => Promise<DecodedInstruction[]>;
  } = $props();
  const options = $derived({ locale });
  // Per-field memoization keeps unrelated prop updates out of the input identity.
  const [code, guest, origin, available] = $derived([bytes, target, base, connected] as const);
  const input = $derived({ bytes: code, target: guest, base: origin, connected: available });
  let query = $state.raw<{ input: typeof input; offset: number } | null>(null);
  const offset = $derived(query?.input === input ? query.offset : 0);
  type Outcome =
    | { type: 'decoded'; rows: DecodedInstruction[]; next: number }
    | { type: 'error'; code: string; address: string | null };
  let completed = $state.raw<{ input: typeof input; value: Outcome } | null>(null);
  const result = $derived(completed?.input === input ? completed.value : null);
  let selection = $state.raw<{
    row: DecodedInstruction;
    request: Promise<InstructionAnalysis>;
  } | null>(null);
  const selected = $derived(
    selection !== null && result?.type === 'decoded' && result.rows.includes(selection.row)
      ? selection
      : null,
  );
  function select(row: DecodedInstruction) {
    selection = { row, request: analyze(new Uint8Array(row.bytes), target, row.address) };
  }
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
    selection = null;
    try {
      const window = decodeWindow(bytes, normalizeAddress(base), start);
      const rows = await decode(window.bytes, target, window.base);
      if (!active || current !== input) return;
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
      if (!active || current !== input) return;
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
      {@render source?.()}
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
    {#if result?.type !== 'decoded'}<p class="instruction-note">
        {m.instructions_hint({}, options)}
      </p>{/if}
    {#if result?.type === 'error'}<p role="alert" class="instruction-error">
        {problemLabel(result.code, locale)}{#if result.address !== null}
          <code>{result.address}</code>{/if}
      </p>{/if}
    {#if result?.type === 'decoded' && result.rows.length > 0}
      <div class="instruction-inspection">
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
                  ><td
                    ><button
                      type="button"
                      class="instruction-select"
                      aria-pressed={selected?.row === row}
                      onclick={() => {
                        if (selected?.row === row) selection = null;
                        else select(row);
                      }}>{row.text}</button
                    ></td
                  ></tr
                >{/each}</tbody
            >
          </table>
        </div>
        {#if selected !== null}
          {@const current = selected}
          {#key current.row}<Analysis
              instruction={current.row}
              request={current.request}
              {locale}
              retry={() => {
                select(current.row);
              }}
              close={() => {
                selection = null;
              }}
            />{/key}
        {/if}
      </div>
    {/if}
  {/if}
</div>

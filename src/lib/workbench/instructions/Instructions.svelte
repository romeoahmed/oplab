<script lang="ts">
  import './instructions.css';
  import * as m from '$lib/paraglide/messages.js';
  import type { Locale } from '$lib/paraglide/runtime';
  import type { DecodedInstruction } from '$lib/protocol/generated/DecodedInstruction';
  import type { InstructionAnalysis } from '$lib/protocol/generated/InstructionAnalysis';
  import type { SourceLocation } from '$lib/protocol/generated/SourceLocation';
  import type { Target } from '$lib/protocol/generated/Target';
  import { normalizeAddress } from '$lib/protocol/scalars';
  import { ArrowRight, ListEnd, Circle, CornerDownRight, ListOrdered } from '@lucide/svelte';
  import { onDestroy, type Snippet } from 'svelte';

  import { sourceIndex } from '../editor/source';
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
    live = false,
    pc,
    breakpoints = [],
    editable = false,
    onbreakpoint,
    onsetpc,
    onrun,
    locations = [],
    onsource,
  }: {
    locations?: readonly SourceLocation[];
    onsource?: (line: number) => void;
    source?: Snippet;
    live?: boolean;
    pc?: string | undefined;
    breakpoints?: readonly string[];
    editable?: boolean;
    onbreakpoint?: (address: string, enabled: boolean) => void;
    onsetpc?: (address: string) => void;
    onrun?: (address: string) => void;
    bytes: Uint8Array | undefined;
    target: Target;
    base: string;
    connected: boolean;
    locale: Locale;
    analyze: (bytes: Uint8Array, target: Target, base: string) => Promise<InstructionAnalysis>;
    decode: (bytes: Uint8Array, target: Target, base: string) => Promise<DecodedInstruction[]>;
  } = $props();
  const options = $derived({ locale });
  const sourceLines = $derived(sourceIndex(locations).addresses);
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
  let pending = $state<symbol>();
  const busy = $derived(pending !== undefined);
  let active = true;
  onDestroy(() => {
    active = false;
  });

  /** Decode from an exact linked address inside the selected byte source. */
  export async function reveal(address: string) {
    if (bytes === undefined) return;
    const relative = BigInt(address) - BigInt(base);
    if (relative < 0n || relative >= BigInt(bytes.length)) return;
    await inspect(Number(relative));
  }

  async function inspect(start = offset) {
    if (bytes === undefined || !connected) return;
    const current = input;
    const request = Symbol();
    pending = request;
    completed = null;
    selection = null;
    try {
      const window = decodeWindow(bytes, normalizeAddress(base), start);
      const rows = await decode(window.bytes, target, window.base);
      if (!active || current !== input || pending !== request) return;
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
      if (!active || current !== input || pending !== request) return;
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
      if (pending === request) pending = undefined;
    }
  }
</script>

<div class="instructions">
  {@render source?.()}
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
        class="icon-button"
        aria-label={m.next_instructions({}, options)}
        title={m.next_instructions({}, options)}
        disabled={!connected || busy || result?.type !== 'decoded' || result.next >= bytes.length}
        onclick={() => {
          if (result?.type === 'decoded') void inspect(result.next);
        }}><ArrowRight size={14} aria-hidden="true" /></button
      >
    </form>
    {#if live}<p class="muted-note">{m.live_decode_hint({}, options)}</p>{/if}
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
                >{#if live}<th scope="col"
                    ><span class="sr-only">{m.execution_controls({}, options)}</span></th
                  >{/if}<th scope="col">{m.address({}, options)}</th><th scope="col"
                  >{m.bytes({}, options)}</th
                ><th scope="col">{m.instruction({}, options)}</th></tr
              ></thead
            >
            <tbody
              >{#each result.rows as row (row.address)}<tr
                  aria-current={row.address === pc ? 'step' : undefined}
                  >{#if live}<td
                      ><button
                        class="instruction-breakpoint icon-button"
                        type="button"
                        disabled={!editable}
                        aria-label={m.breakpoint_at({ address: row.address }, options)}
                        aria-pressed={breakpoints.includes(row.address)}
                        onclick={() => {
                          onbreakpoint?.(row.address, !breakpoints.includes(row.address));
                        }}><Circle size={12} aria-hidden="true" /></button
                      ><button
                        class="icon-button"
                        type="button"
                        disabled={!editable}
                        aria-label={m.set_pc_at({ address: row.address }, options)}
                        title={m.set_pc_at({ address: row.address }, options)}
                        onclick={() => {
                          onsetpc?.(row.address);
                        }}><CornerDownRight size={14} aria-hidden="true" /></button
                      >{#if onrun !== undefined}<button
                          class="icon-button"
                          type="button"
                          disabled={!editable}
                          aria-label={`${m.run_to_address({}, options)} ${row.address}`}
                          title={m.run_to_address({}, options)}
                          onclick={() => {
                            onrun(row.address);
                          }}><ListEnd size={14} aria-hidden="true" /></button
                        >{/if}</td
                    >{/if}<td class="instruction-address"
                    >{#if row.address === pc}<ArrowRight
                        size={12}
                        aria-label={m.current_instruction({}, options)}
                      />{/if}{row.address}</td
                  ><td class="instruction-bytes"
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
                    >
                    {#each [...(sourceLines.get(row.address) ?? [])] as line (line)}
                      <button
                        type="button"
                        class="source-location"
                        title={m.reveal_source({ line }, options)}
                        aria-label={m.reveal_source({ line }, options)}
                        onclick={() => onsource?.(line)}>:{line}</button
                      >
                    {/each}</td
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

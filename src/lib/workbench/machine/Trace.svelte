<script lang="ts">
  import * as m from '$lib/paraglide/messages.js';
  import type { Locale } from '$lib/paraglide/runtime';
  import type { ExecutionTrace } from '$lib/protocol/generated/ExecutionTrace';
  import type { Observation } from '$lib/protocol/generated/Observation';
  import { RefreshCw, Eraser, ListEnd, Circle, FileCode } from '@lucide/svelte';
  import { untrack } from 'svelte';

  import type { sourceIndex as indexSource } from '../editor/source';

  import './trace.css';

  const {
    trace,
    observation,
    connected,
    busy,
    locale,
    sourceIndex,
    onrefresh,
    onrecord,
    onclear,
    onrun,
    onsource,
  }: {
    trace: ExecutionTrace | null;
    observation: Observation | undefined;
    connected: boolean;
    busy: boolean;
    locale: Locale;
    sourceIndex: ReturnType<typeof indexSource>;
    onrefresh: () => Promise<void>;
    onrecord: (enabled: boolean) => void;
    onclear: () => void;
    onrun: (address: string) => Promise<void>;
    onsource: (line: number) => void;
  } = $props();
  const options = $derived({ locale });
  const editable = $derived(
    connected &&
      !busy &&
      observation !== undefined &&
      ['ready', 'paused', 'stepped', 'breakpoint', 'target'].includes(observation.status.type),
  );
  const stopped = $derived(
    connected &&
      !busy &&
      observation !== undefined &&
      !['running', 'crashed'].includes(observation.status.type),
  );
  let address = $state('');
  const id = $props.id();

  // Filter observation-only updates before the request effect, including parent prop updates.
  const position = $derived(
    !connected || observation === undefined || observation.status.type === 'running'
      ? null
      : `${observation.key.session}:${observation.key.generation}:${observation.status.type}:${observation.instructions}`,
  );
  $effect(() => {
    if (position !== null) {
      untrack(() => {
        void onrefresh();
      });
    }
  });
</script>

<section class="execution-trace" aria-label={m.trace({}, options)}>
  <div class="trace-controls">
    <button
      class="trace-record"
      aria-pressed={trace?.enabled ?? false}
      disabled={!editable || trace === null}
      onclick={() => {
        onrecord(!(trace?.enabled ?? false));
      }}
    >
      <Circle size={13} fill={trace?.enabled ? 'currentColor' : 'none'} aria-hidden="true" />
      {m.trace_record({}, options)}
    </button>
    <button
      class="icon-button"
      disabled={!connected || observation === undefined}
      aria-label={m.trace_refresh({}, options)}
      title={m.trace_refresh({}, options)}
      onclick={() => {
        void onrefresh();
      }}><RefreshCw size={15} aria-hidden="true" /></button
    >
    <button
      class="icon-button"
      disabled={!stopped || trace === null || trace.entries.length === 0}
      aria-label={m.trace_clear({}, options)}
      title={m.trace_clear({}, options)}
      onclick={onclear}><Eraser size={15} aria-hidden="true" /></button
    >
    <form
      onsubmit={(event) => {
        event.preventDefault();
        void onrun(address);
      }}
    >
      <label class="sr-only" for={id}>{m.address({}, options)}</label>
      <input
        {id}
        bind:value={address}
        required
        pattern={'(?:0[xX])?[0-9a-fA-F]{1,16}'}
        placeholder="0x1000"
        spellcheck="false"
      />
      <button disabled={!editable}
        ><ListEnd size={14} aria-hidden="true" />{m.run_to_address({}, options)}</button
      >
    </form>
  </div>
  <p class="muted-note">{m.trace_hint({}, options)}</p>
  {#if trace === null || trace.entries.length === 0}
    <p class="muted-note">{m.trace_empty({}, options)}</p>
  {:else}
    {#if trace.discarded !== '0'}<p class="muted-note">
        {m.trace_discarded({ count: trace.discarded }, options)}
      </p>{/if}
    <div class="trace-table">
      <table>
        <thead
          ><tr
            ><th scope="col">{m.trace_instruction({}, options)}</th><th scope="col"
              >{m.address({}, options)}</th
            ><th scope="col">{m.source({}, options)}</th></tr
          ></thead
        >
        <tbody>
          {#each trace.entries as entry (entry.instruction)}
            <tr
              ><td>{entry.instruction}</td><td class="trace-address"><code>{entry.pc}</code></td><td
              >
                {#each sourceIndex.addresses.get(entry.pc) as line (line)}
                  <button
                    class="trace-source"
                    aria-label={m.reveal_source({ line }, options)}
                    title={m.reveal_source({ line }, options)}
                    onclick={() => {
                      onsource(line);
                    }}><FileCode size={13} aria-hidden="true" />{line}</button
                  >
                {/each}
              </td></tr
            >
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</section>

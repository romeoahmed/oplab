<script lang="ts">
  import * as m from '$lib/paraglide/messages.js';
  import type { Locale } from '$lib/paraglide/runtime';
  import type { DataWatchpoint } from '$lib/protocol/generated/DataWatchpoint';
  import type { DataWatchpointHit } from '$lib/protocol/generated/DataWatchpointHit';
  import type { WatchAccess } from '$lib/protocol/generated/WatchAccess';
  import { ScanEye, Plus, Trash } from '@lucide/svelte';

  import './debugging.css';
  import { watchAccessLabel } from '../presentation';

  const {
    points,
    disabled,
    locale,
    hit,
    onchange,
  }: {
    points: readonly DataWatchpoint[];
    disabled: boolean;
    locale: Locale;
    hit: DataWatchpointHit | null;
    onchange: (points: DataWatchpoint[]) => void;
  } = $props();
  const options = $derived({ locale });
  let address = $state('');
  let length = $state<number | undefined>(8);
  let access = $state<WatchAccess>('write');
  const modes = ['read', 'write', 'read_write'] as const;
</script>

<section class="debug-points" aria-label={m.watchpoints({}, options)}>
  <h3>
    <ScanEye size={15} aria-hidden="true" />{m.watchpoints({}, options)}<span
      >{points.length} / 32</span
    >
  </h3>
  <p class="muted-note">{m.watchpoints_hint({}, options)}</p>
  <form
    onsubmit={(event) => {
      event.preventDefault();
      if (length !== undefined) onchange([...points, { address, length, access }]);
    }}
  >
    <fieldset class="watch-fields" disabled={disabled || points.length >= 32}>
      <label class="watch-address"
        >{m.watch_address({}, options)}<input
          required
          bind:value={address}
          spellcheck="false"
          placeholder="0x2000"
        /></label
      >
      <label
        >{m.watch_length({}, options)}<input
          required
          type="number"
          min="1"
          max="65536"
          step="1"
          bind:value={length}
        /></label
      >
      <label
        >{m.watch_access({}, options)}<select bind:value={access}
          >{#each modes as mode (mode)}<option value={mode}>{watchAccessLabel(mode, locale)}</option
            >{/each}</select
        ></label
      >
      <button
        class="icon-button"
        aria-label={m.add_watchpoint({}, options)}
        title={m.add_watchpoint({}, options)}><Plus size={15} aria-hidden="true" /></button
      >
    </fieldset>
  </form>
  <ul>
    {#each points as point, index (`${point.address}:${String(point.length)}:${point.access}`)}
      <li
        class:matched={hit !== null &&
          hit.watchpoint.address === point.address &&
          hit.watchpoint.length === point.length &&
          hit.watchpoint.access === point.access}
      >
        <div>
          <code>{point.address}</code><span
            >{point.length} B · {watchAccessLabel(point.access, locale)}</span
          >
        </div>
        <button
          class="icon-button"
          {disabled}
          onclick={() => {
            onchange(points.filter((_, i) => i !== index));
          }}
          aria-label={m.remove_watchpoint({ address: point.address }, options)}
          title={m.remove_watchpoint({ address: point.address }, options)}
          ><Trash size={14} aria-hidden="true" /></button
        >
      </li>
    {/each}
  </ul>
</section>

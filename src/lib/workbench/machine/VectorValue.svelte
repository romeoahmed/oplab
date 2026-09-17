<script lang="ts">
  import type { VectorEntry, LaneFormat } from './vectors';
  import { vectorLanes } from './vectors';

  const {
    entry,
    format,
    stride = 1,
  }: {
    entry: VectorEntry;
    format: LaneFormat;
    stride?: number;
  } = $props();
  let open = $state(false);
  const raw = $derived(entry.value.slice(2));
  const lanes = $derived(open && format !== 'hex' ? vectorLanes(entry.value, format) : []);
</script>

<details class="vector-register" bind:open>
  <summary>
    <span>{entry.name.toUpperCase()}</span>
    <code>{raw.length > 32 ? `${raw.slice(0, 12)}…${raw.slice(-16)}` : raw}</code>
  </summary>
  {#if open}
    {#if raw.length > 32}<code class="vector-bits">{raw}</code>{/if}
    {#if format !== 'hex'}
      <ol class="vector-lanes" start="0">
        {#each lanes as lane, position (position)}
          {#if position % stride === 0}<li><code>{lane}</code></li>{/if}
        {/each}
      </ol>
    {/if}
  {/if}
</details>

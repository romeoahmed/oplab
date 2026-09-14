<script lang="ts">
  import type { MemoryWindow } from '$lib/protocol/generated/MemoryWindow';
  import type { Locale } from '$lib/paraglide/runtime';
  import { parseAddress, formatAddress } from '$lib/protocol/scalars';
  import * as m from '$lib/paraglide/messages.js';
  const {
    window,
    bytes,
    locale,
  }: { window: MemoryWindow | null; bytes: Uint8Array | null; locale: Locale } = $props();
  const rows = $derived(
    bytes === null || window === null
      ? []
      : Array.from({ length: Math.ceil(bytes.length / 16) }, (_, row) => {
          const start = row * 16;
          const values = Array.from(bytes.subarray(start, start + 16));
          return {
            address: formatAddress(parseAddress(window.address) + BigInt(start)),
            hex: values.map((value) => value.toString(16).padStart(2, '0')).join(' '),
            ascii: values
              .map((value) => (value >= 32 && value <= 126 ? String.fromCharCode(value) : '·'))
              .join(''),
          };
        }),
  );
</script>

<div class="memory-view" aria-label={m.memory({}, { locale })}>
  {#if rows.length === 0}<p class="muted-note">{m.no_memory({}, { locale })}</p>
  {:else}<table>
      <thead
        ><tr
          ><th>{m.address({}, { locale })}</th><th>{m.bytes({}, { locale })}</th><th>ASCII</th></tr
        ></thead
      >
      <tbody
        >{#each rows as row (row.address)}<tr
            ><th scope="row">{row.address.slice(2)}</th><td>{row.hex}</td><td class="ascii"
              >{row.ascii}</td
            ></tr
          >{/each}</tbody
      >
    </table>{/if}
</div>

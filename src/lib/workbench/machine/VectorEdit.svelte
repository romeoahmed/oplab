<script lang="ts">
  import * as m from '$lib/paraglide/messages.js';
  import type { Locale } from '$lib/paraglide/runtime';
  import type { VectorWrite } from '$lib/protocol/generated/VectorWrite';
  import { Check, Pencil, X } from '@lucide/svelte';
  import { Popover } from 'bits-ui';

  import {
    laneFormats,
    laneWidth,
    vectorInput,
    vectorLanes,
    type LaneFormat,
    type VectorEntry,
    vectorWidth,
  } from './vectors';

  import './editing.css';

  const {
    entries,
    predicate = false,
    locale,
    editable,
    onwrite,
  }: {
    entries: VectorEntry[];
    predicate?: boolean;
    locale: Locale;
    editable: boolean;
    onwrite: (write: VectorWrite) => void;
  } = $props();
  const options = $derived({ locale });
  const formats = $derived(predicate ? (['hex', 'bit'] as const) : laneFormats);
  let register = $state(0);
  let format = $state<LaneFormat>('hex');
  const selected = $derived(entries[register]);
  const bits = $derived(selected ? vectorWidth(selected.value) : 0);
  const width = $derived(laneWidth(format, bits));
  let lane = $state(0);
  let value = $state('');
  let input: HTMLInputElement | undefined;
  const parsed = $derived(vectorInput(value, format, bits));

  function readValue() {
    value = selected ? (vectorLanes(selected.value, format)[lane] ?? '') : '';
    input?.setCustomValidity('');
  }
</script>

<Popover.Root
  onOpenChange={(open) => {
    if (open) readValue();
  }}
>
  <Popover.Trigger
    class="icon-button"
    disabled={!editable}
    aria-label={m.edit_vector({}, options)}
    title={m.edit_vector({}, options)}><Pencil size={14} aria-hidden="true" /></Popover.Trigger
  >
  <Popover.Portal>
    <Popover.Content
      collisionPadding={12}
      class="settings-popover"
      sideOffset={8}
      align="end"
      aria-label={m.edit_vector({}, options)}
    >
      <div class="popover-header">
        <h2>{m.edit_vector({}, options)}</h2>
        <Popover.Close class="icon-button" aria-label={m.close({}, options)}
          ><X size={16} aria-hidden="true" /></Popover.Close
        >
      </div>
      <form
        class="machine-edit"
        aria-label={m.edit_vector({}, options)}
        onsubmit={(event) => {
          event.preventDefault();
          if (!editable || !selected) return;
          if (parsed === null) {
            input?.setCustomValidity(m.vector_invalid({ format }, options));
            input?.reportValidity();
            return;
          }
          onwrite({ name: selected.name, width, lane, value: parsed });
        }}
      >
        <label
          >{m.register_name({}, options)}<select
            bind:value={
              () => register,
              (next: number) => {
                register = next;
                readValue();
              }
            }
          >
            {#each entries as entry, index (entry.name)}<option value={index}
                >{entry.name.toUpperCase()}</option
              >{/each}
          </select></label
        >
        <label
          >{m.vector_format({}, options)}<select
            bind:value={
              () => format,
              (next: LaneFormat) => {
                format = next;
                lane = 0;
                readValue();
              }
            }
          >
            {#each formats as choice (choice)}
              <option value={choice}>{choice === 'hex' ? m.vector_hex({}, options) : choice}</option
              >
            {/each}
          </select></label
        >
        {#if format !== 'hex'}
          <label
            >{m.vector_lane({}, options)}<select
              bind:value={
                () => lane,
                (next: number) => {
                  lane = next;
                  readValue();
                }
              }
            >
              {#each Array.from({ length: bits / width }, (_, index) => index) as index (index)}
                <option value={index}>{index}</option>
              {/each}
            </select></label
          >
        {/if}
        <label
          >{m.register_value({}, options)}<input
            bind:this={input}
            bind:value
            required
            maxlength="514"
            spellcheck="false"
            autocomplete="off"
            oninput={(event) => {
              event.currentTarget.setCustomValidity('');
            }}
          /></label
        >
        <p class="muted-note compact">
          {format === 'hex'
            ? m.vector_raw_hint({ digits: String(bits / 4) }, options)
            : format.startsWith('f')
              ? m.vector_float_hint({}, options)
              : m.vector_integer_hint({}, options)}
        </p>
        {#if parsed !== null}<output class="vector-preview"
            ><span>{m.vector_bits({}, options)}</span><code>{parsed}</code></output
          >{/if}
        <p class="muted-note compact">{m.vector_edit_hint({}, options)}</p>
        <button disabled={!editable}
          ><Check size={14} aria-hidden="true" />{m.write_register({}, options)}</button
        >
      </form>
    </Popover.Content>
  </Popover.Portal>
</Popover.Root>

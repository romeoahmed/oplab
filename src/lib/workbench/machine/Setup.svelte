<script lang="ts">
  import * as m from '$lib/paraglide/messages.js';
  import type { Locale } from '$lib/paraglide/runtime';
  import type { Target } from '$lib/protocol/generated/Target';
  import { Plus, Trash } from '@lucide/svelte';

  import { registerNames, type SetupInput } from './setup';

  import './setup.css';

  let {
    value = $bindable(),
    target,
    locale,
  }: { value: SetupInput; target: Target; locale: Locale } = $props();
  const options = $derived({ locale });
  const names = $derived(registerNames(target));
  const available = $derived(
    names.filter((name) => !value.registers.some((row) => row.name === name)),
  );
</script>

<div class="machine-setup">
  <p class="muted-note">{m.initial_setup_hint({}, options)}</p>
  <details>
    <summary>{m.initial_registers({}, options)} <span>{value.registers.length}</span></summary>
    <p class="muted-note">{m.initial_registers_hint({}, options)}</p>
    <div class="setup-rows">
      {#each value.registers as row (row)}
        <div class="setup-register">
          <select bind:value={row.name} aria-label={m.register_name({}, options)}>
            {#each names.filter((name) => name === row.name || available.includes(name)) as name (name)}
              <option value={name}>{name.toUpperCase()}</option>
            {/each}
          </select>
          <input
            bind:value={row.value}
            aria-label={m.initial_register_value({ name: row.name.toUpperCase() }, options)}
            spellcheck="false"
            required
            pattern="(?:[0-9]+|0[xX][0-9a-fA-F]+)"
          />
          <button
            type="button"
            class="icon-button"
            aria-label={m.remove_register({ name: row.name.toUpperCase() }, options)}
            onclick={() => {
              value.registers = value.registers.filter((item) => item !== row);
            }}><Trash size={14} aria-hidden="true" /></button
          >
        </div>
      {/each}
      <button
        type="button"
        class="text-button"
        disabled={available.length === 0}
        onclick={() => {
          const name = available[0];
          if (name !== undefined) value.registers.push({ name, value: '0' });
        }}><Plus size={14} aria-hidden="true" />{m.add_register({}, options)}</button
      >
    </div>
  </details>
  <details>
    <summary>{m.extra_memory({}, options)} <span>{value.mappings.length}</span></summary>
    <p class="muted-note">{m.extra_memory_hint({}, options)}</p>
    <div class="setup-rows">
      {#each value.mappings as row, index (row)}
        <fieldset class="setup-mapping">
          <legend>{m.memory_region({ number: index + 1 }, options)}</legend>
          <label
            >{m.address({}, options)}<input
              bind:value={row.address}
              spellcheck="false"
              required
            /></label
          >
          <label
            >{m.mapping_size({}, options)}<input
              bind:value={row.length}
              spellcheck="false"
              required
              pattern="(?:[0-9]+|0[xX][0-9a-fA-F]+)"
            /></label
          >
          <label
            >{m.mapping_permissions({}, options)}<select bind:value={row.flags}>
              {#each [6, 4, 5, 7, 2, 1, 3, 0] as flags (flags)}
                <option value={flags}
                  >{flags & 4 ? 'R' : '-'}{flags & 2 ? 'W' : '-'}{flags & 1 ? 'X' : '-'}</option
                >
              {/each}
            </select></label
          >
          <button
            type="button"
            class="icon-button"
            aria-label={m.remove_mapping({ number: index + 1 }, options)}
            onclick={() => {
              value.mappings = value.mappings.filter((item) => item !== row);
            }}><Trash size={14} aria-hidden="true" /></button
          >
        </fieldset>
      {/each}
      <button
        type="button"
        class="text-button"
        disabled={value.mappings.length >= 63}
        onclick={() => {
          value.mappings.push({ address: '', length: '4096', flags: 6 });
        }}><Plus size={14} aria-hidden="true" />{m.add_mapping({}, options)}</button
      >
    </div>
  </details>
</div>

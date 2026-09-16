<script lang="ts">
  import * as m from '$lib/paraglide/messages.js';
  import type { Locale } from '$lib/paraglide/runtime';
  import type { Registers } from '$lib/protocol/generated/Registers';
  import { Check, Pencil, X } from '@lucide/svelte';
  import { Popover } from 'bits-ui';

  import { registerNames } from './setup';

  import './editing.css';
  const {
    bank,
    locale,
    editable,
    onwrite,
  }: {
    bank: Registers | null;
    locale: Locale;
    editable: boolean;
    onwrite: (name: string, value: string) => void;
  } = $props();
  let value = $state('');
  const options = $derived({ locale });
  const names = $derived(bank === null ? [] : registerNames(bank.type));
  let name = $derived(names[0] ?? '');
  const values = $derived(
    bank === null
      ? []
      : bank.type === 'x86_64'
        ? [
            { name: 'RIP', value: bank.data.rip },
            { name: 'RFLAGS', value: bank.data.rflags },
            ...bank.data.gpr.map((value, index) => ({
              name: registerNames('x86_64')[index]?.toUpperCase() ?? '',
              value,
            })),
          ]
        : [
            { name: 'PC', value: bank.data.pc },
            { name: 'NZCV', value: String(bank.data.nzcv) },
            { name: 'SP', value: bank.data.sp },
            ...bank.data.x.map((value, index) => ({ name: `X${String(index)}`, value })),
          ],
  );
</script>

<section class="register-panel" aria-label={m.registers({}, options)}>
  <div class="section-heading">
    <h2>{m.registers({}, options)}</h2>
    <span>{m.integer_bank({}, options)}</span>
    <Popover.Root>
      <Popover.Trigger
        class="icon-button"
        disabled={!editable}
        aria-label={m.edit_register({}, options)}
        ><Pencil size={14} aria-hidden="true" /></Popover.Trigger
      >
      <Popover.Portal>
        <Popover.Content
          class="settings-popover"
          sideOffset={8}
          align="end"
          aria-label={m.edit_register({}, options)}
        >
          <div class="popover-header">
            <h2>{m.edit_register({}, options)}</h2>
            <Popover.Close class="icon-button" aria-label={m.close({}, options)}
              ><X size={16} aria-hidden="true" /></Popover.Close
            >
          </div>
          <form
            class="machine-edit"
            onsubmit={(event) => {
              event.preventDefault();
              onwrite(name, value);
            }}
          >
            <label
              >{m.register_name({}, options)}<select bind:value={name}>
                {#each names as register (register)}<option value={register}
                    >{register.toUpperCase()}</option
                  >{/each}
              </select></label
            >
            <label
              >{m.register_value({}, options)}<input
                required
                bind:value
                spellcheck="false"
                placeholder="0x2a"
              /></label
            >
            <p class="muted-note">{m.register_edit_hint({}, options)}</p>
            <button disabled={!editable}
              ><Check size={14} aria-hidden="true" />{m.write_register({}, options)}</button
            >
          </form>
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  </div>
  {#if bank === null}<p class="muted-note">{m.no_registers({}, options)}</p>
  {:else}<dl class="register-grid">
      {#each values as register (register.name)}
        <div>
          <dt>{register.name}</dt>
          <dd>{BigInt(register.value).toString(16).padStart(16, '0')}</dd>
        </div>
      {/each}
    </dl>
    <p class="muted-note compact">{m.raw_flags({}, options)}</p>{/if}
</section>

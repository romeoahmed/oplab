<script lang="ts">
  import * as m from '$lib/paraglide/messages.js';
  import type { Locale } from '$lib/paraglide/runtime';
  import type { Registers } from '$lib/protocol/generated/Registers';
  import { Check, Pencil, X } from '@lucide/svelte';
  import { Popover } from 'bits-ui';

  import { registerGroups, registerNames } from './registers';

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
  const groups = $derived(bank === null ? [] : registerGroups(bank.type));
  let name = $derived(groups[0]?.names[0] ?? '');
  const kind = $derived(groups.find((group) => group.names.includes(name))?.kind);
  const hint = $derived.by(() => {
    switch (kind) {
      case 'pc':
        return m.register_pc_hint({}, options);
      case 'flags':
        return m.register_flag_hint({}, options);
      case '32':
        return m.register_zero_extend_hint({}, options);
      case '16':
      case '8':
        return m.register_partial_hint({}, options);
      case '64':
      case undefined:
        return '';
    }
  });
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
        title={m.edit_register({}, options)}
        ><Pencil size={14} aria-hidden="true" /></Popover.Trigger
      >
      <Popover.Portal>
        <Popover.Content
          collisionPadding={12}
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
                {#each groups as group (group.kind)}
                  <optgroup
                    label={group.kind === 'pc'
                      ? m.program_counter({}, options)
                      : group.kind === 'flags'
                        ? m.register_flags({}, options)
                        : m.register_width({ bits: group.kind }, options)}
                  >
                    {#each group.names as register (register)}<option value={register}
                        >{register.toUpperCase()}</option
                      >{/each}
                  </optgroup>
                {/each}
              </select></label
            >
            <label
              >{m.register_value({}, options)}<input
                required
                bind:value
                spellcheck="false"
                pattern={kind === 'flags' ? '[01]' : undefined}
                title={kind === 'flags' ? hint : undefined}
                placeholder={kind === 'flags' ? '0 / 1' : kind === 'pc' ? '0x1000' : '0x2a'}
              /></label
            >
            <p class="muted-note">
              {#if kind !== 'flags'}{m.register_edit_hint({}, options)}
              {/if}{hint}
            </p>
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
        {@const digits = BigInt(register.value).toString(16)}
        <div class:program-counter={register.name === 'PC' || register.name === 'RIP'}>
          <dt>{register.name}</dt>
          <dd><span class="numeric-padding">{'0'.repeat(16 - digits.length)}</span>{digits}</dd>
        </div>
      {/each}
    </dl>
    <p class="muted-note compact">{m.raw_flags({}, options)}</p>{/if}
</section>

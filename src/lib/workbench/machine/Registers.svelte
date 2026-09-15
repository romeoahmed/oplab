<script lang="ts">
  import * as m from '$lib/paraglide/messages.js';
  import type { Locale } from '$lib/paraglide/runtime';
  import type { Registers } from '$lib/protocol/generated/Registers';

  import { registerNames } from './setup';
  const { bank, locale }: { bank: Registers | null; locale: Locale } = $props();
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

<section class="register-panel" aria-label={m.registers({}, { locale })}>
  <div class="section-heading">
    <h2>{m.registers({}, { locale })}</h2>
    <span>{m.integer_bank({}, { locale })}</span>
  </div>
  {#if bank === null}<p class="muted-note">{m.no_registers({}, { locale })}</p>
  {:else}<dl class="register-grid">
      {#each values as register (register.name)}
        <div>
          <dt>{register.name}</dt>
          <dd>{BigInt(register.value).toString(16).padStart(16, '0')}</dd>
        </div>
      {/each}
    </dl>
    <p class="muted-note compact">{m.raw_flags({}, { locale })}</p>{/if}
</section>

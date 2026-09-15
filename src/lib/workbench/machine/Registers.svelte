<script lang="ts">
  import type { Registers } from '$lib/protocol/generated/Registers';
  import type { Locale } from '$lib/paraglide/runtime';
  import * as m from '$lib/paraglide/messages.js';
  const { bank, locale }: { bank: Registers | null; locale: Locale } = $props();
  const x86 = [
    'RAX',
    'RCX',
    'RDX',
    'RBX',
    'RSP',
    'RBP',
    'RSI',
    'RDI',
    'R8',
    'R9',
    'R10',
    'R11',
    'R12',
    'R13',
    'R14',
    'R15',
  ];
  const values = $derived(
    bank === null
      ? []
      : bank.type === 'x86_64'
        ? [
            { name: 'RIP', value: bank.data.rip },
            { name: 'RFLAGS', value: bank.data.rflags },
            ...bank.data.gpr.map((value, index) => ({ name: x86[index] ?? '', value })),
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

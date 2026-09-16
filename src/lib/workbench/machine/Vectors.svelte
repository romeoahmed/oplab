<script lang="ts">
  import * as m from '$lib/paraglide/messages.js';
  import type { Locale } from '$lib/paraglide/runtime';
  import type { Registers } from '$lib/protocol/generated/Registers';

  import { laneFormats, vectorLanes, type LaneFormat } from './vectors';

  import './vectors.css';

  const { bank, locale }: { bank: Registers | null; locale: Locale } = $props();
  const options = $derived({ locale });
  let format = $state<LaneFormat>('hex');
  const values = $derived(
    bank === null ? [] : bank.type === 'x86_64' ? bank.data.xmm : bank.data.v,
  );
  const controls = $derived(
    bank === null
      ? []
      : bank.type === 'x86_64'
        ? [{ name: 'MXCSR', value: bank.data.mxcsr }]
        : [
            { name: 'FPCR', value: bank.data.fpcr },
            { name: 'FPSR', value: bank.data.fpsr },
          ],
  );
</script>

<section class="vector-panel" aria-label={m.simd_registers({}, options)}>
  <div class="section-heading">
    <h2>{m.simd_registers({}, options)}</h2>
    <span>{m.register_width({ bits: '128' }, options)}</span>
  </div>
  {#if bank === null}
    <p class="muted-note">{m.no_registers({}, options)}</p>
  {:else}
    <label class="vector-format"
      >{m.vector_format({}, options)}
      <select bind:value={format}>
        {#each laneFormats as choice (choice)}
          <option value={choice}>{choice === 'hex' ? m.vector_hex({}, options) : choice}</option>
        {/each}
      </select>
    </label>
    <p class="muted-note compact">{m.vector_lanes_hint({}, options)}</p>
    <dl class="register-grid">
      {#each controls as control (control.name)}
        <div>
          <dt>{control.name}</dt>
          <dd>{control.value.toString(16).padStart(8, '0')}</dd>
        </div>
      {/each}
    </dl>
    {#if bank.type === 'x86_64'}<p class="muted-note compact">
        {m.vector_x86_status({}, options)}
      </p>{/if}
    <dl class="vector-bank">
      {#each values as value, index (index)}
        <div>
          <dt>{bank.type === 'x86_64' ? 'XMM' : 'V'}{index}</dt>
          <dd>
            <code class="vector-bits">{value.slice(2)}</code>
            {#if format !== 'hex'}
              <ol class="vector-lanes" start="0">
                {#each vectorLanes(value, format) as lane, position (position)}
                  <li><code>{lane}</code></li>
                {/each}
              </ol>
            {/if}
          </dd>
        </div>
      {/each}
    </dl>
    <p class="muted-note">
      {bank.type === 'x86_64' ? m.vector_x86_scope({}, options) : m.vector_arm_scope({}, options)}
    </p>
  {/if}
</section>

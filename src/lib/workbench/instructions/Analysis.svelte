<script lang="ts">
  import * as m from '$lib/paraglide/messages.js';
  import type { Locale } from '$lib/paraglide/runtime';
  import type { DataAccess } from '$lib/protocol/generated/DataAccess';
  import type { DecodedInstruction } from '$lib/protocol/generated/DecodedInstruction';
  import type { FlowControl } from '$lib/protocol/generated/FlowControl';
  import type { InstructionAnalysis } from '$lib/protocol/generated/InstructionAnalysis';
  import { RefreshCw, ScanSearch, X } from '@lucide/svelte';

  import { problemLabel } from '../presentation';

  const {
    instruction,
    request,
    locale,
    retry,
    close,
  }: {
    instruction: DecodedInstruction;
    request: Promise<InstructionAnalysis>;
    locale: Locale;
    retry: () => void;
    close: () => void;
  } = $props();
  const options = $derived({ locale });
  function failureCode(failure: unknown) {
    return typeof failure === 'object' &&
      failure !== null &&
      'code' in failure &&
      typeof failure.code === 'string'
      ? failure.code
      : 'unavailable';
  }

  const accessMessages = {
    unknown: m.analysis_unknown,
    read: m.analysis_read,
    conditional_read: m.analysis_conditional_read,
    write: m.analysis_write,
    conditional_write: m.analysis_conditional_write,
    read_write: m.analysis_read_write,
    read_conditional_write: m.analysis_read_conditional_write,
  } satisfies Record<DataAccess, typeof m.analysis_read>;
  const flagMessages = [
    ['read', m.analysis_flags_read],
    ['written', m.analysis_flags_written],
    ['cleared', m.analysis_flags_cleared],
    ['set', m.analysis_flags_set],
    ['undefined', m.analysis_flags_undefined],
  ] as const;
  const flowMessages = {
    next: m.flow_next,
    branch: m.flow_branch,
    indirect_branch: m.flow_indirect_branch,
    conditional_branch: m.flow_conditional_branch,
    call: m.flow_call,
    indirect_call: m.flow_indirect_call,
    return: m.flow_return,
    interrupt: m.flow_interrupt,
    transaction: m.flow_transaction,
    exception: m.flow_exception,
  } satisfies Record<FlowControl, typeof m.flow_next>;
</script>

<section class="instruction-analysis" aria-label={m.instruction_analysis({}, options)}>
  <div class="analysis-heading">
    <h3><ScanSearch size={15} aria-hidden="true" />{m.instruction_analysis({}, options)}</h3>
    <button type="button" class="icon-button" aria-label={m.close({}, options)} onclick={close}
      ><X size={14} aria-hidden="true" /></button
    >
  </div>
  <p class="analysis-identity"><code>{instruction.address}</code><code>{instruction.text}</code></p>
  <p class="instruction-note">{m.analysis_hint({}, options)}</p>
  {#await request}
    <p role="status">{m.analysis_loading({}, options)}</p>
  {:then value}
    <h4>{m.registers({}, options)}</h4>
    {#if value.registers.length === 0}<p class="instruction-note">
        {m.analysis_unreported({}, options)}
      </p>
    {:else}<dl class="analysis-facts">
        {#each value.registers as register (register)}<div>
            <dt><code>{register.name}</code></dt>
            <dd>{accessMessages[register.access]({}, options)}</dd>
          </div>{/each}
      </dl>{/if}
    {#if value.architecture.type === 'x86' && value.architecture.data.registers_incomplete}
      <p class="instruction-note">{m.analysis_partial_registers({}, options)}</p>
    {/if}
    {#if value.memory.length > 0}
      <h4>{m.memory({}, options)}</h4>
      <ul class="analysis-memory">
        {#each value.memory as memory (memory)}<li>
            {accessMessages[memory.access]({}, options)} · {memory.bytes === null
              ? m.analysis_unknown_size({}, options)
              : `${memory.bytes.toString()} B`}
          </li>{/each}
      </ul>
    {/if}
    <dl class="analysis-facts">
      {#if value.branch_target !== null}<div>
          <dt>{m.analysis_destination({}, options)}</dt>
          <dd><code>{value.branch_target}</code></dd>
        </div>{/if}
      {#if value.architecture.type === 'x86'}
        {@const detail = value.architecture.data}
        <div>
          <dt>{m.analysis_flow({}, options)}</dt>
          <dd>{flowMessages[detail.flow]({}, options)}</dd>
        </div>
        <div>
          <dt>CPUID</dt>
          <dd><code>{detail.cpuid.join(' · ') || '—'}</code></dd>
        </div>
        {#each flagMessages as [kind, message] (kind)}
          {@const flags = detail.flags[kind]}
          {#if flags.length > 0}<div>
              <dt>{message({}, options)}</dt>
              <dd><code>{flags.join(' · ')}</code></dd>
            </div>{/if}
        {/each}
        {#if detail.privileged}<div>
            <dt>{m.analysis_privileged({}, options)}</dt>
            <dd>{m.analysis_yes({}, options)}</dd>
          </div>{/if}
      {:else if value.architecture.type === 'aarch64'}
        {@const detail = value.architecture.data}
        <div>
          <dt>{m.analysis_groups({}, options)}</dt>
          <dd><code>{detail.groups.join(' · ') || '—'}</code></dd>
        </div>
        {#if detail.updates_flags}<div>
            <dt>{m.analysis_updates_flags({}, options)}</dt>
            <dd>NZCV</dd>
          </div>{/if}
        {#if detail.writeback}<div>
            <dt>{m.analysis_writeback({}, options)}</dt>
            <dd>{m.analysis_yes({}, options)}</dd>
          </div>{/if}
      {/if}
    </dl>
  {:catch failure}
    <p role="alert">{problemLabel(failureCode(failure), locale)}</p>
    <button type="button" onclick={retry}
      ><RefreshCw size={14} aria-hidden="true" />{m.analysis_retry({}, options)}</button
    >
  {/await}
</section>

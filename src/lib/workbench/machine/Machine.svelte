<script lang="ts">
  import './machine.css';
  import * as m from '$lib/paraglide/messages.js';
  import type { Locale } from '$lib/paraglide/runtime';
  import type { Observation } from '$lib/protocol/generated/Observation';
  import type { RoundingMode } from '$lib/protocol/generated/RoundingMode';
  import type { VectorWrite } from '$lib/protocol/generated/VectorWrite';
  import { Cpu, Keyboard, Binary, Rows3, Circle } from '@lucide/svelte';
  import { Tabs } from 'bits-ui';

  import { stateLabel } from '../presentation';
  import Breakpoints from './Breakpoints.svelte';
  import Registers from './Registers.svelte';
  import Vectors from './Vectors.svelte';
  const {
    observation,
    loadedCurrent,
    loadedRevision,
    loadedKind,
    editable,
    onbreakpoint,
    onregister,
    onvector,
    onrounding,
    locale,
  }: {
    observation: Observation | undefined;
    loadedCurrent: boolean;
    loadedRevision: string | null;
    loadedKind: 'source' | 'raw' | null;
    editable: boolean;
    onbreakpoint: (address: string, enabled: boolean) => void;
    onregister: (name: string, value: string) => void;
    onrounding: (mode: RoundingMode) => void;
    onvector: (write: VectorWrite) => void;
    locale: Locale;
  } = $props();
  const options = $derived({ locale });
  const running = $derived(observation?.status.type === 'running');
</script>

<aside class="inspector" aria-label={m.machine({}, options)}>
  <header class="pane-header">
    <h2><Cpu size={17} aria-hidden="true" />{m.machine({}, options)}</h2>
    <span
      class="machine-status"
      class:active={running}
      class:faulted={observation !== undefined && observation.fault !== null}
      role="status">{stateLabel(observation?.status, locale)}</span
    >
  </header>
  <div class="inspector-body">
    {#if observation === undefined}
      <div class="empty-machine">
        <span class="empty-icon"><Cpu size={24} strokeWidth={1.5} aria-hidden="true" /></span>
        <h3>{m.machine_empty_title({}, options)}</h3>
        <p>{m.machine_empty_hint({}, options)}</p>
      </div>
      <ol class="workflow-steps">
        <li>
          <span>01</span>
          <div>
            <strong>{m.assemble({}, options)}</strong>
            <p>{m.workflow_build({}, options)}</p>
          </div>
        </li>
        <li>
          <span>02</span>
          <div>
            <strong>{m.load_artifact({}, options)}</strong>
            <p>{m.workflow_load({}, options)}</p>
          </div>
        </li>
        <li>
          <span>03</span>
          <div>
            <strong>{m.step({}, options)} / {m.run({}, options)}</strong>
            <p>{m.workflow_run({}, options)}</p>
          </div>
        </li>
      </ol>
    {:else}
      <div class="session-heading">
        <span class="architecture-label"
          >{observation.registers?.type === 'x86_64'
            ? 'x86_64'
            : observation.registers?.type === 'aarch64'
              ? 'AArch64'
              : '—'}</span
        ><span
          >{loadedKind === 'raw'
            ? m.loaded_raw({}, options)
            : loadedRevision === null
              ? m.existing_session({}, options)
              : m.loaded_revision({ revision: loadedRevision }, options)}</span
        >
      </div>
      {#if !loadedCurrent}<p class="revision-warning">
          {loadedKind === 'raw'
            ? m.raw_changed({}, options)
            : loadedRevision === null
              ? m.source_unlinked({}, options)
              : m.source_changed({}, options)}
        </p>{/if}
      <dl class="execution-counts">
        <div>
          <dt>{m.instructions_started({}, options)}</dt>
          <dd>{observation.instructions}</dd>
        </div>
      </dl>
      <Tabs.Root value="integer" class="machine-banks">
        <Tabs.List class="panel-tabs" aria-label={m.machine_views({}, options)}>
          <Tabs.Trigger value="integer"
            ><Binary size={14} aria-hidden="true" />{m.integer_view({}, options)}</Tabs.Trigger
          >
          <Tabs.Trigger value="simd"><Rows3 size={14} aria-hidden="true" />SIMD</Tabs.Trigger>
          <Tabs.Trigger value="breakpoints">
            <Circle size={12} aria-hidden="true" />{m.breakpoint_view({}, options)}
            <span class="tab-count">{observation.breakpoints.length}</span>
          </Tabs.Trigger>
        </Tabs.List>
        <Tabs.Content value="integer"
          ><Registers
            bank={observation.registers}
            {locale}
            {editable}
            onwrite={onregister}
          /></Tabs.Content
        >
        <Tabs.Content value="simd">
          {#key observation.registers?.type}<Vectors
              bank={observation.registers}
              {locale}
              {editable}
              onwrite={onvector}
              {onrounding}
            />{/key}
        </Tabs.Content>
        <Tabs.Content value="breakpoints">
          <Breakpoints
            addresses={observation.breakpoints}
            disabled={!editable}
            {locale}
            onchange={onbreakpoint}
          />
        </Tabs.Content>
      </Tabs.Root>
    {/if}
  </div>
  <div class="inspector-footer">
    <Keyboard size={15} aria-hidden="true" /><span>{m.step_over({}, options)} <kbd>F10</kbd></span
    ><span>{m.run({}, options)} <kbd>F5</kbd></span>
  </div>
</aside>

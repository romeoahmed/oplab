<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import {
    Hammer,
    Download,
    Play,
    StepForward,
    Pause,
    Square,
    RotateCcw,
    FileCode,
    Cpu,
  } from '@lucide/svelte';
  import Editor from './Editor.svelte';
  import Registers from './Registers.svelte';
  import Memory from './Memory.svelte';
  import { createLocaleController } from '$lib/i18n/locale.svelte';
  import { createWorkbench, examples } from './controller.svelte';
  import { problemLabel, stateLabel } from './presentation';
  import * as m from '$lib/paraglide/messages.js';
  import './workbench.css';

  const { portFactory }: { portFactory?: Parameters<typeof createWorkbench>[0] } = $props();
  const language = createLocaleController();
  const work = createWorkbench(untrack(() => portFactory));
  let initialized = $state(false);
  const options = $derived({ locale: language.current });
  const observation = $derived(work.snapshot?.observation);
  const running = $derived(observation?.status.type === 'running');
  const resumable = $derived(
    observation !== undefined &&
      ['ready', 'paused', 'stepped', 'breakpoint'].includes(observation.status.type),
  );
  const lines = $derived(work.source.split('\n').length);

  onMount(() => {
    language.initialize();
    work.initialize();
    initialized = true;
    return () => {
      work.dispose();
    };
  });
  function shortcuts(event: KeyboardEvent) {
    if (!work.connected || event.isComposing || event.defaultPrevented) return;
    if ((event.metaKey || event.ctrlKey) && event.key === 'Enter') {
      event.preventDefault();
      void work.assemble();
    }
    if (event.key === 'F10' && resumable) {
      event.preventDefault();
      void work.execute({ type: 'step' });
    }
    if (event.key === 'F5' && resumable) {
      event.preventDefault();
      void work.execute({ type: 'run' });
    }
  }
</script>

<svelte:window onkeydown={shortcuts} />
{#if initialized}
  <main class="workbench" aria-label={m.workspace({}, options)}>
    <header class="topbar">
      <div class="brand">
        <img class="brand-mark" src="/icon.svg" alt="" width="28" height="28" /><strong
          >oplab</strong
        >
      </div>
      <span class="divider" aria-hidden="true"></span>
      <span class="experiment-name">{m.experiment({}, options)}</span>
      <span class="revision-tag">r{work.revision}</span>
      <div class="toolbar-end">
        <label class="target-picker"
          ><span class="sr-only">{m.target({}, options)}</span>
          <select
            value={work.target}
            onchange={(event) => {
              work.setTarget(event.currentTarget.value === 'aarch64' ? 'aarch64' : 'x86_64');
            }}
          >
            <option value="x86_64">x86_64</option><option value="aarch64">AArch64</option>
          </select>
        </label>
        <label class="locale-picker">
          <span class="sr-only">{m.language({}, options)}</span>
          <select
            value={language.current}
            onchange={(event) => {
              void language.change(event.currentTarget.value === 'zh-CN' ? 'zh-CN' : 'en');
            }}
          >
            <option value="en" lang="en">English</option>
            <option value="zh-CN" lang="zh-CN">简体中文</option>
          </select>
        </label>
      </div>
    </header>

    <div class="execution-toolbar" role="group" aria-label={m.workspace({}, options)}>
      <button
        class="primary"
        onclick={() => {
          void work.assemble();
        }}
        disabled={!work.connected || work.building}
        aria-keyshortcuts="Meta+Enter Control+Enter"
      >
        <Hammer size={15} aria-hidden="true" />
        {work.building ? m.assembling({}, options) : m.assemble({}, options)}
      </button>
      <button
        onclick={() => {
          void work.load();
        }}
        disabled={!work.connected || !work.artifactCurrent || work.controlling || running}
        ><Download size={15} aria-hidden="true" />{m.load_artifact({}, options)}</button
      >
      <span class="divider" aria-hidden="true"></span>
      <button
        onclick={() => {
          void work.execute({ type: 'run' });
        }}
        disabled={!work.connected || !resumable || work.controlling}
        aria-keyshortcuts="F5"><Play size={15} aria-hidden="true" /> {m.run({}, options)}</button
      >
      <button
        onclick={() => {
          void work.execute({ type: 'step' });
        }}
        disabled={!work.connected || !resumable || work.controlling}
        aria-keyshortcuts="F10"
        ><StepForward size={15} aria-hidden="true" /> {m.step({}, options)}</button
      >
      <button
        onclick={() => {
          void work.execute({ type: 'pause' });
        }}
        disabled={!work.connected || !running || work.controlling}
        ><Pause size={15} aria-hidden="true" />{m.pause({}, options)}</button
      >
      <button
        onclick={() => {
          void work.execute({ type: 'cancel' });
        }}
        disabled={!work.connected || (!running && !resumable) || work.controlling}
        ><Square size={15} aria-hidden="true" />{m.cancel({}, options)}</button
      >
      <button
        onclick={() => {
          void work.execute({ type: 'reset' });
        }}
        disabled={!work.connected || observation === undefined || running || work.controlling}
        ><RotateCcw size={15} aria-hidden="true" />{m.reset({}, options)}</button
      >
      <span class="machine-status" class:active={running} role="status"
        >{stateLabel(observation?.status, language.current)}</span
      >
    </div>

    <div class="workspace-grid">
      <section class="source-pane" aria-label={m.source({}, options)}>
        <div class="pane-header">
          <span class="active-tab"><FileCode size={16} aria-hidden="true" /> experiment.asm</span>
          <button
            class="text-button"
            onclick={() => {
              work.setSource(examples[work.target]);
            }}>{m.load_example({}, options)}</button
          >
        </div>
        <div class="editor-body">
          <Editor
            value={work.source}
            locale={language.current}
            onchange={(value: string) => {
              work.setSource(value);
            }}
          />
        </div>
        <div class="artifact-strip">
          {#if work.candidate !== null}<span class:stale={!work.artifactCurrent}
              >{m.built_revision(
                { revision: work.candidate.artifact.identity.revision },
                options,
              )}</span
            ><span>{work.candidate.artifact.image_bytes} B · ELF</span>
          {:else}<span>{m.no_artifact({}, options)}</span>{/if}
          {#if observation !== undefined}<span class:stale={!work.loadedCurrent}
              >{work.loadedRevision === null
                ? m.existing_session({}, options)
                : m.loaded_revision({ revision: work.loadedRevision }, options)}</span
            >{/if}
        </div>
      </section>
      <aside class="inspector" aria-label={m.inspector({}, options)}>
        <div class="pane-header">
          <span>{m.inspector({}, options)}</span><Cpu size={16} aria-hidden="true" />
        </div>
        <div class="inspector-body">
          <p class="eyebrow">{m.app_description({}, options)}</p>
          <div class="experiment-settings">
            <label
              >{m.build_address({}, options)}<input
                bind:value={work.base}
                spellcheck="false"
              /></label
            >
            <label
              >{m.completion({}, options)}<input
                bind:value={work.completion}
                spellcheck="false"
              /></label
            >
            <label
              >{m.instruction_budget({}, options)}<input
                bind:value={work.budget}
                inputmode="numeric"
                spellcheck="false"
              /></label
            >
          </div>
          {#if observation !== undefined}
            <div class="execution-summary">
              <span>{m.generation({ generation: observation.key.generation }, options)}</span>
              <span>{m.observation({ sequence: observation.sequence }, options)}</span>
            </div>
            <dl class="execution-counts">
              <div>
                <dt>{m.instructions({}, options)}</dt>
                <dd>{observation.instructions}</dd>
              </div>
              <div>
                <dt>{m.dispatches({}, options)}</dt>
                <dd>{observation.dispatches}</dd>
              </div>
            </dl>
            {#if !work.loadedCurrent}<p class="revision-warning">
                {m.source_changed({}, options)}
              </p>{/if}
          {/if}
          <Registers bank={observation?.registers ?? null} locale={language.current} />
        </div>
      </aside>
      <section class="memory-pane" aria-label={m.memory({}, options)}>
        <div class="pane-header">
          <span>{m.memory({}, options)}</span>
          <form
            class="memory-controls"
            onsubmit={(event) => {
              event.preventDefault();
              void work.inspect();
            }}
          >
            <input
              aria-label={m.address({}, options)}
              bind:value={work.memoryAddress}
              spellcheck="false"
            />
            <input
              aria-label={m.window_size({}, options)}
              type="number"
              min="1"
              max="4096"
              bind:value={work.memoryLength}
            />
            <button disabled={!work.connected || observation === undefined}
              >{m.inspect_memory({}, options)}</button
            >
          </form>
        </div>
        <Memory
          window={observation?.memory ?? null}
          bytes={work.snapshot?.memory ?? null}
          locale={language.current}
        />
      </section>
    </div>
    {#if work.problem !== null}<div class="error-banner" role="alert">
        <span
          >{problemLabel(work.problem, language.current)}
          {work.unknown ? m.unknown_outcome({}, options) : ''}</span
        >
        {#if !work.connected && !work.preview}<button
            onclick={() => {
              void work.connect(true);
            }}
            disabled={work.connecting}>{m.reconnect({}, options)}</button
          >{/if}
      </div>{/if}
    {#if language.failed}<p class="error" role="alert">{m.locale_failed({}, options)}</p>{/if}
    <footer class="statusbar">
      <span
        ><span class="status-dot" class:offline={!work.connected} aria-hidden="true"
        ></span>{work.preview
          ? m.preview_notice({}, options)
          : work.connecting
            ? m.connecting({}, options)
            : work.connected
              ? m.connected({}, options)
              : m.state_crashed({}, options)}</span
      >
      <span class="development-note">{m.scratch_saved({}, options)}</span><span
        >{m.line_count({ count: String(lines) }, options)}</span
      ><span>UTF-8 · LF</span>
    </footer>
  </main>
{/if}

<script lang="ts">
  import '@fontsource-variable/jetbrains-mono/wght.css';
  import { desktopFiles, type FilePort } from '$lib/desktop/files';
  import { createLocaleController } from '$lib/i18n/locale.svelte';
  import * as m from '$lib/paraglide/messages.js';
  import type { Diagnostic } from '@codemirror/lint';
  import {
    Binary,
    Hammer,
    Import,
    Play,
    StepForward,
    Pause,
    Square,
    RotateCcw,
    FileCode,
    Settings2,
    X,
    Maximize2,
    Minimize2,
    MemoryStick,
    Box,
    ArrowRight,
    CircleCheck,
    ListTree,
    LocateFixed,
    RefreshCw,
  } from '@lucide/svelte';
  import { Toolbar, Tooltip, Popover, Tabs } from 'bits-ui';
  import { onMount, untrack } from 'svelte';

  import Appearance from './Appearance.svelte';
  import { createWorkbench, examples } from './controller.svelte';
  import { sourceInfo, sourceLocation } from './editor/source';
  import Files from './Files.svelte';
  import { segmentBytes } from './instructions/bytes';
  import Instructions from './instructions/Instructions.svelte';
  import Machine from './machine/Machine.svelte';
  import Memory from './machine/Memory.svelte';
  import Patch from './machine/Patch.svelte';
  import Raw from './machine/Raw.svelte';
  import Setup from './machine/Setup.svelte';
  import { defaultPreferences, readPreferences } from './preferences';
  import { problemLabel } from './presentation';
  import ToolbarAction from './ToolbarAction.svelte';

  import './workbench.css';

  const {
    portFactory,
    filePort = desktopFiles(),
  }: { portFactory?: Parameters<typeof createWorkbench>[0]; filePort?: FilePort | null } = $props();
  const language = createLocaleController();
  const work = createWorkbench(untrack(() => portFactory));
  let editor = $state<{ revealDiagnostic: () => void }>();
  const options = $derived({ locale: language.current });
  let byteSource = $state('');
  let observationTab = $state('memory');
  let fileProblem = $state<string | null>(null);
  const captured = $derived(work.inspected);
  const artifactSources = $derived(
    work.candidate?.artifact.image.segments
      .flatMap((segment, index) => {
        if (segment.file_bytes === 0 || work.candidate === null) return [];
        return [
          {
            id: `segment-${String(index)}`,
            label: `${segment.address} · ${segment.flags & 4 ? 'R' : '-'}${segment.flags & 2 ? 'W' : '-'}${segment.flags & 1 ? 'X' : '-'} · ${String(segment.file_bytes)} B`,
            executable: (segment.flags & 1) !== 0,
            bytes: segmentBytes(work.candidate.image, segment),
            target: work.candidate.artifact.identity.target,
            base: segment.address,
          },
        ];
      })
      .toSorted((left, right) => Number(right.executable) - Number(left.executable)) ?? [],
  );
  const byteSources = $derived([
    ...(work.binary === undefined
      ? []
      : [
          {
            id: 'imported',
            label: m.imported_binary({}, options),
            bytes: work.binary,
            target: work.raw.target,
            base: work.raw.base,
          },
        ]),
    ...(captured?.memory == null ||
    captured.observation.memory === null ||
    captured.observation.registers === null
      ? []
      : [
          {
            id: 'live',
            label: m.live_memory({}, options),
            bytes: captured.memory,
            target: captured.observation.registers.type,
            base: captured.observation.memory.address,
          },
        ]),
    ...artifactSources,
  ]);
  const selectedBytes = $derived(
    byteSources.find((item) => item.id === byteSource) ?? byteSources[0],
  );
  let initialized = $state(false);
  let preferences = $state({ ...defaultPreferences });
  let preferencesFailed = $state(false);
  let focused = $state(false);
  let cursor = $state({ line: 1, column: 1 });
  const location = $derived(sourceLocation(work.source, work.diagnostic?.source_offset ?? null));
  const diagnostic = $derived<Diagnostic | null>(
    location === null || work.diagnostic === null
      ? null
      : {
          from: location.position,
          to: location.position,
          severity: 'error',
          message: problemLabel(work.diagnostic.code, language.current),
        },
  );
  const observation = $derived(work.snapshot?.observation);
  const capturedPC = $derived(
    captured?.observation.registers?.type === 'x86_64'
      ? captured.observation.registers.data.rip
      : captured?.observation.registers?.data.pc,
  );
  const pc = $derived(
    observation?.registers?.type === 'x86_64'
      ? observation.registers.data.rip
      : observation?.registers?.data.pc,
  );
  const running = $derived(observation?.status.type === 'running');
  const resumable = $derived(
    observation !== undefined &&
      ['ready', 'paused', 'stepped', 'breakpoint'].includes(observation.status.type),
  );
  const sourceDetails = $derived(sourceInfo(work.source));
  const actions = $derived([
    {
      label: work.building ? m.assembling({}, options) : m.assemble({}, options),
      hint: m.assemble_hint({}, options),
      icon: Hammer,
      disabled: !work.connected || work.building,
      primary: true,
      prominent: true,
      shortcut: 'Meta+Enter Control+Enter',
      run: () => {
        void work.assemble();
      },
    },
    {
      label: m.load_artifact({}, options),
      hint: m.load_hint({}, options),
      icon: Import,
      prominent: true,
      disabled: !work.connected || !work.artifactCurrent || work.controlling || running,
      run: () => {
        void work.load();
      },
    },
    {
      label: m.run({}, options),
      hint: m.run_hint({}, options),
      icon: Play,
      prominent: true,
      disabled: !work.connected || !resumable || work.controlling,
      shortcut: 'F5',
      run: () => {
        void work.execute({ type: 'run' });
      },
    },
    {
      label: m.step({}, options),
      hint: m.step_hint({}, options),
      icon: StepForward,
      disabled: !work.connected || !resumable || work.controlling,
      shortcut: 'F10',
      run: () => {
        void work.execute({ type: 'step' });
      },
    },
    {
      label: m.pause({}, options),
      hint: m.pause_hint({}, options),
      icon: Pause,
      disabled: !work.connected || !running || work.controlling,
      run: () => {
        void work.execute({ type: 'pause' });
      },
    },
    {
      label: m.cancel({}, options),
      hint: m.stop_hint({}, options),
      icon: Square,
      disabled: !work.connected || (!running && !resumable) || work.controlling,
      run: () => {
        void work.execute({ type: 'cancel' });
      },
    },
    {
      label: m.reset({}, options),
      hint: m.reset_hint({}, options),
      icon: RotateCcw,
      disabled: !work.connected || observation === undefined || running || work.controlling,
      run: () => {
        void work.execute({ type: 'reset' });
      },
    },
  ]);

  onMount(() => {
    language.initialize();
    work.initialize();
    try {
      preferences = readPreferences(
        JSON.parse(localStorage.getItem('oplab.appearance.v1') ?? 'null'),
      );
    } catch {
      preferencesFailed = true;
    }
    initialized = true;
    return () => {
      work.dispose();
    };
  });
  $effect(() => {
    if (!initialized) return;
    try {
      localStorage.setItem('oplab.appearance.v1', JSON.stringify(preferences));
      preferencesFailed = false;
    } catch {
      preferencesFailed = true;
    }
  });
  function shortcuts(event: KeyboardEvent) {
    if (!work.connected || event.isComposing || event.defaultPrevented || event.repeat) return;
    if ((event.metaKey || event.ctrlKey) && event.key === 'Enter' && !work.building) {
      event.preventDefault();
      void work.assemble();
    }
    if (event.key === 'F10' && resumable && !work.controlling) {
      event.preventDefault();
      void work.execute({ type: 'step' });
    }
    if (event.key === 'F5' && resumable && !work.controlling) {
      event.preventDefault();
      void work.execute({ type: 'run' });
    }
  }
</script>

<svelte:window onkeydown={shortcuts} />
{#if initialized}
  <Tooltip.Provider delayDuration={450}>
    <main
      class="workbench"
      class:focus-mode={focused}
      class:system-font={preferences.font === 'system'}
      style:--editor-size={`${String(preferences.fontSize)}px`}
      style:--inspector-width={`${String(preferences.inspectorWidth)}%`}
      style:--memory-height={`${String(preferences.memoryHeight)}dvh`}
      aria-label={m.workspace({}, options)}
    >
      <header class="topbar">
        <h1 class="brand" aria-label="Oplab">
          <img src="/icon.svg" alt="" width="30" height="30" /><strong
            >oplab<span class="brand-period">.</span></strong
          >
        </h1>
        <Files
          port={filePort}
          locale={language.current}
          source={work.source}
          object={work.candidate?.object}
          image={work.candidate?.image}
          binary={selectedBytes?.bytes}
          onsource={(source: string) => {
            work.setSource(source);
          }}
          onbinary={(bytes: Uint8Array) => {
            work.importBinary(bytes);
            byteSource = 'imported';
            observationTab = 'raw';
            focused = false;
          }}
          onerror={(code: string | null) => {
            fileProblem = code;
          }}
        />
        <Toolbar.Root class="execution-toolbar" aria-label={m.execution_controls({}, options)}>
          {#each actions as action, index (index)}
            {#if index === 2}<span class="divider" aria-hidden="true"></span>{/if}
            <ToolbarAction
              label={action.label}
              hint={action.hint}
              disabled={action.disabled}
              primary={action.primary}
              prominent={action.prominent}
              shortcut={action.shortcut}
              onclick={action.run}><action.icon size={16} aria-hidden="true" /></ToolbarAction
            >
          {/each}
        </Toolbar.Root>
        <Popover.Root>
          <Popover.Trigger
            class="configuration-button"
            aria-label={m.configuration({}, options)}
            title={m.configuration({}, options)}
            ><Settings2 size={16} aria-hidden="true" /><span>{m.configuration({}, options)}</span
            ></Popover.Trigger
          >
          <Popover.Portal>
            <Popover.Content
              collisionPadding={12}
              class="settings-popover configuration-popover"
              sideOffset={10}
              align="end"
              aria-label={m.configuration({}, options)}
            >
              <div class="popover-header">
                <h2>{m.configuration({}, options)}</h2>
                <Popover.Close class="icon-button" aria-label={m.close({}, options)}
                  ><X size={16} aria-hidden="true" /></Popover.Close
                >
              </div>
              <div class="settings-fields">
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
              <p class="muted-note">{m.configuration_hint({}, options)}</p>
              <Setup bind:value={work.setup} target={work.target} locale={language.current} />
            </Popover.Content>
          </Popover.Portal>
        </Popover.Root>
        <div class="toolbar-end">
          <label class="locale-picker"
            ><span class="sr-only">{m.language({}, options)}</span><select
              value={language.current}
              onchange={(event) => {
                void language.change(event.currentTarget.value === 'zh-CN' ? 'zh-CN' : 'en');
              }}
              ><option value="en" lang="en">English</option><option value="zh-CN" lang="zh-CN"
                >简体中文</option
              ></select
            ></label
          >
          <Appearance bind:preferences locale={language.current} />
        </div>
      </header>
      <div class="workspace-grid">
        <section class="source-pane" aria-label={m.source({}, options)}>
          <header class="pane-header source-header">
            <div class="file-heading">
              <FileCode size={17} aria-hidden="true" />
              <h2>experiment.s</h2>
              <span class="revision-tag">r{work.revision}</span>
            </div>
            <div class="pane-tools">
              <label class="target-picker"
                ><span class="sr-only">{m.target({}, options)}</span><select
                  value={work.target}
                  onchange={(event) => {
                    work.setTarget(event.currentTarget.value === 'aarch64' ? 'aarch64' : 'x86_64');
                  }}
                  ><option value="x86_64">x86_64</option><option value="aarch64">AArch64</option
                  ></select
                ></label
              >
              <button
                class="icon-button"
                aria-label={m.focus_editor({}, options)}
                aria-pressed={focused}
                onclick={() => {
                  focused = !focused;
                }}
                >{#if focused}<Minimize2 size={16} aria-hidden="true" />{:else}<Maximize2
                    size={16}
                    aria-hidden="true"
                  />{/if}</button
              >
            </div>
          </header>
          <div class="source-context">
            <span
              >{m.source({}, options)} <span aria-hidden="true">/</span>
              {m.assembly_syntax({}, options)}</span
            ><button
              class="text-button"
              onclick={() => {
                work.setSource(examples[work.target]);
              }}>{m.load_example({}, options)}<ArrowRight size={13} aria-hidden="true" /></button
            >
          </div>
          <div class="editor-body">
            {#await import('./editor/Editor.svelte')}
              <p class="editor-message muted-note" role="status">{m.editor_loading({}, options)}</p>
            {:then { default: Editor }}
              <Editor
                bind:this={editor}
                {diagnostic}
                value={work.source}
                locale={language.current}
                target={work.target}
                wrap={preferences.wrap}
                oncursor={(line: number, column: number) => {
                  cursor = { line, column };
                }}
                onchange={(value: string) => {
                  work.setSource(value);
                }}
              />
            {:catch}
              <p class="editor-message" role="alert">{m.editor_failed({}, options)}</p>
            {/await}
          </div>
          <div class="artifact-strip">
            <span class:stale={work.candidate !== null && !work.artifactCurrent}
              ><Box size={13} aria-hidden="true" />{work.candidate === null
                ? m.no_artifact({}, options)
                : m.built_revision(
                    { revision: work.candidate.artifact.identity.revision },
                    options,
                  )}</span
            >
            {#if work.candidate !== null}<span>{work.candidate.artifact.image_bytes} B · ELF</span
              >{/if}
            <span class="cursor-position"
              >{m.cursor_position(
                { line: String(cursor.line), column: String(cursor.column) },
                options,
              )}</span
            >
          </div>
        </section>
        <Machine
          {observation}
          loadedCurrent={work.loadedCurrent}
          loadedRevision={work.loadedRevision}
          loadedKind={work.loadedKind}
          editable={work.connected && resumable && !work.controlling}
          onbreakpoint={(address: string, enabled: boolean) => {
            void work.breakpoint(address, enabled);
          }}
          onregister={(name: string, value: string) => {
            void work.writeRegister(name, value);
          }}
          locale={language.current}
        />
        <Tabs.Root bind:value={observationTab} class="observation-pane">
          <div class="pane-header">
            <Tabs.List class="panel-tabs" aria-label={m.observation_views({}, options)}
              ><Tabs.Trigger value="memory"
                ><MemoryStick size={15} aria-hidden="true" />{m.memory({}, options)}</Tabs.Trigger
              ><Tabs.Trigger value="instructions"
                ><ListTree size={15} aria-hidden="true" />{m.instructions(
                  {},
                  options,
                )}</Tabs.Trigger
              ><Tabs.Trigger value="raw"
                ><Binary size={15} aria-hidden="true" />{m.raw_code({}, options)}</Tabs.Trigger
              ><Tabs.Trigger value="artifact"
                ><Box size={15} aria-hidden="true" />{m.artifact({}, options)}</Tabs.Trigger
              ></Tabs.List
            >
          </div>
          <Tabs.Content value="memory" class="memory-content">
            <form
              class="memory-controls"
              onsubmit={(event) => {
                event.preventDefault();
                void work.inspect();
              }}
            >
              <label
                ><span>{m.address({}, options)}</span><input
                  bind:value={work.memoryAddress}
                  spellcheck="false"
                /></label
              >
              <label
                ><span>{m.window_size({}, options)}</span><input
                  type="number"
                  required
                  aria-label={m.window_size({}, options)}
                  min="1"
                  max="4096"
                  bind:value={work.memoryLength}
                /><span class="input-unit">B</span></label
              >
              <button
                type="button"
                disabled={!work.connected || pc === undefined}
                onclick={(event) => {
                  if (pc !== undefined) {
                    work.memoryAddress = pc;
                    event.currentTarget.form?.requestSubmit();
                  }
                }}><LocateFixed size={14} aria-hidden="true" />{m.inspect_pc({}, options)}</button
              >
              <button disabled={!work.connected || observation === undefined}
                >{m.inspect_memory({}, options)}<ArrowRight size={14} aria-hidden="true" /></button
              >
              <Patch
                disabled={!work.connected || !resumable || work.controlling}
                locale={language.current}
                onwrite={(address: string, bytes: string) => {
                  void work.writeMemory(address, bytes);
                }}
              />
            </form>
            <Memory
              window={captured?.observation.memory ?? null}
              bytes={captured?.memory ?? null}
              locale={language.current}
            />
          </Tabs.Content>
          <Tabs.Content value="instructions" class="instructions-content">
            <Instructions
              bytes={selectedBytes?.bytes}
              target={selectedBytes?.target ?? work.target}
              base={selectedBytes?.base ?? work.base}
              connected={work.connected}
              locale={language.current}
              decode={work.decode}
              analyze={work.analyze}
              live={selectedBytes?.id === 'live'}
              pc={selectedBytes?.id === 'live' ? capturedPC : undefined}
              breakpoints={observation?.breakpoints ?? []}
              editable={selectedBytes?.id === 'live' &&
                work.connected &&
                resumable &&
                !work.controlling}
              onbreakpoint={(address: string, enabled: boolean) => {
                void work.breakpoint(address, enabled);
              }}
              onsetpc={(address: string) => {
                if (selectedBytes?.id === 'live')
                  void work.writeRegister(
                    selectedBytes.target === 'x86_64' ? 'rip' : 'pc',
                    address,
                  );
              }}
            >
              {#snippet source()}
                {#if byteSources.length > 0}<label class="byte-source"
                    >{m.byte_source({}, options)}
                    <select
                      value={selectedBytes?.id}
                      onchange={(event) => {
                        byteSource = event.currentTarget.value;
                      }}
                    >
                      {#each byteSources as item (item.id)}<option value={item.id}
                          >{item.label}</option
                        >{/each}
                    </select>
                    {#if selectedBytes?.id.startsWith('segment-') && !work.artifactCurrent}<span
                        class="stale">{m.artifact_stale({}, options)}</span
                      >{/if}
                  </label>{/if}
              {/snippet}
            </Instructions>
          </Tabs.Content>
          <Tabs.Content value="raw" class="raw-content">
            <Raw
              bind:value={work.raw}
              bind:setup={work.rawSetup}
              bind:budget={work.budget}
              bytes={work.binary?.length}
              disabled={!work.connected || work.controlling || running}
              locale={language.current}
              onload={() => {
                void work.loadRaw();
              }}
            />
          </Tabs.Content>
          <Tabs.Content value="artifact" class="artifact-content">
            {#if work.candidate === null}<div class="empty-observation">
                <Box size={22} strokeWidth={1.5} aria-hidden="true" />
                <div>
                  <strong>{m.no_artifact({}, options)}</strong>
                  <p>{m.workflow_build({}, options)}</p>
                </div>
              </div>
            {:else}<div class="artifact-description">
                <CircleCheck size={18} aria-hidden="true" /><strong
                  >{m.built_revision(
                    { revision: work.candidate.artifact.identity.revision },
                    options,
                  )}</strong
                ><span>{work.candidate.artifact.identity.target} · ELF</span
                >{#if !work.artifactCurrent}<span class="stale"
                    >{m.artifact_stale({}, options)}</span
                  >{/if}
              </div>
              <dl class="artifact-facts">
                <div>
                  <dt>{m.entry_address({}, options)}</dt>
                  <dd>{work.candidate.artifact.image.entry}</dd>
                </div>
                <div>
                  <dt>{m.object_size({}, options)}</dt>
                  <dd>{work.candidate.artifact.object_bytes} B</dd>
                </div>
                <div>
                  <dt>{m.image_size({}, options)}</dt>
                  <dd>{work.candidate.artifact.image_bytes} B</dd>
                </div>
                <div>
                  <dt>{m.segments({}, options)}</dt>
                  <dd>{work.candidate.artifact.image.segments.length}</dd>
                </div>
              </dl>
            {/if}
          </Tabs.Content>
        </Tabs.Root>
      </div>
      {#if work.problem !== null}<div class="error-banner" role="alert">
          <span
            >{problemLabel(work.problem, language.current)}
            {work.unknown ? m.unknown_outcome({}, options) : ''}</span
          >{#if location !== null}<button
              onclick={() => {
                editor?.revealDiagnostic();
              }}
              ><LocateFixed size={14} aria-hidden="true" />{m.show_diagnostic(
                { line: String(location.line), column: String(location.column) },
                options,
              )}</button
            >{/if}{#if !work.connected && !work.preview}<button
              onclick={() => {
                void work.connect(true);
              }}
              disabled={work.connecting}
              ><RefreshCw size={14} aria-hidden="true" />{m.reconnect({}, options)}</button
            >{/if}
        </div>{/if}
      {#if fileProblem !== null}<p class="error-banner" role="alert">
          {problemLabel(fileProblem, language.current)}
        </p>{/if}
      {#if language.failed || preferencesFailed}<p class="error-banner" role="alert">
          {language.failed ? m.locale_failed({}, options) : m.preferences_failed({}, options)}
        </p>{/if}
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
        ><span>{m.line_count({ count: String(sourceDetails.lines) }, options)}</span><span
          >UTF-8{work.source.startsWith('\uFEFF') ? ' BOM' : ''} · {sourceDetails.ending === 'mixed'
            ? m.mixed_newlines({}, options)
            : sourceDetails.ending}</span
        >
      </footer>
    </main>
  </Tooltip.Provider>
{/if}

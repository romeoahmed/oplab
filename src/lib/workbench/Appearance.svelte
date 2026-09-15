<script lang="ts">
  import * as m from '$lib/paraglide/messages.js';
  import type { Locale } from '$lib/paraglide/runtime';
  import { RotateCcw, SlidersHorizontal, X } from '@lucide/svelte';
  import { Popover } from 'bits-ui';

  import { defaultPreferences, type Preferences } from './preferences';
  let {
    preferences = $bindable({ ...defaultPreferences }),
    locale,
  }: { preferences?: Preferences; locale: Locale } = $props();
  const options = $derived({ locale });
</script>

<Popover.Root>
  <Popover.Trigger class="icon-button" aria-label={m.appearance({}, options)}
    ><SlidersHorizontal size={18} aria-hidden="true" /></Popover.Trigger
  >
  <Popover.Portal>
    <Popover.Content
      class="settings-popover"
      sideOffset={10}
      align="end"
      aria-label={m.appearance({}, options)}
    >
      <div class="popover-header">
        <h2>{m.appearance({}, options)}</h2>
        <Popover.Close class="icon-button" aria-label={m.close({}, options)}
          ><X size={16} aria-hidden="true" /></Popover.Close
        >
      </div>
      <div class="settings-fields">
        <label
          >{m.editor_font({}, options)}<select bind:value={preferences.font}
            ><option value="jetbrains">JetBrains Mono</option><option value="system"
              >{m.system_mono({}, options)}</option
            ></select
          ></label
        >
        <label
          >{m.font_size({}, options)}<span class="range-field"
            ><input type="range" min="12" max="22" bind:value={preferences.fontSize} /><output
              >{preferences.fontSize} px</output
            ></span
          ></label
        >
        <label class="checkbox-field"
          ><input type="checkbox" bind:checked={preferences.wrap} />{m.word_wrap(
            {},
            options,
          )}</label
        >
        <hr />
        <label
          >{m.inspector_width({}, options)}<span class="range-field"
            ><input type="range" min="26" max="45" bind:value={preferences.inspectorWidth} /><output
              >{preferences.inspectorWidth}%</output
            ></span
          ></label
        >
        <label
          >{m.memory_height({}, options)}<span class="range-field"
            ><input type="range" min="20" max="45" bind:value={preferences.memoryHeight} /><output
              >{preferences.memoryHeight}%</output
            ></span
          ></label
        >
        <button
          type="button"
          disabled={preferences.inspectorWidth === defaultPreferences.inspectorWidth &&
            preferences.memoryHeight === defaultPreferences.memoryHeight}
          onclick={() => {
            preferences.inspectorWidth = defaultPreferences.inspectorWidth;
            preferences.memoryHeight = defaultPreferences.memoryHeight;
          }}><RotateCcw size={14} aria-hidden="true" />{m.reset_layout({}, options)}</button
        >
      </div>
      <p class="muted-note">{m.appearance_hint({}, options)}</p>
    </Popover.Content>
  </Popover.Portal>
</Popover.Root>

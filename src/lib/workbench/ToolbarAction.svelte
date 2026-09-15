<script lang="ts">
  import { Toolbar, Tooltip } from 'bits-ui';
  import type { Snippet } from 'svelte';
  const {
    label,
    hint,
    shortcut,
    disabled = false,
    primary = false,
    onclick,
    children,
  }: {
    label: string;
    hint: string;
    shortcut?: string | undefined;
    disabled?: boolean;
    primary?: boolean | undefined;
    onclick: () => void;
    children: Snippet;
  } = $props();
</script>

<Tooltip.Root>
  <Tooltip.Trigger {disabled} {onclick}>
    {#snippet child({ props })}
      <Toolbar.Button
        {...props}
        class={primary ? 'action primary' : 'action'}
        {disabled}
        aria-label={label}
        aria-keyshortcuts={shortcut}
      >
        {@render children()}<span>{label}</span>
      </Toolbar.Button>
    {/snippet}
  </Tooltip.Trigger>
  <Tooltip.Portal>
    <Tooltip.Content class="tooltip" side="bottom" sideOffset={8}>
      {hint}{#if shortcut}<kbd
          >{shortcut === 'Meta+Enter Control+Enter' ? 'Ctrl / ⌘ + Enter' : shortcut}</kbd
        >{/if}
    </Tooltip.Content>
  </Tooltip.Portal>
</Tooltip.Root>

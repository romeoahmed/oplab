<script lang="ts">
  import { onDestroy } from 'svelte';
  import { DropdownMenu } from 'bits-ui';
  import { Binary, Box, ChevronDown, FileDown, FileUp, FolderOpen } from '@lucide/svelte';
  import type { FilePort } from '$lib/desktop/files';
  import type { FileFormat } from '$lib/protocol/generated/FileFormat';
  import type { Locale } from '$lib/paraglide/runtime';
  import * as m from '$lib/paraglide/messages.js';

  const {
    port,
    locale,
    source,
    object,
    image,
    binary,
    onsource,
    onbinary,
    onerror,
  }: {
    port: FilePort | null;
    locale: Locale;
    source: string;
    object: Uint8Array | undefined;
    image: Uint8Array | undefined;
    binary: Uint8Array | undefined;
    onsource: (source: string) => void;
    onbinary: (bytes: Uint8Array) => void;
    onerror: (code: string | null) => void;
  } = $props();
  let active = true;
  onDestroy(() => {
    active = false;
  });
  let busy = $state(false);
  const options = $derived({ locale });

  async function transfer(
    fallback: 'file_read' | 'file_write',
    operation: (files: FilePort) => Promise<void>,
  ) {
    if (busy || port === null) return;
    busy = true;
    onerror(null);
    try {
      await operation(port);
    } catch (error) {
      if (active) onerror(typeof error === 'string' ? error : fallback);
    } finally {
      busy = false;
    }
  }
  function open(format: 'source' | 'binary', title: string) {
    // If the source changes while a picker is open, do not overwrite those edits.
    const previous = source;
    void transfer('file_read', async (files) => {
      const bytes = await files.open(format, title);
      if (!active || bytes === null) return;
      if (format === 'binary') onbinary(bytes);
      else if (source !== previous) onerror('file_conflict');
      else {
        let text: string;
        try {
          text = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(bytes);
        } catch {
          onerror('file_encoding');
          return;
        }
        onsource(text);
      }
    });
  }
  function save(format: FileFormat, title: string, bytes: Uint8Array) {
    void transfer('file_write', async (files) => {
      await files.save(format, title, bytes);
    });
  }
</script>

<DropdownMenu.Root>
  <DropdownMenu.Trigger class="file-trigger" disabled={port === null || busy}>
    <FolderOpen size={16} aria-hidden="true" />{m.files({}, options)}
    <ChevronDown size={12} aria-hidden="true" />
  </DropdownMenu.Trigger>
  <DropdownMenu.Portal>
    <DropdownMenu.Content class="file-menu" align="start" sideOffset={6}>
      <DropdownMenu.Item
        onSelect={() => {
          open('source', m.import_source({}, options));
        }}
      >
        <FileUp size={16} aria-hidden="true" />{m.import_source({}, options)}
      </DropdownMenu.Item>
      <DropdownMenu.Item
        onSelect={() => {
          save('source', m.export_source({}, options), new TextEncoder().encode(source));
        }}
      >
        <FileDown size={16} aria-hidden="true" />{m.export_source({}, options)}
      </DropdownMenu.Item>
      <DropdownMenu.Separator />
      <DropdownMenu.Item
        onSelect={() => {
          open('binary', m.import_binary({}, options));
        }}
      >
        <Binary size={16} aria-hidden="true" />{m.import_binary({}, options)}
      </DropdownMenu.Item>
      <DropdownMenu.Item
        disabled={binary === undefined}
        onSelect={() => {
          if (binary !== undefined) save('binary', m.export_binary({}, options), binary);
        }}
      >
        <FileDown size={16} aria-hidden="true" />{m.export_binary({}, options)}
      </DropdownMenu.Item>
      <DropdownMenu.Separator />
      <DropdownMenu.Item
        disabled={object === undefined}
        onSelect={() => {
          if (object !== undefined) save('object', m.export_object({}, options), object);
        }}
      >
        <Box size={16} aria-hidden="true" />{m.export_object({}, options)}
      </DropdownMenu.Item>
      <DropdownMenu.Item
        disabled={image === undefined}
        onSelect={() => {
          if (image !== undefined) save('image', m.export_image({}, options), image);
        }}
      >
        <FileDown size={16} aria-hidden="true" />{m.export_image({}, options)}
      </DropdownMenu.Item>
    </DropdownMenu.Content>
  </DropdownMenu.Portal>
</DropdownMenu.Root>

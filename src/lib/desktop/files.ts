import { invoke, isTauri } from '@tauri-apps/api/core';
import type { FileFormat } from '$lib/protocol/generated/FileFormat';

/** Native dialogs retain paths; callers receive only bounded contents or cancellation. */
export type FilePort = {
  open: (format: 'source' | 'binary', title: string) => Promise<Uint8Array | null>;
  save: (format: FileFormat, title: string, bytes: Uint8Array) => Promise<boolean>;
};

export function desktopFiles(): FilePort | null {
  if (!isTauri()) return null;
  return {
    async open(format, title) {
      const bytes = await invoke<number[] | null>('import_file', { format, title });
      return bytes === null ? null : new Uint8Array(bytes);
    },
    save(format, title, bytes) {
      return invoke<boolean>('export_file', { format, title, bytes: Array.from(bytes) });
    },
  };
}

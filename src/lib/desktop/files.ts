import type { FileFormat } from '$lib/protocol/generated/FileFormat';
import { invoke, isTauri } from '@tauri-apps/api/core';

/** Native file I/O without exposing selected paths to the frontend. */
export type FilePort = {
  /** Return bounded file contents, or `null` if the user cancels. I/O failures reject. */
  open: (format: 'source' | 'binary', title: string) => Promise<Uint8Array | null>;
  /** Return whether the file was saved; cancellation returns `false`, I/O failures reject. */
  save: (format: FileFormat, title: string, bytes: Uint8Array) => Promise<boolean>;
};

/** Create the native file adapter, or return `null` in browser preview. */
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

import { isTauri } from '@tauri-apps/api/core';

/** Keep the system title bar in sync; browser tabs use Svelte's document head. */
export async function setWindowTitle(title: string, signal: AbortSignal): Promise<void> {
  if (!isTauri()) return;
  const { getCurrentWindow } = await import('@tauri-apps/api/window');
  if (!signal.aborted) await getCurrentWindow().setTitle(title);
}

import { invoke } from '@tauri-apps/api/core';

export const isTauri = () =>
  typeof window !== 'undefined' &&
  '__TAURI_INTERNALS__' in (window as unknown as Record<string, unknown>);

export async function invokeOr<T>(
  command: string,
  fallback: T,
  args?: Record<string, unknown>,
): Promise<T> {
  if (!isTauri()) return fallback;
  return invoke<T>(command, args);
}

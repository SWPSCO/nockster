import { invoke } from '@tauri-apps/api/core';

// The native commands serialize file access and replace the ciphertext file atomically.
export const desktopStorage = {
  get(keys: string[]): Promise<Record<string, unknown>> {
    return invoke('desktop_storage_get', { keys });
  },
  set(items: Record<string, unknown>): Promise<void> {
    return invoke('desktop_storage_set', { items });
  },
  remove(keys: string[]): Promise<void> {
    return invoke('desktop_storage_remove', { keys });
  }
};

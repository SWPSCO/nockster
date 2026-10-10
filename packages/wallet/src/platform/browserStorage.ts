/** Keep this app's browser data separate from other apps on the ship's origin. */
export function scopedStorage(storage: Storage, prefix: string): Storage {
  const keys = () =>
    Array.from({ length: storage.length }, (_, index) => storage.key(index)!).filter(key =>
      key.startsWith(prefix)
    );
  return {
    get length() {
      return keys().length;
    },
    key(index: number) {
      return keys()[index]?.slice(prefix.length) ?? null;
    },
    getItem(key: string) {
      return storage.getItem(prefix + key);
    },
    setItem(key: string, value: string) {
      storage.setItem(prefix + key, value);
    },
    removeItem(key: string) {
      storage.removeItem(prefix + key);
    },
    clear() {
      keys().forEach(key => storage.removeItem(key));
    }
  };
}

export const browserStorage =
  typeof localStorage === 'undefined'
    ? undefined!
    : import.meta.env.MODE === 'urbit'
      ? scopedStorage(localStorage, 'nockster:')
      : localStorage;

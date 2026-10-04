/**
 * Cache utility for managing data freshness and TTL (Time To Live)
 */

export interface CacheMetadata {
  timestamp: number;
  expiresAt: number;
  version?: number;
}

export interface CachedData<T> {
  data: T;
  metadata: CacheMetadata;
}

/**
 * Cache version - increment this to invalidate all cached data
 * when the data structure changes (e.g., adding new fields)
 */
export const CACHE_VERSION = 4; // v4: Fix API bug where 'self' type is incorrect when counterparty differs

/**
 * TTL (Time To Live) constants in milliseconds
 */
export const CACHE_TTL = {
  BALANCE: 30 * 1000, // 30 seconds
  NOTES: 60 * 1000, // 1 minute
  TRANSACTIONS: 5 * 60 * 1000, // 5 minutes
  PRICE: 60 * 1000 // 1 minute
} as const;

/**
 * Check if cached data is still valid based on TTL
 */
export function isCacheValid(timestamp: number, ttl: number): boolean {
  const now = Date.now();
  const age = now - timestamp;
  return age < ttl;
}

/**
 * Check if cached data with expiration is still valid
 * Also checks cache version - if version doesn't match, cache is invalid
 */
export function isCachedDataValid<T>(cached: CachedData<T> | null | undefined): boolean {
  if (!cached || !cached.metadata) return false;
  // Check if cache version matches - invalidate old cache versions
  if (cached.metadata.version !== CACHE_VERSION) return false;
  return Date.now() < cached.metadata.expiresAt;
}

/**
 * Create cache metadata for data
 */
export function createCacheMetadata(ttl: number): CacheMetadata {
  const now = Date.now();
  return {
    timestamp: now,
    expiresAt: now + ttl,
    version: CACHE_VERSION
  };
}

/**
 * Wrap data with cache metadata
 */
export function createCachedData<T>(data: T, ttl: number): CachedData<T> {
  return {
    data,
    metadata: createCacheMetadata(ttl)
  };
}

/**
 * Get age of cached data in milliseconds
 */
export function getCacheAge(timestamp: number): number {
  return Date.now() - timestamp;
}

/**
 * Get time until cache expires in milliseconds
 */
export function getTimeUntilExpiry(expiresAt: number): number {
  return Math.max(0, expiresAt - Date.now());
}

/**
 * Invalidate cache by returning null
 */
export function invalidateCache(): null {
  return null;
}

/**
 * Merge two arrays by unique ID, preferring newer items
 */
export function mergeByUniqueId<T extends Record<string, unknown>>(
  existing: T[],
  incoming: T[],
  idField: keyof T = 'id' as keyof T
): T[] {
  const merged = new Map<string, T>();

  // Add existing items
  existing.forEach(item => {
    const id = item[idField];
    if (typeof id === 'string') {
      merged.set(id, item);
    }
  });

  // Add or update with incoming items (prefer newer)
  incoming.forEach(item => {
    const id = item[idField];
    if (typeof id !== 'string') return;

    const existingItem = merged.get(id);
    const itemTimestamp = (item as Record<string, unknown>).timestamp as number | undefined;
    const existingTimestamp = existingItem
      ? ((existingItem as Record<string, unknown>).timestamp as number | undefined)
      : undefined;

    if (!existingItem) {
      merged.set(id, item);
    } else if (itemTimestamp && existingTimestamp) {
      // Prefer newer timestamp, or incoming if equal (fresher from API with potential fixes)
      if (itemTimestamp >= existingTimestamp) {
        merged.set(id, item);
      }
    } else {
      // If no timestamp, prefer incoming (fresher from API)
      merged.set(id, item);
    }
  });

  return Array.from(merged.values());
}

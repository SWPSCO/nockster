// Nicks conversion utilities
import { NICKS_PER_NOCK, type Nicks } from '../types/nicks.ts';

/**
 * Convert nicks to NOCK for display
 * @param nicks - Amount in nicks (bigint)
 * @returns Amount in NOCK as a number
 */
export function nicksToNocks(nicks: Nicks): number {
  return Number(nicks) / Number(NICKS_PER_NOCK);
}

/**
 * Convert NOCK to nicks for internal storage
 * @param nocks - Amount in NOCK
 * @returns Amount in nicks (bigint)
 */
export function nocksToNicks(nocks: number): Nicks {
  const value = Math.round(nocks * Number(NICKS_PER_NOCK));
  return toNicks(value);
}

/**
 * Format nicks as NOCK string for UI display
 * @param nicks - Amount in nicks
 * @param decimals - Number of decimal places (default: 8)
 * @returns Formatted NOCK string
 */
export function formatNocks(nicks: Nicks, decimals: number = 8): string {
  const nocks = nicksToNocks(nicks);
  return nocks.toFixed(decimals);
}

/**
 * Format a NOCK amount (number) with thousands separators.
 * @param nocks - Amount in NOCK
 * @param decimals - Number of decimal places (default: 8)
 * @returns Formatted NOCK string with separators
 */
export function formatNocksNumberWithSeparator(nocks: number, decimals: number = 8): string {
  if (!Number.isFinite(nocks)) {
    return new Intl.NumberFormat('en-US', {
      minimumFractionDigits: decimals,
      maximumFractionDigits: decimals
    }).format(0);
  }
  return new Intl.NumberFormat('en-US', {
    minimumFractionDigits: decimals,
    maximumFractionDigits: decimals
  }).format(nocks);
}

/**
 * Format nicks as NOCK string with thousands separators
 * @param nicks - Amount in nicks
 * @param decimals - Number of decimal places (default: 8)
 * @returns Formatted NOCK string with separators
 */
export function formatNocksWithSeparator(nicks: Nicks, decimals: number = 8): string {
  const nocks = nicksToNocks(nicks);
  return formatNocksNumberWithSeparator(nocks, decimals);
}

/**
 * Format a nicks (bigint) value with thousands separators.
 * @param nicks - Amount in nicks (bigint)
 * @returns Formatted string with separators
 */
export function formatNicksWithSeparator(nicks: Nicks): string {
  const raw = nicks.toString();
  return raw.replace(/\B(?=(\d{3})+(?!\d))/g, ',');
}

/**
 * Parse user input (in NOCK) to nicks
 * @param input - User input string (NOCK amount)
 * @returns Amount in nicks, or null if invalid
 */
export function parseNocksInput(input: string): Nicks | null {
  const cleaned = input.trim();
  if (!/^(?:\d+|\d{1,3}(?:,\d{3})+)(?:\.\d{1,16})?$/.test(cleaned)) return null;
  const [whole, fraction = ''] = cleaned.replace(/,/g, '').split('.');
  const scale = 10n ** BigInt(fraction.length);
  const numerator = (BigInt(whole) * scale + BigInt(fraction || '0')) * NICKS_PER_NOCK;
  // Round at the nick boundary using integer arithmetic.
  const nicks = (numerator + scale / 2n) / scale;
  return nicks <= BigInt(Number.MAX_SAFE_INTEGER) ? nicks : null;
}

/**
 * Convert a number (representing nicks as a regular number) to Nicks type
 * This is useful for compatibility with existing code that uses numbers
 * @param value - Numeric value representing nicks
 * @returns Nicks (bigint)
 */
export function toNicks(value: number): Nicks {
  if (!Number.isSafeInteger(value) || value < 0) throw new Error('Invalid nick amount');
  return BigInt(value);
}

/**
 * Convert Nicks to a regular number
 * Use with caution for large values due to precision limits
 * @param nicks - Amount in nicks
 * @returns Numeric value
 */
export function fromNicks(nicks: Nicks): number {
  if (nicks < 0n || nicks > BigInt(Number.MAX_SAFE_INTEGER))
    throw new Error('Nick amount exceeds supported range');
  return Number(nicks);
}

/**
 * Format fee for display - shows in nicks for small amounts, nocks for large amounts
 * @param nicks - Amount in nicks
 * @returns Formatted string showing "X nick", "X nicks", or "X.XXXX NOCK"
 */
export function formatFeeInNicks(nicks: Nicks): string {
  const count = Number(nicks);
  const NOCKS_THRESHOLD = 2 ** 16; // 65536 nicks

  if (count >= NOCKS_THRESHOLD) {
    // Format in NOCK for large fees
    const nockAmount = nicksToNocks(nicks);
    // Use up to 8 decimal places, but trim trailing zeros
    const formatted = nockAmount.toFixed(8).replace(/\.?0+$/, '');
    return `${formatted} NOCK`;
  }

  return count === 1 ? '1 nick' : `${count.toLocaleString()} nicks`;
}

export function parseNicksInput(input: string): Nicks | null {
  const cleaned = input.trim();
  if (!/^(?:\d+|\d{1,3}(?:,\d{3})+)$/.test(cleaned)) return null;
  const value = BigInt(cleaned.replace(/,/g, ''));
  return value <= BigInt(Number.MAX_SAFE_INTEGER) ? value : null;
}

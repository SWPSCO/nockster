const alphabet = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';
const addressSpace = 0xffffffff00000001n ** 5n;
const maxDigits: number[] = [];
for (let value = addressSpace - 1n; value > 0n; value /= 58n) {
  maxDigits.unshift(Number(value % 58n));
}

// MatchMode::Insensitive in the vanity crate keeps i and l distinct;
// the digit 1 accepts both groups.
const equivalents: Record<string, string> = {
  '4': 'a',
  '8': 'b',
  '3': 'e',
  '0': 'o',
  '5': 's',
  '7': 't',
  '2': 'z'
};
function normalize(character: string) {
  return equivalents[character] ?? character.toLowerCase();
}

/** Expected trials for a uniform PKH, including canonical Base58's leading-digit bias. */
export function expectedVanityAttempts(prefix: string, insensitive = false): number | null {
  prefix = prefix.trim();
  if (!prefix || prefix.length > maxDigits.length) return null;
  const allowed = [...prefix].map(character =>
    [...alphabet]
      .map((candidate, digit) => {
        const a = normalize(character);
        const b = normalize(candidate);
        const matches = insensitive
          ? a === b ||
            (a === '1' && (b === 'i' || b === 'l')) ||
            (b === '1' && (a === 'i' || a === 'l'))
          : character === candidate;
        return matches ? digit : -1;
      })
      .filter(digit => digit >= 0)
  );
  if (allowed.some(digits => !digits.length)) return null;

  // Count matching numerals of each possible length, bounded by p^5 - 1.
  // `equal` follows the upper bound; `less` counts already smaller numerals.
  let matches = 0n;
  const allDigits = Array.from({ length: 58 }, (_, digit) => digit);
  for (let length = prefix.length; length <= maxDigits.length; length++) {
    let equal = 1n;
    let less = 0n;
    for (let position = 0; position < length; position++) {
      const limit = length === maxDigits.length ? maxDigits[position] : 57;
      const digits = (allowed[position] ?? allDigits).filter(
        digit => position !== 0 || length === 1 || digit !== 0
      );
      less =
        less * BigInt(digits.length) + equal * BigInt(digits.filter(digit => digit < limit).length);
      equal = digits.includes(limit) ? equal : 0n;
    }
    matches += less + equal;
  }
  return matches > 0n ? Number(addressSpace) / Number(matches) : null;
}

export function formatVanityDuration(seconds: number): string | null {
  if (!Number.isFinite(seconds) || seconds < 0) return null;
  if (seconds < 1) return 'less than a second';
  const units: [number, string][] = [
    [365.25 * 86400, 'year'],
    [86400, 'day'],
    [3600, 'hour'],
    [60, 'minute'],
    [1, 'second']
  ];
  const [size, unit] = units.find(([size]) => seconds >= size)!;
  const value = Number((seconds / size).toPrecision(2));
  const count = value.toLocaleString(undefined, {
    maximumFractionDigits: 1,
    notation: value >= 1e12 ? 'scientific' : value >= 1e6 ? 'compact' : 'standard'
  });
  return `about ${count} ${unit}${value === 1 ? '' : 's'}`;
}

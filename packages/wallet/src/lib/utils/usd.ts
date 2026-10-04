export function formatUsdEstimate(nocks: number, price: number | null | undefined): string | null {
  if (!Number.isFinite(nocks) || nocks < 0 || !price || !Number.isFinite(price) || price <= 0)
    return null;
  const value = nocks * price;
  if (!Number.isFinite(value)) return null;
  if (value > 0 && value < 0.001) return '<0.001 USD';
  const decimals = value >= 1 ? 2 : 3;
  return `~${value.toLocaleString('en-US', { minimumFractionDigits: decimals, maximumFractionDigits: decimals })} USD`;
}

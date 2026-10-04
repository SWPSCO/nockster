export function nextWalletName(names: string[]): string {
  const existing = new Set(names);
  let number = names.length + 1;
  let name = number === 1 ? 'My Wallet' : `My Wallet ${number}`;
  while (existing.has(name)) name = `My Wallet ${++number}`;
  return name;
}

import type { PendingTransaction, Wallet } from '../types/wallet';

// Split persisted device-wide accounts into wallets identified by their address.
// Keep the original record and its activity; additional addresses get separate accounts.
export function migrateHardwareWallets(wallets: Wallet[]): Wallet[] {
  const result: Wallet[] = [];
  const knownAddresses = new Set(
    wallets.filter(w => w.hardware && w.addresses.length === 1).map(w => w.addresses[0])
  );
  for (const wallet of wallets) {
    if (!wallet.hardware || wallet.addresses.length <= 1) {
      result.push(wallet);
      continue;
    }
    const addresses = [...new Set(wallet.addresses)];
    const primary = wallet.addresses[wallet.currentAddressIndex] ?? addresses[0];
    result.push({ ...wallet, addresses: [primary], currentAddressIndex: 0 });
    knownAddresses.add(primary);
    for (const address of addresses) {
      if (address === primary || knownAddresses.has(address)) continue;
      knownAddresses.add(address);
      result.push({
        id: `hardware-${address}`,
        name: `${wallet.name} · ${addresses.indexOf(address) + 1}`,
        addresses: [address],
        currentAddressIndex: 0,
        createdAt: wallet.createdAt,
        lastUsed: wallet.lastUsed,
        watchOnly: true,
        hardware: { ...wallet.hardware }
      });
    }
  }
  const byAddress = new Map(result.filter(w => w.hardware).map(w => [w.addresses[0], w.id]));
  const pending = new Map<string, Map<string, PendingTransaction>>();
  for (const wallet of wallets) {
    for (const tx of wallet.pendingTransactions ?? []) {
      const target = wallet.hardware ? (byAddress.get(tx.fromAddress) ?? wallet.id) : wallet.id;
      const entries = pending.get(target) ?? new Map<string, PendingTransaction>();
      entries.set(tx.txId, tx);
      pending.set(target, entries);
    }
  }
  return result.map(wallet =>
    pending.has(wallet.id) || wallet.pendingTransactions
      ? { ...wallet, pendingTransactions: [...(pending.get(wallet.id)?.values() ?? [])] }
      : wallet
  );
}

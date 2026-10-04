import { mount } from 'svelte';
import ImportWallet from '../../packages/wallet/src/lib/components/organisms/ImportWallet.svelte';
import RenameWalletModal from '../../packages/wallet/src/lib/components/molecules/RenameWalletModal.svelte';
import WalletListItem from '../../packages/wallet/src/lib/components/molecules/WalletListItem.svelte';
import { walletStore } from '../../packages/wallet/src/lib/stores/wallet';
import type { Wallet } from '../../packages/wallet/src/lib/types/wallet';

export async function render(screen: 'import' | 'rename' | 'wallet') {
  await walletStore.loadFromStorage();
  const wallets: Wallet[] = ['Savings', 'Spending'].map((name, index) => ({
    id: `test-${index}`,
    name,
    addresses: [`test-address-${index}`],
    currentAddressIndex: 0,
    createdAt: 1,
    lastUsed: 1,
    balance: 0
  }));
  walletStore.setWallets(wallets);
  const target = document.createElement('div');
  document.body.replaceChildren(target);
  if (screen === 'import') mount(ImportWallet, { target });
  if (screen === 'rename') mount(RenameWalletModal, { target, props: { currentName: 'Savings' } });
  if (screen === 'wallet') mount(WalletListItem, { target, props: { wallet: wallets[1] } });
}

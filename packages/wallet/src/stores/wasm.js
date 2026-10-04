export async function createWallet() {
  const res = await chrome.runtime.sendMessage({ type: 'createWallet' });
  if (!res.success) {
    throw new Error(res.error);
  }
  return res.wallet;
}

export async function createWalletFromSeedPhrase(seedPhrase) {
  const res = await chrome.runtime.sendMessage({ type: 'createWalletFromSeedPhrase', seedPhrase });
  if (!res.success) {
    throw new Error(res.error);
  }
  return res.wallet;
}

export async function createTransaction(notes) {
  const res = await chrome.runtime.sendMessage({ type: 'createTransaction', notes });
  if (!res.success) {
    throw new Error(res.error);
  }
  return res.transaction;
}

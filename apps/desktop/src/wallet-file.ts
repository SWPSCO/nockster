import { hardwareCrypto } from './hardware/crypto';
import { validateWalletKey } from '../../../packages/wallet/src/vaultController';

export async function readWalletFile(file: File): Promise<{ phrase: string; address: string }[]> {
  if (file.size === 0 || file.size > 1024 * 1024)
    throw new Error('Choose a nonempty Nockchain wallet file smaller than 1 MB.');
  const bytes = new Uint8Array(await file.arrayBuffer());
  try {
    const crypto = await hardwareCrypto();
    let summary: { seedphrases?: unknown };
    try {
      summary = crypto.parse_wallet_keyfile(bytes);
    } catch {
      throw new Error('Unable to read this Nockchain wallet file. Choose a keys.export file.');
    }
    if (!Array.isArray(summary.seedphrases) || !summary.seedphrases.length)
      throw new Error(
        'This wallet file has no seed phrase. Import its original 24-word seed phrase or zprv instead.'
      );
    const wallets: { phrase: string; address: string }[] = [];
    for (const value of summary.seedphrases) {
      if (typeof value !== 'string')
        throw new Error('The wallet file contains an invalid seed phrase.');
      const phrase = value.trim().split(/\s+/).join(' ');
      if (phrase.split(' ').length !== 24)
        throw new Error('The wallet file must contain a 24-word seed phrase.');
      const address = await validateWalletKey(phrase);
      if (!wallets.some(wallet => wallet.address === address)) wallets.push({ phrase, address });
    }
    return wallets;
  } finally {
    bytes.fill(0);
  }
}

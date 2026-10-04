import { composeUnsignedTx, inspectTxJam, verifySignedDraft } from '../../pkg/nockster_core.js';
import { ensureVaultReady, cheetahPkhB58 } from '../../vault/wasm';
import { withSigningDevice } from './hardwareDevice.ts';
import { getRPCClientV1, type NoteV1 } from './rpc';

type HardwarePreview = {
  feePaid: number;
  inputNotes: string[];
};

type HardwareSigned = HardwarePreview & {
  signedTx: string;
  txId: string;
};

async function compose(
  walletAddress: string,
  notes: NoteV1[],
  recipients: Array<{ address: string; amount: number; bridgeEvmAddress?: string }>
) {
  await ensureVaultReady();
  const height = await getRPCClientV1().getTipHeight();
  return composeUnsignedTx(
    walletAddress,
    notes,
    recipients.map(r => ({
      address: r.address,
      gift: r.amount,
      bridgeEvmAddress: r.bridgeEvmAddress
    })),
    height
  ) as { base64Tx: string; feePaid: number; inputNotes: string[] };
}

function base64ToBytes(value: string): Uint8Array {
  return Uint8Array.from(atob(value), c => c.charCodeAt(0));
}

function bytesToBase64(bytes: Uint8Array): string {
  let binary = '';
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  return btoa(binary);
}

export async function previewHardwareTransaction(
  walletAddress: string,
  notes: NoteV1[],
  recipients: Array<{ address: string; amount: number }>
): Promise<HardwarePreview> {
  return compose(walletAddress, notes, recipients);
}

export async function signHardwareTransaction(
  walletAddress: string,
  notes: NoteV1[],
  recipients: Array<{ address: string; amount: number; bridgeEvmAddress?: string }>
): Promise<HardwareSigned> {
  const composed = await compose(walletAddress, notes, recipients);

  return withSigningDevice(walletAddress, cheetahPkhB58, async device => {
    const signedBytes = await device.signDraft(base64ToBytes(composed.base64Tx));
    verifySignedDraft(composed.base64Tx, bytesToBase64(signedBytes));
    const info = inspectTxJam(bytesToBase64(signedBytes));
    if (
      !info.spends.length ||
      info.spends.some((spend: { isFullySigned: boolean }) => !spend.isFullySigned)
    ) {
      throw new Error('Device returned an incomplete transaction');
    }

    return {
      signedTx: bytesToBase64(signedBytes),
      txId: info.txId,
      feePaid: composed.feePaid,
      inputNotes: composed.inputNotes
    };
  });
}

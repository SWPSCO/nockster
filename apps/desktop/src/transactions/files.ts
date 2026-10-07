export const transactionFileTypes = '.jam,.tx,.signed,.draft,.noun,.psnt,.wallet';
export type TransactionTreeNode = { label: string; value: string; children: TransactionTreeNode[] };

export function bytesToBase64(bytes: Uint8Array): string {
  let binary = '';
  for (let offset = 0; offset < bytes.length; offset += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + 0x8000));
  }
  return btoa(binary);
}

export function base64ToBytes(value: string): Uint8Array {
  return Uint8Array.from(atob(value), character => character.charCodeAt(0));
}

export async function readTransactionFile(file: File): Promise<Uint8Array> {
  if (!file.size) throw new Error('The selected file is empty.');
  return new Uint8Array(await file.arrayBuffer());
}

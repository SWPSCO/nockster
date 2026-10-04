import bs58 from 'bs58';

export function parseRecipientAddress(value: string): string {
  let address = value.trim();
  if (address.length > 4096) throw new Error('QR code is too large');
  if (address.includes(':')) {
    const uri = new URL(address);
    const recipients = uri.searchParams.getAll('to');
    if (
      !['nockster:', 'web+nockster:'].includes(uri.protocol) ||
      !['send', 'pay'].includes(uri.hostname) ||
      uri.username ||
      uri.password ||
      uri.port ||
      !['', '/'].includes(uri.pathname) ||
      uri.hash ||
      recipients.length !== 1
    )
      throw new Error('Scan a single Nockchain address');
    address = recipients[0].trim();
  }
  if (!/^[1-9A-HJ-NP-Za-km-z]{40,60}$/.test(address) || bs58.decode(address).length > 40)
    throw new Error('QR code does not contain a Nockchain address');
  return address;
}

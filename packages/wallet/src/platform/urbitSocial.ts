export interface PublishedAddress {
  ship: string;
  address: string | null;
}

export const hasShipSocial =
  import.meta.env.MODE === 'urbit' && location.pathname.startsWith('/apps/nockster');

async function request(path = '', method = 'GET', address?: string): Promise<PublishedAddress> {
  const response = await fetch(`/nockster/social${path}`, {
    method,
    credentials: 'same-origin',
    redirect: 'error',
    headers: { 'X-Nockster-Social': '1', 'Content-Type': 'text/plain' },
    ...(address === undefined ? {} : { body: address }),
    signal: AbortSignal.timeout(25_000)
  });
  if (!response.ok) {
    if (response.status === 400)
      throw new Error('Enter a valid ship name, such as ~sampel-palnet.');
    if (response.status === 404)
      throw new Error('This ship does not have an available Nockster profile.');
    if (response.status === 504)
      throw new Error('The ship did not respond. It may be offline. Try again.');
    if (response.status === 429)
      throw new Error('Too many lookups are running. Try again shortly.');
    throw new Error(`Ship address request failed (${response.status}).`);
  }
  const value: unknown = await response.json();
  const record = value as Partial<PublishedAddress> | null;
  if (
    !record ||
    typeof record.ship !== 'string' ||
    !record.ship.startsWith('~') ||
    !(
      record.address === null ||
      (typeof record.address === 'string' && /^[1-9A-HJ-NP-Za-km-z]{30,128}$/.test(record.address))
    )
  )
    throw new Error('The ship returned an invalid address.');
  return record as PublishedAddress;
}

export function readPublishedAddress() {
  return request();
}

export function publishAddress(address: string) {
  return request('', 'PUT', address);
}

export function unpublishAddress() {
  return request('', 'DELETE');
}

export async function lookupShipAddress(input: string) {
  const ship = input.trim().toLowerCase();
  if (!/^~[a-z]+(?:-+[a-z]+)*$/.test(ship) || ship.length > 128)
    throw new Error('Enter a ship name beginning with ~.');
  const record = await request(`/lookup/${encodeURIComponent(ship)}`);
  if (record.ship !== ship) throw new Error('The returned ship does not match your lookup.');
  return record;
}

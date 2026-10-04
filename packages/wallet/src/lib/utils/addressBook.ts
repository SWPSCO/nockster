import { getRPCClientV1 } from './rpc';

export interface AddressAlias {
  id: string;
  alias: string;
  address: string;
  notes?: string | null;
  updatedAt: string;
}
export async function getAddressBook(): Promise<AddressAlias[]> {
  const result = await getRPCClientV1().request<{ entries: AddressAlias[] }>('getAddressBook');
  if (!Array.isArray(result?.entries)) throw new Error('Invalid address book response');
  return result.entries;
}
export function saveAddressAlias(address: string, alias: string): Promise<AddressAlias> {
  if (!address.trim() || !alias.trim()) throw new Error('Enter an address and nickname');
  return getRPCClientV1().request('saveAddressAlias', [
    { address: address.trim(), alias: alias.trim() }
  ]);
}
export function deleteAddressAlias(id: string): Promise<{ deleted: boolean }> {
  return getRPCClientV1().request('deleteAddressAlias', [{ id }]);
}

import { get } from 'svelte/store';
import { walletStore } from '../lib/stores/wallet';
import { NETWORK } from '../lib/constants';
import { handleVaultMessage, type VaultMessage } from '../vault/engine';
import { vaultStorage } from './vault';

const inProcess = ['mobile', 'desktop', 'urbit'].includes(import.meta.env.MODE);
const throughShip =
  import.meta.env.MODE === 'urbit' && location.pathname.startsWith('/apps/nockster');
export const ACCOUNTS_URL = (import.meta.env.VITE_ACCOUNTS_URL || 'https://nockblocks.com').replace(
  /\/$/,
  ''
);
const network = import.meta.env.VITE_RPC_NETWORK || 'mainnet';
type Credential = { key: string; expiresAt: string; id: string };
type AccountSession = { cookie?: string };
type Identity = {
  nickname: string;
  address: string;
  context: string;
  storageKey: string;
  epoch: number;
};
let epoch = 0;
let cache: { context: string; credential: Credential } | undefined;
let pending: { context: string; promise: Promise<Credential> } | undefined;
let controller = new AbortController();
const rejected = new Set<string>();

export function clearRpcAuth() {
  epoch++;
  controller.abort();
  controller = new AbortController();
  cache = undefined;
  pending = undefined;
  rejected.clear();
}

async function vault<T>(message: VaultMessage): Promise<T> {
  const reply = await handleVaultMessage(message);
  if (!reply.success) throw new Error(reply.error || 'Vault operation failed');
  return reply.data as T;
}

function check(identity: Identity) {
  const state = get(walletStore);
  if (
    identity.epoch !== epoch ||
    (inProcess && (state.isLocked || state.activeWallet?.addresses[0] !== identity.address))
  )
    throw new Error('Wallet session ended');
}

async function identity(): Promise<Identity> {
  const currentEpoch = epoch;
  const status = await vault<{ unlocked: boolean }>({ type: 'vault:status' });
  const state = inProcess
    ? get(walletStore)
    : ((await vaultStorage.get(['walletState'])).walletState as {
        activeWallet?: { id: string; name: string; addresses: string[] };
        isLocked?: boolean;
      });
  const wallet = state?.activeWallet;
  if (!status.unlocked || (inProcess && state.isLocked) || !wallet || currentEpoch !== epoch)
    throw new Error('Unlock your wallet first');
  const address = wallet.addresses[0];
  const context = JSON.stringify([
    ACCOUNTS_URL,
    network,
    NETWORK.RPC_URL,
    NETWORK.RPC_V1_URL,
    address
  ]);
  return {
    nickname: wallet.id.startsWith('vault-') ? wallet.id.slice(6) : wallet.name,
    address,
    context,
    storageKey: `fletch_rpc_iris_v1:${context}`,
    epoch
  };
}

async function accounts<T>(
  path: string,
  who: Identity,
  body?: unknown,
  session?: AccountSession
): Promise<T> {
  check(who);
  const signal = AbortSignal.any([controller.signal, AbortSignal.timeout(20_000)]);
  const response = await fetch(`${throughShip ? '/nockster/accounts' : ACCOUNTS_URL}${path}`, {
    method: body === undefined ? 'GET' : 'POST',
    credentials: throughShip ? 'same-origin' : 'include',
    redirect: 'error',
    headers: {
      'Content-Type': 'application/json',
      Accept: 'application/json',
      ...(throughShip ? { 'X-Nockster-Proxy': '1' } : {}),
      ...(throughShip && session?.cookie ? { 'X-Nockster-Session': session.cookie } : {})
    },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
    signal
  });
  check(who);
  if (!response.ok) throw new Error(`Nockblocks sign-in failed (${response.status})`);
  if (throughShip && session && path === '/auth/iris/token') {
    const cookie = response.headers.get('x-nockster-session')?.split(';', 1)[0];
    if (!cookie || !/^[^\s=;,]+=[^\s;,]+$/.test(cookie))
      throw new Error('Nockblocks did not issue a sign-in session.');
    session.cookie = cookie;
  }
  const result: T = await response.json();
  check(who);
  return result;
}

function valid(value: unknown): value is Credential {
  const item = value as Partial<Credential> | null;
  return Boolean(
    item &&
    typeof item.key === 'string' &&
    item.key.startsWith('ak_') &&
    typeof item.id === 'string' &&
    typeof item.expiresAt === 'string' &&
    Date.parse(item.expiresAt) > Date.now() + 60_000
  );
}

async function authenticate(who: Identity, session: AccountSession) {
  const origin = globalThis.location.origin;
  const challenge = await accounts<{
    challenge_id: string;
    nonce: string;
    message: string;
    address: string;
    account_type: string;
    issued_at: string;
    expires_at: string;
  }>('/auth/iris/challenge', who, { address: who.address, account_type: 'v1', origin });
  const issued = Date.parse(challenge.issued_at);
  const expires = Date.parse(challenge.expires_at);
  if (
    !Number.isFinite(issued) ||
    !Number.isFinite(expires) ||
    expires <= Date.now() ||
    issued > Date.now() + 60_000 ||
    Date.now() - issued > 600_000 ||
    !challenge.challenge_id ||
    !challenge.nonce ||
    challenge.nonce.includes('\n') ||
    challenge.address !== who.address ||
    challenge.account_type !== 'v1'
  )
    throw new Error('Invalid Iris V1 login challenge');
  const issuedAt = new Date(issued).toISOString().replace(/\.\d{3}Z$/, 'Z');
  const expected = `Nockblocks Iris authentication\nAddress: ${who.address}\nOrigin: ${origin}\nChallenge: ${challenge.nonce}\nIssued At: ${issuedAt}`;
  if (challenge.message !== expected) throw new Error('Invalid Iris V1 login message');
  check(who);
  const signed = await vault<{ signature: { c: string; s: string }; public_key: string }>({
    type: 'vault:signIrisV1',
    nickname: who.nickname,
    address: who.address,
    message: expected
  });
  check(who);
  await accounts(
    '/auth/iris/token',
    who,
    {
      address: who.address,
      account_type: 'v1',
      challenge_id: challenge.challenge_id,
      nonce: challenge.nonce,
      message: expected,
      ...signed,
      origin,
      link_account: false
    },
    session
  );
}

async function obtain(who: Identity): Promise<Credential> {
  const saved = (await vaultStorage.get([who.storageKey]))[who.storageKey];
  check(who);
  if (typeof saved === 'string') {
    const decrypted = await vault<string>({
      type: 'vault:openSecret',
      context: who.context,
      envelope: saved
    });
    check(who);
    const credential: unknown = JSON.parse(decrypted);
    if (valid(credential) && !rejected.has(credential.key)) return credential;
  }

  const session: AccountSession = {};
  await authenticate(who, session);
  const expiresAt = new Date(Date.now() + 30 * 24 * 60 * 60 * 1000).toISOString();
  const result = await accounts<{ key: string; api_key: { id: string; expires_at: string } }>(
    '/auth/keys',
    who,
    {
      name:
        import.meta.env.MODE === 'urbit'
          ? 'Nockster Urbit'
          : import.meta.env.MODE === 'desktop'
            ? 'Nockster desktop'
            : inProcess
              ? 'Nockster mobile'
              : 'Nockster extension',
      networks: [network],
      expires_at: expiresAt
    },
    session
  );
  session.cookie = undefined;
  const credential = {
    key: result.key,
    id: result.api_key?.id,
    expiresAt: result.api_key?.expires_at
  };
  if (!valid(credential)) throw new Error('Invalid RPC API key response');
  const envelope = await vault<string>({
    type: 'vault:sealSecret',
    context: who.context,
    value: JSON.stringify(credential)
  });
  check(who);
  await vaultStorage.set({ [who.storageKey]: envelope });
  check(who);
  return credential;
}

async function credential(who: Identity): Promise<Credential> {
  check(who);
  if (
    cache?.context === who.context &&
    valid(cache.credential) &&
    !rejected.has(cache.credential.key)
  )
    return cache.credential;
  if (!pending || pending.context !== who.context) {
    const operation = obtain(who)
      .then(result => {
        check(who);
        cache = { context: who.context, credential: result };
        return result;
      })
      .finally(() => {
        if (pending?.promise === operation) pending = undefined;
      });
    pending = { context: who.context, promise: operation };
  }
  return pending.promise;
}

/** Sign in as the selected wallet without returning its credential to the UI. */
export async function ensureRpcAuth(): Promise<void> {
  const who = await identity();
  await credential(who);
  check(who);
}

export const GRPC_URL = (
  import.meta.env.VITE_WALLET_GRPC_URL || new URL(NETWORK.RPC_URL).origin
).replace(/\/$/, '');
export const GRPC_SERVICE = '/nockchain.public.v2.NockchainService/';
export async function fetchAuthenticatedRpc(
  url: string,
  body: BodyInit,
  signal: AbortSignal,
  headers: Record<string, string> = {},
  transport: typeof fetch = fetch
): Promise<Response> {
  if (
    ![
      NETWORK.RPC_URL,
      NETWORK.RPC_V1_URL,
      `${GRPC_URL}${GRPC_SERVICE}WalletSendTransaction`,
      `${GRPC_URL}${GRPC_SERVICE}TransactionAccepted`
    ].includes(url)
  )
    throw new Error('Untrusted RPC endpoint');
  const who = await identity();
  for (let attempt = 0; attempt < 2; attempt++) {
    const key = await credential(who);
    check(who);
    const response = await transport(url, {
      method: 'POST',
      credentials: 'omit',
      redirect: 'error',
      headers: {
        'Content-Type': 'application/json',
        ...headers,
        Authorization: `Bearer ${key.key}`
      },
      body,
      signal: AbortSignal.any([signal, controller.signal])
    });
    check(who);
    if (response.status !== 401 || attempt === 1) return response;
    // Only the rejected credential is discarded. Parallel 401s share one new login.
    rejected.add(key.key);
    if (cache?.credential.key === key.key) cache = undefined;
    check(who);
  }
  throw new Error('RPC authentication failed');
}

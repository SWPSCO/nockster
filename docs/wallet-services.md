# Wallet services

Mobile and the extension use the same wallet identity, address book, and
transaction gateway. The pinned `iris-v1` SDK signs login challenges with the
saved Nockchain key and encodes public V2 gRPC-web transaction requests.

## Services and routing

The accounts API provides `/auth/iris/challenge`, `/auth/iris/token`, and
`/auth/keys`. A successful wallet signature creates or signs into its account.
Nockster requests a network-scoped API key with a 30-day expiry, encrypts it with
the vault master key, and sends it as `Authorization: Bearer ...` to RPC.
Expired or rejected keys trigger a new wallet signature. Locking clears memory
and cancels in-flight authentication. Private keys and recovery phrases never
reach these services.

The RPC API configuration includes:

- Curator's primary node `url` and `authorization_bearer`: its authenticated
  private gRPC connection handles submission and acceptance checks. The wallet
  does not need a public node listener or an external RPC provider.
- `accounts_api_url`, `accounts_api_service_token`, and `network_slug`: the
  accounts service and its configured service credential/network.
- The existing NATS connection: JetStream persists transaction tracking in a
  network-specific `wallet_submissions_...` KV bucket. Retain this storage across
  API restarts and include it in operational backups.

The API sends network-scoped wallet requests over NATS to Curator. Curator
accepts only transaction submission and acceptance checks, translates V2
protobufs into the node's `heard-tx` poke or `tx-accepted` peek, and uses the
primary node connection and its configured authentication. An acknowledgement
means the poke completed; a separate acceptance check determines acceptance.
Deploy both `nockblocks-api` and `curator` together.

The authenticated gRPC-web routes are:

```text
/nockchain.public.v2.NockchainService/WalletSendTransaction
/nockchain.public.v2.NockchainService/TransactionAccepted
```

Deploy the accounts API and RPC API from `../nockchain-rpc-server`. Deploy the
worker code and route configuration in `../edge/prod/rpc-cache-worker`, plus the
`lb.nockblocks.com` Caddy configuration in `../edge/dev/Caddyfile`. These routes
forward the caller's authorization and binary body unchanged, disable caching,
and preserve gRPC response bytes and status headers. They do not retry a send.
The native client uses explicit base64 bridge transport for binary HTTP bodies;
this encoding is local to Capacitor and is not sent to the server.

## Private JSON RPC methods

Send authenticated JSON-RPC 2.0 POST requests to `/rpc/v1`. Parameters use an array
containing one object; `getAddressBook` also accepts an empty array.

| Method | Parameters | Result |
| --- | --- | --- |
| `getAddressBook` | `[]` | `{entries: [{id, address, alias, notes, updatedAt}]}` |
| `saveAddressAlias` | `[{address, alias}]` | Saved entry; updates the same address and preserves notes |
| `deleteAddressAlias` | `[{id}]` | `{deleted: boolean}` |
| `getTransactionSubmission` | `[{txId}]` | `{txId, status, tracked, updatedAt?, checkFailed?}` |

Account ownership comes from the verified API key. Clients cannot select another
account ID. The accounts API's `/internal/wallet/address-book` endpoint requires
the RPC service credential. The worker bypasses shared caching for all four
methods. Address aliases use the same account-owned rows as the Nockblocks site.

Submission tracking distinguishes `submitting`, `acknowledged`, `accepted`,
`rejected`, and `unknown`. Acknowledgement means the node received the request;
acceptance comes from a separate node query. Neither means block confirmation.
An interrupted request can leave an uncertain status. Poll by transaction ID
before an explicit retry; status queries never rebroadcast. The gateway rejects
a different payload for a tracked ID, deduplicates acknowledged/accepted sends,
and applies a 60-second lease before another attempt after uncertainty or
rejection. Pending wallet inputs remain reserved until chain history confirms
the payment.

## Automatic Nockblocks sign-in

The wallet signs a server-issued Iris challenge with the selected wallet's key.
Nockblocks resolves the verified address to its bound account and creates a
wallet account when no binding exists. Importing keys starts this flow without
browser interaction. A saved contact or watched address does not establish an
account binding. An unrelated browser session is not used to merge accounts.

The API key is encrypted in the vault and bound to the wallet address, service,
network, and RPC endpoints. Existing credentials are reused while valid. Network
failures leave wallet creation intact; a later request retries sign-in. Locking
or switching wallets cancels in-flight authentication and clears in-memory
credentials. Account-owned address aliases follow the authenticated wallet.

## Client configuration and checks

The defaults are `https://nockblocks.com` for accounts and wallet gRPC,
`https://nockblocks.com/rpc` and `/rpc/v1` for JSON RPC, and `mainnet` for the API
key network. Set `VITE_ACCOUNTS_URL`, `VITE_WALLET_GRPC_URL`, `VITE_RPC_URL`,
`VITE_RPC_V1_URL`, and `VITE_RPC_NETWORK` at build time to target another
installation. Mobile reads environment files from `apps/mobile/`; extension builds
read the root environment files. Extension host permissions must include every
configured remote origin.

```sh
npm run typecheck
npm test
npm run test:mobile
npm run build:svelte
npm run test:extension
```

The browser tests intercept external traffic and never broadcast transactions.
The extension test loads the built package into Chromium and exercises its
actual service worker. Backend integration tests use isolated PostgreSQL and
NATS instances via `NOCKSTER_TEST_ACCOUNTS_DATABASE_URL` and
`NOCKSTER_TEST_NATS_URL`; enable their ignored tests explicitly. The worker tests
verify byte-preserving forwarding and private RPC cache bypass.


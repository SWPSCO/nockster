#!/usr/bin/env python3
"""Capture a small public-chain snapshot. Credentials are read without echo."""
import argparse
import datetime
import getpass
import json
from pathlib import Path
import sys
import urllib.error
import urllib.request


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('address', help='Public demonstration wallet address')
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    key = getpass.getpass('Nockblocks API key (not saved): ')
    if not key.strip():
        parser.error('An API key is required')
    requests = [
        ('v0', 'getTip', []),
        ('v0', 'getNotes', [{'address': args.address}]),
        ('v0', 'getTransactionsByAddress', [{'address': args.address, 'limit': 20, 'offset': 0}]),
        ('v0', 'getBlockchainMetrics', []),
        ('v1', 'getTip', [{'includeTransactions': False}]),
        ('v1', 'getNotesByAddress', [{'address': args.address}]),
    ]
    opener = urllib.request.build_opener(NoRedirect())
    records = []
    for version, method, params in requests:
        endpoint = 'https://nockblocks.com/rpc' + ('/v1' if version == 'v1' else '')
        body = json.dumps(dict(jsonrpc='2.0', id=1, method=method, params=params)).encode()
        request = urllib.request.Request(endpoint, data=body, headers={
            'Content-Type': 'application/json', 'Authorization': f'Bearer {key}',
            'User-Agent': 'Nockster-Fixture-Capture/1.0',
        })
        with opener.open(request, timeout=30) as response:
            raw = response.read(4 * 1024 * 1024 + 1)
        if len(raw) > 4 * 1024 * 1024:
            raise ValueError(f'{method}: response too large')
        data = json.loads(raw)
        if data.get('error') or 'result' not in data:
            raise ValueError(f'{method}: RPC request failed (response omitted)')
        # Keep only public method results and the optional top-level price.
        envelope = {'jsonrpc': '2.0', 'result': data['result']}
        if isinstance(data.get('price'), (int, float)):
            envelope['price'] = data['price']
        records.append(dict(version=version, method=method, params=params, response=envelope))
        print(f'Captured {version}/{method}', flush=True)
    snapshot = dict(schemaVersion=1, source='https://nockblocks.com',
                    capturedAt=datetime.datetime.now(datetime.timezone.utc).isoformat(),
                    address=args.address, records=records)
    encoded = json.dumps(snapshot, indent=2, allow_nan=False) + '\n'
    # Fail closed if any credential-like content unexpectedly appears in a result.
    if key in encoded or 'ak_live_' in encoded or any(
        f'"{field}"' in encoded.lower()
        for field in ('authorization', 'mnemonic', 'privatekey', 'private_key', 'access_token', 'refresh_token')
    ):
        raise ValueError('Unexpected sensitive content; nothing saved')
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(encoded)
    print(f'Saved {len(records)} public RPC responses to {args.output}')


if __name__ == '__main__':
    try:
        main()
    except urllib.error.HTTPError as error:
        body = error.read(65536).lower()
        reason = ('edge challenge' if b'cloudflare' in body or b'just a moment' in body
                  else 'API authentication/permission response' if b'api key' in body or b'forbidden' in body
                  else 'server rejection')
        sys.exit(f'HTTP {error.code}: {reason}; response omitted to protect credentials')
    except (urllib.error.URLError, TimeoutError):
        sys.exit('Network request failed; no snapshot saved')
    except (ValueError, OSError) as error:
        # JSON parsing errors can contain response fragments; do not print them.
        sys.exit('Capture failed validation or could not save the snapshot')

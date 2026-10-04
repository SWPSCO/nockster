#!/usr/bin/env python3
"""
Submit a signed transaction file to the Nockster wallet gateway.

Usage:
    python3 scripts/submit-tx.py ~/split.signed

This replicates the same auth + RPC flow the wallet extension uses:
  1. Generate Ed25519 device keypair (or reuse cached one)
  2. Fetch nonce from gateway
  3. Sign nonce, exchange for JWT
  4. Build JSON-RPC body for submitTransaction
  5. Compute X-Client-Proof header
  6. POST to /wallet-gw/rpc
"""

import sys, os, json, time, hashlib, base64, secrets
import urllib.request, urllib.error

# pip install cryptography  (or: pip install PyNaCl)
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives import serialization

GATEWAY_BASE = os.environ.get("NOCKSTER_GATEWAY", "https://fletch.dev.nockblocks.com")
RPC_PATH = "/wallet-gw/rpc"
NONCE_URL = f"{GATEWAY_BASE}/wallet-gw/nonce"
TOKEN_URL = f"{GATEWAY_BASE}/wallet-gw/token"
RPC_URL = f"{GATEWAY_BASE}{RPC_PATH}"

KEYFILE = os.path.expanduser("~/.fletch-cli-key.json")


# ── helpers ──────────────────────────────────────────────────────────

def b64url_encode(data: bytes) -> str:
    return base64.urlsafe_b64encode(data).rstrip(b"=").decode()

def b64url_decode(s: str) -> bytes:
    pad = 4 - len(s) % 4
    if pad != 4:
        s += "=" * pad
    return base64.urlsafe_b64decode(s)

def sha256hex(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()

def post_json(url: str, payload: dict, headers: dict | None = None) -> dict:
    body = json.dumps(payload).encode()
    hdrs = {"Content-Type": "application/json"}
    if headers:
        hdrs.update(headers)
    req = urllib.request.Request(url, data=body, headers=hdrs, method="POST")
    with urllib.request.urlopen(req) as resp:
        return json.loads(resp.read())


# ── Ed25519 key management ──────────────────────────────────────────

def load_or_create_keypair() -> tuple[Ed25519PrivateKey, bytes]:
    """Returns (private_key, public_key_raw_bytes)."""
    if os.path.exists(KEYFILE):
        with open(KEYFILE) as f:
            data = json.load(f)
        priv_bytes = b64url_decode(data["priv"])
        priv = serialization.load_der_private_key(priv_bytes, password=None)
        pub_raw = b64url_decode(data["pub"])
        return priv, pub_raw

    priv = Ed25519PrivateKey.generate()
    priv_der = priv.private_bytes(
        serialization.Encoding.DER,
        serialization.PrivateFormat.PKCS8,
        serialization.NoEncryption(),
    )
    pub_raw = priv.public_key().public_bytes(
        serialization.Encoding.Raw,
        serialization.PublicFormat.Raw,
    )
    with open(KEYFILE, "w") as f:
        json.dump({"priv": b64url_encode(priv_der), "pub": b64url_encode(pub_raw)}, f)
    print(f"[+] Created device keypair at {KEYFILE}")
    return priv, pub_raw


# ── Auth flow ────────────────────────────────────────────────────────

def get_jwt(priv: Ed25519PrivateKey, pub_raw: bytes) -> str:
    pub_b64 = b64url_encode(pub_raw)

    # 1) get nonce
    resp = post_json(NONCE_URL, {})
    nonce = resp["nonce"]
    print(f"[+] Got nonce: {nonce[:20]}...")

    # 2) sign (nonce || pubkey)
    msg = (nonce + pub_b64).encode()
    sig = priv.sign(msg)
    sig_b64 = b64url_encode(sig)

    # 3) exchange for token
    resp = post_json(TOKEN_URL, {"nonce": nonce, "pubkey": pub_b64, "sig": sig_b64})
    token = resp["token"]
    print("[+] Got JWT token")
    return token


def make_proof(priv: Ed25519PrivateKey, pub_raw: bytes, path: str, body_str: str) -> str:
    ts = int(time.time())
    body_sha = sha256hex(body_str.encode())
    pub_b64 = b64url_encode(pub_raw)

    payload_no_sig = {
        "ts": ts,
        "method": "POST",
        "path": path,
        "body_sha": body_sha,
        "pubkey": pub_b64,
    }
    # Sign over JSON with sig=""  (matches wallet's makeProofHeader)
    to_sign = json.dumps({**payload_no_sig, "sig": ""}).encode()
    sig = priv.sign(to_sign)

    proof = {**payload_no_sig, "sig": b64url_encode(sig)}
    return json.dumps(proof)


# ── Main ─────────────────────────────────────────────────────────────

def main():
    if len(sys.argv) < 2:
        print(f"Usage: {sys.argv[0]} <signed-tx-file>")
        sys.exit(1)

    tx_file = sys.argv[1]
    with open(tx_file, "rb") as f:
        raw = f.read()

    # The wallet sends the signed tx as a base64 string
    raw_tx_b64 = base64.b64encode(raw).decode()
    print(f"[+] Read {len(raw)} bytes from {tx_file}")
    print(f"[+] Base64 encoded: {len(raw_tx_b64)} chars")

    priv, pub_raw = load_or_create_keypair()
    jwt_token = get_jwt(priv, pub_raw)

    # Build JSON-RPC body (same as wallet's submitTransaction)
    rpc_body = {
        "jsonrpc": "2.0",
        "method": "submitTransaction",
        "params": [{"rawTransaction": raw_tx_b64}],
        "id": secrets.token_hex(5),
    }
    body_str = json.dumps(rpc_body)

    # Build auth headers
    proof = make_proof(priv, pub_raw, RPC_PATH, body_str)
    headers = {
        "Content-Type": "application/json",
        "Authorization": f"Bearer {jwt_token}",
        "X-Client-Proof": proof,
    }

    print(f"[+] Submitting to {RPC_URL}")

    req = urllib.request.Request(RPC_URL, data=body_str.encode(), headers=headers, method="POST")
    try:
        with urllib.request.urlopen(req) as resp:
            result = json.loads(resp.read())
            print(json.dumps(result, indent=2))
            if result.get("error"):
                print(f"[!] RPC error: {result['error']}")
                sys.exit(1)
            else:
                print(f"[+] Transaction submitted: {result.get('result')}")
    except urllib.error.HTTPError as e:
        body = e.read().decode()
        print(f"[!] HTTP {e.code}: {body}")
        sys.exit(1)


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""
Generate a BIP39 mnemonic and seed for testing.
WARNING: This is for TESTING ONLY. Never use these keys for real funds.
"""

from mnemonic import Mnemonic
import hashlib
import hmac

# Generate a test mnemonic (12 words)
mnemo = Mnemonic("english")
words = mnemo.generate(strength=128)  # 128 bits = 12 words

print("TEST MNEMONIC (NEVER USE FOR REAL FUNDS!):")
print(words)
print()

# Convert to seed (64 bytes)
seed = mnemo.to_seed(words, passphrase="")
print(f"BIP39 Seed (hex): {seed.hex()}")
print()

# Export command for testing
print("To test your firmware, run:")
print(f"export BIP39_SEED_HEX={seed.hex()}")
print("./ping")
print()

# Show some derived paths for reference
import hmac
import hashlib

def get_master_key(seed):
    """Derive master key from seed"""
    return hmac.new(b"Bitcoin seed", seed, hashlib.sha512).digest()

master = get_master_key(seed)
master_privkey = master[:32]
master_chaincode = master[32:]

print(f"Master private key: {master_privkey.hex()}")
print(f"Master chain code: {master_chaincode.hex()}")

# Calculate master fingerprint (you can verify this matches your firmware)
from hashlib import sha256
import hashlib

# For fingerprint calculation, you'd need the compressed public key
# This is just to show the structure
print("\nCommon derivation paths:")
print("m/44'/0'/0'/0/0  - Bitcoin first address")
print("m/44'/60'/0'/0/0 - Ethereum first address")

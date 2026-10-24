"""Envelope encryption for users' provider keys (spec 7.2). Pure: the master key is passed in.

Each key row gets its own random 256-bit data key. The data key encrypts the API key (AES-GCM) and
is itself wrapped by the master key (AES-GCM). Stored layout (VARBINARY):

    version(1) | wrap_nonce(12) | wrapped_dek(48) | nonce(12) | ciphertext(+16 tag)

The row's PublicId is bound as associated data, so a ciphertext can't be moved to another row.
"""

import base64
import os

from cryptography.hazmat.primitives.ciphers.aead import AESGCM

VERSION = 1
_WRAPPED_LEN = 32 + 16


class CryptoError(ValueError):
    pass


def master_key(b64: str) -> bytes:
    key = base64.b64decode(b64)
    if len(key) != 32:
        raise CryptoError("the master key must be 32 bytes (base64)")
    return key


def seal(master: bytes, plaintext: str, row_id: str) -> bytes:
    dek = AESGCM.generate_key(bit_length=256)
    wrap_nonce, nonce = os.urandom(12), os.urandom(12)
    wrapped = AESGCM(master).encrypt(wrap_nonce, dek, row_id.encode())
    ct = AESGCM(dek).encrypt(nonce, plaintext.encode(), row_id.encode())
    return bytes([VERSION]) + wrap_nonce + wrapped + nonce + ct


def open_sealed(master: bytes, blob: bytes, row_id: str) -> str:
    if not blob or blob[0] != VERSION:
        raise CryptoError("unknown ciphertext version")
    wrap_nonce = blob[1:13]
    wrapped = blob[13 : 13 + _WRAPPED_LEN]
    nonce = blob[13 + _WRAPPED_LEN : 25 + _WRAPPED_LEN]
    ct = blob[25 + _WRAPPED_LEN :]
    try:
        dek = AESGCM(master).decrypt(wrap_nonce, wrapped, row_id.encode())
        return AESGCM(dek).decrypt(nonce, ct, row_id.encode()).decode()
    except Exception as e:  # InvalidTag: wrong master key, wrong row, or tampered
        raise CryptoError("can't decrypt this key") from e


def last4(api_key: str) -> str:
    tail = api_key.strip()[-4:]
    return tail.rjust(4, "*")

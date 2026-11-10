#!/usr/bin/env python3
"""Reference side for excubia-core-pake: python-spake2 (Ed25519 group) plus the same HMAC confirmation.

  interop.py vector      print the deterministic vector as JSON (pasted into tests/vectors.rs)
  interop.py peer ROLE CODE HOST_ID DEVICE_PUBKEY_HEX RUST_MESSAGE_HEX EXPORTER_HEX SPKI_HEX
                         ROLE is host|device, the role PYTHON plays (fresh os.urandom entropy). Prints
                         message=, key= and tag= (its own confirmation tag) as hex.

Needs `pip install spake2` (tested with 0.9) in a venv outside the repo.
"""
import hashlib
import hmac
import json
import os
import sys

from spake2 import SPAKE2_A, SPAKE2_B
from spake2.ed25519_basic import L

DOMAIN = b"excubia-pake-confirm-v1\0"
ROLE_BYTE = {"host": 1, "device": 2}

# Fixed inputs of the vector (the Rust test has the same constants).
CODE = "K7M2QXTZ"
HOST_ID = "excubia-host-1"
DEVICE_PUBKEY = bytes([4]) + bytes(range(1, 65))
EXPORTER = bytes(range(32))
SPKI = bytes(range(100, 132))


def spake(role, code, host_id, device_pubkey, entropy_f=os.urandom):
    cls = SPAKE2_A if role == "device" else SPAKE2_B  # device is side A, host is side B
    return cls(code.encode("ascii"), idA=device_pubkey, idB=host_id.encode("utf-8"), entropy_f=entropy_f)


def tag(key, role, exporter, spki):
    return hmac.new(key, DOMAIN + bytes([ROLE_BYTE[role]]) + exporter + spki, hashlib.sha256).digest()


def scalar_for(label):
    return int.from_bytes(hashlib.sha256(label).digest(), "little") % L


def vector():
    out = {"code": CODE, "host_id": HOST_ID, "device_pubkey": DEVICE_PUBKEY.hex(),
           "exporter": EXPORTER.hex(), "spki": SPKI.hex()}
    sides = {}
    for role in ("device", "host"):
        x = scalar_for(b"excubia-vector-" + role.encode())
        # python reads 64 bytes as one big-endian integer; the Rust crate reads them as a little-endian wide scalar.
        py_entropy = bytes(32) + x.to_bytes(32, "big")
        rust_entropy = x.to_bytes(32, "little") + bytes(32)
        s = spake(role, CODE, HOST_ID, DEVICE_PUBKEY, lambda n, e=py_entropy: e[:n] if n == 64 else None)
        sides[role] = (s, s.start())
        out[role + "_entropy"] = rust_entropy.hex()
        out[role + "_message"] = sides[role][1].hex()
    key_d = sides["device"][0].finish(sides["host"][1])
    key_h = sides["host"][0].finish(sides["device"][1])
    assert key_d == key_h
    out["key"] = key_d.hex()
    out["host_tag"] = tag(key_d, "host", EXPORTER, SPKI).hex()
    out["device_tag"] = tag(key_d, "device", EXPORTER, SPKI).hex()
    print(json.dumps(out, indent=2))


def peer(role, code, host_id, device_pubkey, rust_message, exporter, spki):
    # os.urandom entropy: this direction is about interoperability, not determinism
    s = spake(role, code, host_id, bytes.fromhex(device_pubkey))
    message = s.start()
    key = s.finish(bytes.fromhex(rust_message))
    print("message=" + message.hex())
    print("key=" + key.hex())
    print("tag=" + tag(key, role, bytes.fromhex(exporter), bytes.fromhex(spki)).hex())


if __name__ == "__main__":
    if sys.argv[1:] == ["vector"]:
        vector()
    elif len(sys.argv) == 9 and sys.argv[1] == "peer" and sys.argv[2] in ROLE_BYTE:
        peer(*sys.argv[2:])
    else:
        sys.exit(__doc__)

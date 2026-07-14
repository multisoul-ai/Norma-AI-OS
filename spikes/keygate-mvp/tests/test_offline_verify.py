"""Unit tests for Keygate offline token verification (script spike seam).

Seam under test: verify_offline_token(token, public_key_hex) -> payload dict
Independent of Keygate server — uses a locally generated Ed25519 keypair.
"""

from __future__ import annotations

import base64
import json
import sys
import time
import unittest
from pathlib import Path

from nacl.signing import SigningKey

SCRIPTS = Path(__file__).resolve().parents[1] / "scripts"
sys.path.insert(0, str(SCRIPTS))

from offline_verify import OfflineVerifyError, verify_offline_token  # noqa: E402


def _b64url(data: bytes) -> str:
    return base64.urlsafe_b64encode(data).rstrip(b"=").decode("ascii")


def _sign_token(payload: dict, signing_key: SigningKey) -> str:
    raw = json.dumps(payload, separators=(",", ":"), sort_keys=True).encode("utf-8")
    # Keygate signs the base64url(payload) bytes, not the raw JSON.
    # Sort keys only in tests for stability; server JSON key order may differ
    # but signature covers the exact b64 string embedded in the token.
    b64 = _b64url(raw)
    signed = signing_key.sign(b64.encode("ascii"))
    return f"{b64}.{_b64url(signed.signature)}"


class OfflineVerifyTests(unittest.TestCase):
    def setUp(self) -> None:
        self.signing_key = SigningKey.generate()
        self.public_hex = bytes(self.signing_key.verify_key).hex()

    def test_valid_token_returns_payload(self) -> None:
        payload = {
            "lid": "lic-1",
            "pid": "prod-1",
            "pln": "plan-1",
            "sts": "active",
            "did": "device-spike",
            "iat": int(time.time()) - 10,
            "exp": int(time.time()) + 3600,
            "grc": 7,
            "nce": "abc",
        }
        token = _sign_token(payload, self.signing_key)
        got = verify_offline_token(token, self.public_hex)
        self.assertEqual(got["lid"], "lic-1")
        self.assertEqual(got["did"], "device-spike")
        self.assertEqual(got["sts"], "active")

    def test_tampered_payload_rejected(self) -> None:
        payload = {
            "lid": "lic-1",
            "sts": "active",
            "did": "device-spike",
            "iat": int(time.time()),
            "exp": int(time.time()) + 3600,
            "grc": 0,
            "nce": "n",
            "pid": "p",
            "pln": "pl",
        }
        token = _sign_token(payload, self.signing_key)
        b64, sig = token.rsplit(".", 1)
        raw = json.loads(base64.urlsafe_b64decode(b64 + "=="))
        raw["sts"] = "revoked"
        bad_b64 = _b64url(json.dumps(raw, separators=(",", ":"), sort_keys=True).encode())
        bad_token = f"{bad_b64}.{sig}"
        with self.assertRaises(OfflineVerifyError):
            verify_offline_token(bad_token, self.public_hex)

    def test_expired_token_rejected(self) -> None:
        payload = {
            "lid": "lic-1",
            "sts": "active",
            "did": "device-spike",
            "iat": int(time.time()) - 100,
            "exp": int(time.time()) - 1,
            "grc": 0,
            "nce": "n",
            "pid": "p",
            "pln": "pl",
        }
        token = _sign_token(payload, self.signing_key)
        with self.assertRaises(OfflineVerifyError):
            verify_offline_token(token, self.public_hex)


if __name__ == "__main__":
    unittest.main()

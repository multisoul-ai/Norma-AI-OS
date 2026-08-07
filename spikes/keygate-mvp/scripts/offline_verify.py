#!/usr/bin/env python3
"""Offline verification of Keygate Ed25519 license tokens.

Token format (from Keygate OpenAPI / internal/license/token.go):
  base64url(json_payload) + "." + base64url(ed25519_signature)

The signed message is the base64url payload *string bytes*, not the raw JSON.
"""

from __future__ import annotations

import argparse
import base64
import json
import sys
import time
from typing import Any


class OfflineVerifyError(Exception):
    pass


def _b64url_decode(data: str) -> bytes:
    pad = "=" * (-len(data) % 4)
    return base64.urlsafe_b64decode(data + pad)


def verify_offline_token(token: str, public_key_hex: str, now: int | None = None) -> dict[str, Any]:
    """Verify signature + expiry; return decoded payload dict.

    Raises OfflineVerifyError on any failure.
    """
    if not token or "." not in token:
        raise OfflineVerifyError("invalid token format")
    if not public_key_hex or len(public_key_hex.strip()) != 64:
        raise OfflineVerifyError("public_key must be 64 hex chars (32-byte Ed25519 key)")

    try:
        from nacl.exceptions import BadSignatureError
        from nacl.signing import VerifyKey
    except ImportError as exc:  # pragma: no cover
        raise OfflineVerifyError(
            "PyNaCl is required: python3 -m venv .venv && .venv/bin/pip install -r requirements.txt"
        ) from exc

    idx = token.rfind(".")
    b64, sig_b64 = token[:idx], token[idx + 1 :]
    try:
        sig = _b64url_decode(sig_b64)
        pub = bytes.fromhex(public_key_hex.strip())
    except (ValueError, TypeError) as exc:
        raise OfflineVerifyError(f"decode error: {exc}") from exc

    if len(pub) != 32:
        raise OfflineVerifyError("public key must decode to 32 bytes")

    verify_key = VerifyKey(pub)
    try:
        verify_key.verify(b64.encode("ascii"), sig)
    except BadSignatureError as exc:
        raise OfflineVerifyError("invalid signature") from exc

    try:
        payload = json.loads(_b64url_decode(b64))
    except (ValueError, json.JSONDecodeError) as exc:
        raise OfflineVerifyError(f"payload decode error: {exc}") from exc

    if not isinstance(payload, dict):
        raise OfflineVerifyError("payload must be a JSON object")

    exp = payload.get("exp")
    if isinstance(exp, (int, float)) and exp > 0:
        ts = int(time.time() if now is None else now)
        if ts > int(exp):
            raise OfflineVerifyError("token expired")

    return payload


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Verify a Keygate offline license token")
    parser.add_argument("--token-file", required=True, help="Path to file containing the token")
    parser.add_argument("--pubkey-hex", required=True, help="Ed25519 public key (64 hex chars)")
    parser.add_argument("--json", action="store_true", help="Print payload as JSON")
    args = parser.parse_args(argv)

    try:
        token = open(args.token_file, encoding="utf-8").read().strip()
    except OSError as exc:
        print(f"error: cannot read token file: {exc}", file=sys.stderr)
        return 2

    try:
        payload = verify_offline_token(token, args.pubkey_hex)
    except OfflineVerifyError as exc:
        print(f"FAIL: {exc}", file=sys.stderr)
        return 1

    if args.json:
        print(json.dumps(payload, indent=2, sort_keys=True))
    else:
        print(
            "OK offline verify:",
            f"status={payload.get('sts')}",
            f"license={payload.get('lid')}",
            f"device={payload.get('did')}",
            f"exp={payload.get('exp')}",
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

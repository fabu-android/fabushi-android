#!/usr/bin/env python3
"""Release one Android CI Account Factory lease without exposing credentials."""

from __future__ import annotations

import json
import os
import pathlib
import urllib.error
import urllib.parse
import urllib.request


def main() -> int:
    path = pathlib.Path(os.environ.get("FABUSHI_CI_ACCOUNT_LEASE_FILE", "")).resolve()
    if not path.is_file():
        print("No Android Account Factory lease was allocated.")
        return 0
    value = json.loads(path.read_text(encoding="utf-8"))
    lease_id = value.get("leaseId")
    factory_url = str(value.get("factoryUrl") or "").rstrip("/")
    if not isinstance(lease_id, str) or not lease_id or not factory_url.startswith("https://"):
        raise RuntimeError("Android Account Factory lease metadata is invalid")
    request_url = os.environ.get("ACTIONS_ID_TOKEN_REQUEST_URL", "")
    request_token = os.environ.get("ACTIONS_ID_TOKEN_REQUEST_TOKEN", "")
    if not request_url or not request_token:
        raise RuntimeError("GitHub Actions OIDC is required for lease cleanup")
    parsed = urllib.parse.urlsplit(request_url)
    query = urllib.parse.parse_qs(parsed.query)
    query["audience"] = [factory_url]
    oidc_url = urllib.parse.urlunsplit((parsed.scheme, parsed.netloc, parsed.path, urllib.parse.urlencode(query, doseq=True), ""))
    oidc_request = urllib.request.Request(oidc_url, headers={"Accept": "application/json", "Authorization": f"Bearer {request_token}"})
    with urllib.request.urlopen(oidc_request, timeout=20) as response:
        oidc = json.loads(response.read(16 * 1024 + 1).decode("utf-8"))
    token = oidc.get("value") if isinstance(oidc, dict) else None
    if not isinstance(token, str) or not token:
        raise RuntimeError("GitHub Actions OIDC token was invalid")
    request = urllib.request.Request(
        f"{factory_url}/v1/leases/{lease_id}",
        method="DELETE",
        headers={"Accept": "application/json", "Authorization": f"Bearer {token}"},
    )
    try:
        with urllib.request.urlopen(request, timeout=180) as response:
            result = json.loads(response.read(16 * 1024 + 1).decode("utf-8"))
            if response.status != 200 or result.get("remaining") != 0:
                raise RuntimeError("Android Account Factory cleanup reported residual data")
    except urllib.error.HTTPError as error:
        raise RuntimeError(f"Android Account Factory cleanup failed with HTTP {error.code}") from None
    path.unlink()
    print("Cleaned Android Account Factory lease.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

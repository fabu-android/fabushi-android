#!/usr/bin/env python3
"""Create a bounded refresh-token-free Fabushi Android CI application session."""

from __future__ import annotations

import json
import os
import pathlib
import re
import stat
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request

MAX_SESSION_BYTES = 64 * 1024
MAX_LIFETIME_SECONDS = 5 * 60 * 60
DEVICE_RE = re.compile(r"^gha-([0-9]+)-([0-9]+)-interactive$")
DIGITS_RE = re.compile(r"^[0-9]+$")


def _credential(value: object) -> str:
    return str(value or "").strip()


def valid_credential(value: str) -> bool:
    return 24 <= len(value) <= 16 * 1024 and not any(ch.isspace() for ch in value)


def normalize_base_url(value: str) -> str:
    value = value.strip().rstrip("/")
    parsed = urllib.parse.urlsplit(value)
    if parsed.scheme != "https" or not parsed.hostname or parsed.username or parsed.password:
        raise ValueError("Fabushi auth base URL must be a clean HTTPS origin")
    if parsed.query or parsed.fragment or parsed.path not in ("", "/"):
        raise ValueError("Fabushi auth base URL must not contain path/query/fragment")
    return value


def factory_base_url(env: dict[str, str] | os._Environ[str] | None = None) -> str:
    values = os.environ if env is None else env
    return normalize_base_url(
        values.get("FABUSHI_CI_ACCOUNT_FACTORY_URL")
        or "https://fabushi-ci-isolated-accounts.bhrumom.workers.dev"
    )


def oidc_token(*, audience: str, env: dict[str, str] | os._Environ[str] | None = None) -> str:
    values = os.environ if env is None else env
    request_url = values.get("ACTIONS_ID_TOKEN_REQUEST_URL", "").strip()
    request_token = values.get("ACTIONS_ID_TOKEN_REQUEST_TOKEN", "").strip()
    if not request_url or not request_token:
        raise RuntimeError("GitHub Actions OIDC is required for the isolated account factory")
    parsed = urllib.parse.urlsplit(request_url)
    if parsed.scheme != "https" or parsed.hostname != "token.actions.githubusercontent.com":
        raise RuntimeError("GitHub Actions OIDC endpoint is invalid")
    query = urllib.parse.parse_qs(parsed.query)
    query["audience"] = [audience]
    url = urllib.parse.urlunsplit((parsed.scheme, parsed.netloc, parsed.path, urllib.parse.urlencode(query, doseq=True), ""))
    request = urllib.request.Request(
        url,
        method="GET",
        headers={"Accept": "application/json", "Authorization": f"Bearer {request_token}"},
    )
    try:
        with urllib.request.urlopen(request, timeout=20) as response:
            body = response.read(16 * 1024 + 1)
    except Exception as error:
        raise RuntimeError("GitHub Actions OIDC token request failed") from error
    try:
        value = json.loads(body.decode("utf-8"))
    except Exception as error:
        raise RuntimeError("GitHub Actions OIDC response was invalid") from error
    token = value.get("value") if isinstance(value, dict) else None
    if not isinstance(token, str) or not (24 <= len(token) <= 16 * 1024) or any(ch.isspace() for ch in token):
        raise RuntimeError("GitHub Actions OIDC token was invalid")
    return token


def allocate_factory_lease(*, base_url: str, scenario_id: str, token: str) -> dict:
    payload = json.dumps({"scenarioId": scenario_id}).encode("utf-8")
    request = urllib.request.Request(
        f"{normalize_base_url(base_url)}/v1/leases",
        data=payload,
        method="POST",
        headers={"Accept": "application/json", "Content-Type": "application/json", "Authorization": f"Bearer {token}"},
    )
    try:
        with urllib.request.urlopen(request, timeout=180) as response:
            body = response.read(MAX_SESSION_BYTES + 1)
            if response.status != 201:
                raise RuntimeError("Account Factory lease allocation was rejected")
    except urllib.error.HTTPError as error:
        raise RuntimeError(f"Account Factory lease allocation failed with HTTP {error.code}") from None
    except urllib.error.URLError as error:
        raise RuntimeError("Account Factory lease allocation transport failed") from error
    try:
        value = json.loads(body.decode("utf-8"))
    except Exception as error:
        raise RuntimeError("Account Factory lease response was invalid") from error
    if not isinstance(value, dict) or not isinstance(value.get("leaseId"), str) or not isinstance(value.get("accounts"), list):
        raise RuntimeError("Account Factory lease response was incomplete")
    return value


def lease_session(lease: dict) -> tuple[dict, str, str]:
    accounts = lease.get("accounts")
    if len(accounts) < 1 or not isinstance(accounts[0], dict):
        raise RuntimeError("Account Factory returned no test account")
    account = accounts[0]
    sessions = account.get("sessions")
    if not isinstance(sessions, list) or not sessions or not isinstance(sessions[0], dict):
        raise RuntimeError("Account Factory returned no account session")
    session = sessions[0]
    session_device_id = _credential(session.get("deviceId"))
    if not session_device_id:
        raise RuntimeError("Account Factory session has no device identity")
    user = session.get("user") if isinstance(session.get("user"), dict) else {}
    source = {
        **session,
        "username": account.get("username") or user.get("username"),
        "userId": user.get("id"),
        "user": user,
        "provider": session.get("provider") or "official",
        "ciRunner": False,
        "deviceId": session_device_id,
    }
    return source, lease["leaseId"], session_device_id


def ci_auth_base_url(env: dict[str, str] | os._Environ[str] | None = None) -> str:
    values = os.environ if env is None else env
    return normalize_base_url(
        values.get("FABUSHI_CI_AUTH_BASE_URL")
        or values.get("FABUSHI_API_BASE_URL")
        or "https://mahayana-platform.bhrumom.workers.dev"
    )


def login(
    *,
    base_url: str,
    username: str,
    password: str,
    device_id: str,
    timeout_seconds: int = 20,
) -> dict:
    payload = json.dumps(
        {"username": username, "password": password, "deviceId": device_id}
    ).encode("utf-8")
    request = urllib.request.Request(
        f"{normalize_base_url(base_url)}/api/auth/login",
        data=payload,
        method="POST",
        headers={"Accept": "application/json", "Content-Type": "application/json"},
    )
    try:
        with urllib.request.urlopen(request, timeout=timeout_seconds) as response:
            body = response.read(MAX_SESSION_BYTES + 1)
    except urllib.error.HTTPError as error:
        detail = "request rejected"
        try:
            raw = error.read(4097)
            if len(raw) <= 4096:
                parsed = json.loads(raw.decode("utf-8"))
                if isinstance(parsed, dict):
                    candidate = parsed.get("message")
                    if not isinstance(candidate, str):
                        value = parsed.get("error")
                        candidate = value if isinstance(value, str) else (
                            value.get("message") if isinstance(value, dict) else None
                        )
                    code = parsed.get("code")
                    if not isinstance(code, str):
                        value = parsed.get("error")
                        code = value.get("code") if isinstance(value, dict) else None
                    safe = " ".join(
                        str(part).strip()
                        for part in (code, candidate)
                        if isinstance(part, str) and part.strip()
                    )
                    if safe:
                        detail = safe[:240]
        except Exception:
            pass
        raise RuntimeError(
            f"Fabushi CI login failed with HTTP {error.code}: {detail}"
        ) from None
    except urllib.error.URLError as error:
        raise RuntimeError("Fabushi CI login transport failed") from error
    if len(body) > MAX_SESSION_BYTES:
        raise RuntimeError("Fabushi account response is too large")
    try:
        value = json.loads(body.decode("utf-8"))
    except Exception as error:
        raise RuntimeError("Fabushi CI login returned invalid JSON") from error
    if not isinstance(value, dict):
        raise RuntimeError("Fabushi CI login returned an invalid session")
    return value


def bounded_session(
    source: dict,
    *,
    device_id: str,
    run_id: str,
    run_attempt: str,
    now: int | None = None,
) -> dict:
    now = int(time.time()) if now is None else now
    match = DEVICE_RE.fullmatch(device_id)
    if not match:
        raise ValueError("DEVICE_ID must be a protected Android GitHub Actions device id")
    if not DIGITS_RE.fullmatch(run_id) or not DIGITS_RE.fullmatch(run_attempt):
        raise ValueError("GitHub run identity is invalid")
    if match.group(1) != run_id or match.group(2) != run_attempt:
        raise ValueError("DEVICE_ID must be bound to the current run and attempt")

    access_token = _credential(source.get("accessToken"))
    refresh_token = _credential(source.get("refreshToken"))
    token_type = _credential(source.get("tokenType") or "Bearer")
    source_device_id = _credential(source.get("deviceId"))
    username = _credential(source.get("username") or (source.get("user") or {}).get("username"))
    user_id = _credential(source.get("userId") or (source.get("user") or {}).get("id"))
    nested_user_id = _credential((source.get("user") or {}).get("id"))
    expiry = source.get("accessTokenExpiresAt")

    if not valid_credential(access_token) or not valid_credential(refresh_token):
        raise ValueError("The ordinary Fabushi account session is incomplete")
    if token_type != "Bearer" or source_device_id != device_id:
        raise ValueError("The ordinary Fabushi account identity is inconsistent")
    if not username or not user_id or nested_user_id != user_id:
        raise ValueError("The ordinary Fabushi user identity is inconsistent")
    if source.get("ciRunner") is True or source.get("provider") == "github-actions":
        raise ValueError("Expected an ordinary refreshable Fabushi account session")
    if not isinstance(expiry, int):
        raise ValueError("Fabushi access-token expiry is invalid")
    if expiry <= now + 30 or expiry > now + MAX_LIFETIME_SECONDS:
        raise ValueError("Fabushi access token is not valid for a bounded CI session")

    return {
        "accessToken": access_token,
        "tokenType": "Bearer",
        "accessTokenExpiresAt": expiry,
        "sessionId": f"ci-runner:{run_id}:{run_attempt}",
        "deviceId": device_id,
        "username": username,
        "userId": user_id,
        "user": source.get("user"),
        "provider": "github-actions",
        "ciRunner": True,
    }


def atomic_private_write(path: pathlib.Path, document: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    encoded = (json.dumps(document, indent=2, sort_keys=True) + "\n").encode("utf-8")
    if not (0 < len(encoded) <= MAX_SESSION_BYTES):
        raise ValueError("Bounded application session is too large")
    fd, temporary = tempfile.mkstemp(prefix=f"{path.name}.", dir=path.parent)
    try:
        os.fchmod(fd, stat.S_IRUSR | stat.S_IWUSR)
        with os.fdopen(fd, "wb", closefd=True) as handle:
            handle.write(encoded)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, path)
        os.chmod(path, 0o600)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def _required(name: str) -> str:
    value = os.environ.get(name, "").strip()
    if not value:
        raise RuntimeError(f"{name} is required")
    return value


def main() -> int:
    if os.environ.get("GITHUB_ACTIONS") != "true":
        raise RuntimeError("CI application sessions can be created only in GitHub Actions")
    runner_temp = pathlib.Path(_required("RUNNER_TEMP")).resolve()
    output = pathlib.Path(_required("FABUSHI_CI_ACCOUNT_SESSION_FILE")).resolve()
    if runner_temp not in output.parents:
        raise RuntimeError("FABUSHI_CI_ACCOUNT_SESSION_FILE must live under RUNNER_TEMP")

    device_id = _required("DEVICE_ID")
    run_id = _required("GITHUB_RUN_ID")
    run_attempt = _required("GITHUB_RUN_ATTEMPT")
    factory_url = factory_base_url()
    token = oidc_token(audience=factory_url)
    scenario_id = f"android-{run_id}-{run_attempt}"
    lease = allocate_factory_lease(base_url=factory_url, scenario_id=scenario_id, token=token)
    source, lease_id, session_device_id = lease_session(lease)
    exported = bounded_session(
        source,
        device_id=device_id,
        run_id=run_id,
        run_attempt=run_attempt,
    )
    atomic_private_write(output, exported)
    lease_file = pathlib.Path(_required("FABUSHI_CI_ACCOUNT_LEASE_FILE")).resolve()
    atomic_private_write(lease_file, {"leaseId": lease_id, "factoryUrl": factory_url})
    github_env = os.environ.get("GITHUB_ENV")
    if github_env:
        with open(github_env, "a", encoding="utf-8") as handle:
            handle.write(f"DEVICE_ID={session_device_id}\n")
    print("Prepared bounded refresh-token-free Fabushi Android CI application session.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

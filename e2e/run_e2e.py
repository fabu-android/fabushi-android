#!/usr/bin/env python3
"""Canonical CLI runner for the Fabushi Android 4-Tier Requirement-Driven Opaque-Box E2E Suite."""

from __future__ import annotations

import os
import sys

# MUST precede all package imports so zero __pycache__ / .pyc files pollute e2e/
sys.dont_write_bytecode = True
os.environ["PYTHONDONTWRITEBYTECODE"] = "1"

import argparse
import base64
import hashlib
import json
import signal
import socket
import struct
import time
import urllib.request
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parents[1]
if str(REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(REPO_ROOT))

from e2e.harness.build_lock import E2eWorkspaceManager, safe_chmod
from e2e.harness.context import E2eRunServices, E2eTestContext
from e2e.harness.qr_decoder import synthesize_fake_sha256_qr_matrix
from e2e.harness.registry import (
    ALL_FEATURES,
    E2eTestResult,
    execute_test_spec,
    filter_tests,
    get_all_tests,
    summarize_registry,
    validate_quantitative_thresholds,
)


def _import_all_test_modules() -> None:
    import e2e.tier1.test_t1_f01_f02_m1  # noqa: F401
    import e2e.tier1.test_t1_f03_f07_rust  # noqa: F401
    import e2e.tier1.test_t1_f08_f09_mika  # noqa: F401
    import e2e.tier1.test_t1_f10_f12_app  # noqa: F401
    import e2e.tier1.test_t1_f13_f15_ux_rel  # noqa: F401
    import e2e.tier2.test_t2_f01_f02_m1  # noqa: F401
    import e2e.tier2.test_t2_f03_f07_rust  # noqa: F401
    import e2e.tier2.test_t2_f08_f09_mika  # noqa: F401
    import e2e.tier2.test_t2_f10_f12_app  # noqa: F401
    import e2e.tier2.test_t2_f13_f15_ux_rel  # noqa: F401
    import e2e.tier3.test_t3_pairwise  # noqa: F401
    import e2e.tier4.test_t4_workflows  # noqa: F401


def _run_harness_self_validation(services: E2eRunServices) -> None:
    summary = validate_quantitative_thresholds()
    print(
        f"[validate-harness] Registry thresholds OK: total={summary['total']} "
        f"(T1={summary['by_tier'][1]}, T2={summary['by_tier'][2]}, "
        f"T3={summary['by_tier'][3]}, T4={summary['by_tier'][4]})"
    )

    # 1. ZXing QR round-trip + fake SHA-256 QR rejection
    sample_uri = "ton://transfer/UQB_test_address_v2_1_0?amount=1000000000&text=法布施"
    encoded = services.qr.encode_matrix(sample_uri)
    ok_real, dec_real = services.qr.decode_matrix(encoded)
    assert ok_real and dec_real == sample_uri, f"ZXing self-test failed: {dec_real!r}"
    fake_qr = synthesize_fake_sha256_qr_matrix(sample_uri)
    ok_fake, _ = services.qr.decode_matrix(fake_qr)
    assert not ok_fake, "ZXing self-test failed to reject fake SHA-256 QR matrix"
    print("[validate-harness] ZXing ISO/IEC 18004 QR optical decoder & fake-QR rejection OK")

    # 2. Loopback servers (HTTP + RFC 6455 WS + SSE + TON BOC + WebMCP)
    no_proxy_opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    from e2e.servers.loopback_upstreams import (
        LoopbackMagpieUpstreamHub,
        LoopbackMarketplaceMcpServer,
        LoopbackTelegramServer,
        LoopbackTonRpcServer,
    )

    with (
        LoopbackTelegramServer() as tg,
        LoopbackMagpieUpstreamHub("sk-selftest") as mg,
        LoopbackTonRpcServer() as ton,
        LoopbackMarketplaceMcpServer() as mcp,
    ):
        # HTTP check on Telegram server
        with no_proxy_opener.open(f"{tg.base_url}/api/v1/status", timeout=5) as r:
            st = json.loads(r.read().decode("utf-8"))
            assert st.get("ok") is True

        # RFC 6455 WebSocket handshake + masked Ping -> Pong check
        with socket.create_connection((tg.host, tg.port), timeout=5) as sock:
            ws_key = base64.b64encode(b"0123456789abcdef").decode("ascii")
            req = (
                f"GET /api/v1/ws?token=selftest HTTP/1.1\r\n"
                f"Host: {tg.host}:{tg.port}\r\n"
                f"Upgrade: websocket\r\n"
                f"Connection: Upgrade\r\n"
                f"Sec-WebSocket-Key: {ws_key}\r\n"
                f"Sec-WebSocket-Version: 13\r\n\r\n"
            )
            sock.sendall(req.encode("ascii"))
            resp_hdr = sock.recv(1024).decode("iso-8859-1")
            assert "101 Switching Protocols" in resp_hdr, f"WS upgrade failed: {resp_hdr}"
            payload = b'{"type":"Ping","payload":{"nonce":42}}'
            mask = b"\x11\x22\x33\x44"
            masked = bytes(b ^ mask[i % 4] for i, b in enumerate(payload))
            frame = struct.pack("!BB", 0x81, 0x80 | len(payload)) + mask + masked
            sock.sendall(frame)
            ws_reply = sock.recv(1024)
            assert b"Pong" in ws_reply, f"WS Ping/Pong failed: {ws_reply!r}"

        # Magpie SSE check
        mg_req = urllib.request.Request(
            f"{mg.base_url}/v1/chat/completions",
            data=json.dumps({"model": "gpt-4o-mini", "stream": True, "messages": [{"role": "user", "content": "hi"}]}).encode("utf-8"),
            headers={"Authorization": "Bearer sk-selftest", "Content-Type": "application/json"},
            method="POST",
        )
        with no_proxy_opener.open(mg_req, timeout=5) as r:
            sse_body = r.read().decode("utf-8")
            assert "LIVE_UPSTREAM_OK" in sse_body and "[DONE]" in sse_body

        # TON BOC magic validation check
        valid_boc = base64.b64encode(b"\xb5\xee\x9c\x72" + b"\x01" * 72).decode("ascii")
        ton_req = urllib.request.Request(
            f"{ton.base_url}/api/v2/sendBoc",
            data=json.dumps({"boc": valid_boc}).encode("utf-8"),
            headers={"Content-Type": "application/json"},
            method="POST",
        )
        with no_proxy_opener.open(ton_req, timeout=5) as r:
            ton_out = json.loads(r.read().decode("utf-8"))
            assert ton_out.get("ok") is True and len(ton_out["result"]["hash"]) == 64

        # WebMCP entitlement lock check
        with no_proxy_opener.open(f"{mcp.base_url}/v1/plugins/global-dharma/entitlements/local.prayer-wheel.start", timeout=5) as r:
            ent = json.loads(r.read().decode("utf-8"))
            assert ent["access"]["allowed"] is False

    print("[validate-harness] All 4 loopback servers (HTTP/WS/SSE/TON-BOC/WebMCP) OK")


def _print_list_summary() -> None:
    specs = get_all_tests()
    summary = validate_quantitative_thresholds()
    print(f"Fabushi Android Opaque-Box E2E Suite — {summary['total']} Registered Tests")
    print("=" * 78)
    for tier in (1, 2, 3, 4):
        print(f"  Tier {tier}: {summary['by_tier'][tier]:3d} tests")
    print("-" * 78)
    print("  Feature Coverage (Tier 1 / Tier 2 / Total across Tiers 1-4):")
    for feat in ALL_FEATURES:
        c1 = summary["by_tier_feature"][1][feat]
        c2 = summary["by_tier_feature"][2][feat]
        ct = summary["by_feature_total"][feat]
        print(f"    {feat:<4s} : Tier1={c1:2d}  Tier2={c2:2d}  Total={ct:2d}")
    print("=" * 78)
    for spec in specs:
        feats = ",".join(spec.features)
        needs = ",".join(sorted(spec.needs)) or "none"
        print(f"[{spec.test_id:<10s}] T{spec.tier} ({feats:<16s}) [needs:{needs:<10s}] {spec.title}")


def _parse_args(argv: list[str]) -> argparse.Namespace:
    p = argparse.ArgumentParser(
        description="Fabushi Android 4-Tier Requirement-Driven Opaque-Box E2E Test Runner"
    )
    p.add_argument("--list", action="store_true", help="List all registered tests & verify quantitative counts")
    p.add_argument(
        "--validate-harness",
        "--dry-run",
        dest="validate_harness",
        action="store_true",
        help="Validate registry thresholds, ZXing QR decoder, and all 4 loopback servers without Cargo/Gradle builds",
    )
    p.add_argument("--all", action="store_true", help="Execute all 4 tiers (Tiers 1..4)")
    p.add_argument("--tier", type=int, action="append", choices=[1, 2, 3, 4], help="Filter by tier (repeatable)")
    p.add_argument("--feature", type=str, action="append", help="Filter by feature F1..F15 (repeatable or comma-separated)")
    p.add_argument("--test", type=str, default=None, help="Filter by exact test_id or substring")
    p.add_argument("--json-report", type=str, default=None, help="Write structured JSON report to path")
    p.add_argument(
        "--preserve-release-artifacts",
        action="store_true",
        help="Do not delete dist/ during post-build cleanup (used in M6 release packaging)",
    )
    return p.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    if hasattr(signal, "SIGPIPE"):
        signal.signal(signal.SIGPIPE, signal.SIG_DFL)
    args = _parse_args(argv if argv is not None else sys.argv[1:])
    _import_all_test_modules()

    if args.list:
        _print_list_summary()
        return 0

    # Validate registry quantitative thresholds on every run
    registry_summary = validate_quantitative_thresholds()

    ws = E2eWorkspaceManager(preserve_release_artifacts=args.preserve_release_artifacts)

    def _sig_handler(signum: int, _frame: Any) -> None:
        ws.cleanup()
        sys.exit(128 + signum)

    signal.signal(signal.SIGINT, _sig_handler)
    signal.signal(signal.SIGTERM, _sig_handler)

    try:
        services = E2eRunServices(ws)
        if args.validate_harness:
            _run_harness_self_validation(services)
            return 0

        features: list[str] | None = None
        if args.feature:
            features = []
            for raw in args.feature:
                features.extend(part.strip() for part in raw.split(",") if part.strip())

        tiers = [1, 2, 3, 4] if args.all and not args.tier else args.tier
        if not tiers and not features and not args.test:
            tiers = [1, 2, 3, 4]

        selected = filter_tests(tiers=tiers, features=features, test_filter=args.test)
        if not selected:
            print("No E2E tests matched the given filter.", file=sys.stderr)
            return 2

        required_needs: set[str] = set()
        for spec in selected:
            required_needs.update(spec.needs)

        if required_needs:
            ws.materialize_snapshots(required_needs)

        results: list[E2eTestResult] = []
        run_start = time.monotonic()
        print(f"Running {len(selected)} E2E test(s) (needs={sorted(required_needs) or ['none']})...")
        print("-" * 78)

        for spec in selected:
            ctx = E2eTestContext(services, spec.test_id)
            res = execute_test_spec(spec, ctx)
            results.append(res)
            feats = ",".join(spec.features)
            line = f"[{res.status:<5s}] {res.test_id:<10s} (T{res.tier} {feats:<12s}) {res.duration_ms:7.1f}ms  {res.title}"
            print(line, flush=True)
            if res.status in ("FAIL", "ERROR") and res.message:
                first_line = res.message.splitlines()[0][:220]
                print(f"        -> {first_line}", flush=True)

        total_ms = (time.monotonic() - run_start) * 1000.0
        pass_cnt = sum(1 for r in results if r.status == "PASS")
        fail_cnt = sum(1 for r in results if r.status == "FAIL")
        err_cnt = sum(1 for r in results if r.status == "ERROR")

        print("-" * 78)
        print(
            f"Summary: total={len(results)}  PASS={pass_cnt}  FAIL={fail_cnt}  ERROR={err_cnt}  "
            f"elapsed={total_ms / 1000.0:.2f}s"
        )

        if args.json_report:
            report_path = Path(args.json_report)
            report_path.parent.mkdir(parents=True, exist_ok=True)
            safe_chmod(report_path.parent, 0o700)
            payload = {
                "total_registered": registry_summary["total"],
                "selected_count": len(results),
                "pass_count": pass_cnt,
                "fail_count": fail_cnt,
                "error_count": err_cnt,
                "duration_ms": round(total_ms, 2),
                "results": [r.to_dict() for r in results],
            }
            report_path.write_text(json.dumps(payload, indent=2, ensure_ascii=False), encoding="utf-8")

        if err_cnt > 0:
            return 2
        if fail_cnt > 0:
            return 1
        return 0
    finally:
        services.jvm.close()
        ws.cleanup()


if __name__ == "__main__":
    raise SystemExit(main())

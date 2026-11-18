"""Per-test execution context wiring workspace snapshots, FFI/JNI, QR, JVM probes, and loopback servers."""

from __future__ import annotations

from pathlib import Path
from typing import Any

from e2e.harness.build_lock import REPO_ROOT, E2eWorkspaceManager, ensure_dir
from e2e.harness.host_ffi import HostSession
from e2e.harness.jni_bridge import JniBridgeRunner
from e2e.harness.jvm_contract_probe import JvmBatchProbeClient
from e2e.harness.qr_decoder import ZxingQrVerifier
from e2e.servers.loopback_upstreams import (
    LoopbackMagpieUpstreamHub,
    LoopbackMarketplaceMcpServer,
    LoopbackTelegramServer,
    LoopbackTonRpcServer,
)


class E2eRunServices:
    """Shared per-run harness services lazily initialized inside E2eWorkspaceManager."""

    def __init__(self, ws: E2eWorkspaceManager) -> None:
        self.ws = ws
        self.repo_root = REPO_ROOT
        self.qr = ZxingQrVerifier(
            ws.run_dir / "zxing_probe_classes",
            ws.env(scrub_loopback_proxy=True),
            jvm_tmp_dir=ws.jvm_tmp_dir,
        )
        self.jni = JniBridgeRunner(ws)
        self.jvm = JvmBatchProbeClient(ws)


class E2eTestContext:
    """Isolated per-test context passed into every @e2e_test function."""

    def __init__(self, services: E2eRunServices, test_id: str) -> None:
        self.services = services
        self.ws = services.ws
        self.repo_root = services.repo_root
        self.test_id = test_id
        self.root_dir = services.ws.case_dir(test_id)

    def subdir(self, name: str = "work") -> Path:
        return ensure_dir(self.root_dir / name)

    def env(self, *, scrub_loopback_proxy: bool = True) -> dict[str, str]:
        return self.ws.env(scrub_loopback_proxy=scrub_loopback_proxy)

    def host_session(self, name: str = "host", *, test_mode: bool = False) -> HostSession:
        return HostSession(self.ws.host_lib_so, self.subdir(name), test_mode=test_mode)

    @property
    def qr(self) -> ZxingQrVerifier:
        return self.services.qr

    @property
    def jni(self) -> JniBridgeRunner:
        return self.services.jni

    def jvm_probe(self, probe: str, subdir_name: str = "jvm", **kwargs: Any) -> dict[str, Any]:
        return self.services.jvm.run_probe(probe, self.subdir(subdir_name), **kwargs)

    def telegram_server(self) -> LoopbackTelegramServer:
        return LoopbackTelegramServer()

    def magpie_hub(self, expected_api_key: str = "sk-live-e2e-key") -> LoopbackMagpieUpstreamHub:
        return LoopbackMagpieUpstreamHub(expected_api_key=expected_api_key)

    def ton_rpc(self) -> LoopbackTonRpcServer:
        return LoopbackTonRpcServer()

    def mcp_server(self) -> LoopbackMarketplaceMcpServer:
        return LoopbackMarketplaceMcpServer()

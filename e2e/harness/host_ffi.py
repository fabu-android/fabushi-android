"""Safe ctypes C-FFI client, SQLite WAL inspector, and ELF/.dynsym analyzer for libmahayana_app_host.so."""

from __future__ import annotations

import ctypes
import json
import os
import sqlite3
import struct
import subprocess
from pathlib import Path
from typing import Any


class NativeHostLibrary:
    """Singleton loader binding libmahayana_app_host.so C ABI with strict c_void_p + free_string ownership."""

    _instances: dict[str, "NativeHostLibrary"] = {}

    def __init__(self, so_path: Path) -> None:
        if not so_path.is_file():
            raise FileNotFoundError(f"Native host library not found at {so_path}")
        self.so_path = so_path
        # Scrub proxy vars in-process before Rust HTTP/WS clients initialize
        for key in ("HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "http_proxy", "https_proxy", "all_proxy"):
            os.environ.pop(key, None)
        os.environ["NO_PROXY"] = "127.0.0.1,localhost,::1,[::1]"
        os.environ["no_proxy"] = "127.0.0.1,localhost,::1,[::1]"

        lib = ctypes.CDLL(str(so_path))

        lib.mahayana_app_host_create.argtypes = [ctypes.c_char_p]
        lib.mahayana_app_host_create.restype = ctypes.c_int64

        lib.mahayana_app_host_create_test.argtypes = [ctypes.c_char_p]
        lib.mahayana_app_host_create_test.restype = ctypes.c_int64

        # CRITICAL: restype MUST be c_void_p so ctypes does not convert & lose the raw heap pointer!
        lib.mahayana_app_host_dispatch_with_handle.argtypes = [ctypes.c_int64, ctypes.c_char_p]
        lib.mahayana_app_host_dispatch_with_handle.restype = ctypes.c_void_p

        lib.mahayana_app_host_destroy.argtypes = [ctypes.c_int64]
        lib.mahayana_app_host_destroy.restype = None

        lib.mahayana_app_host_free_string.argtypes = [ctypes.c_void_p]
        lib.mahayana_app_host_free_string.restype = None

        self._lib = lib

    @classmethod
    def get(cls, so_path: Path) -> "NativeHostLibrary":
        key = str(so_path.resolve())
        if key not in cls._instances:
            cls._instances[key] = cls(so_path)
        return cls._instances[key]

    def create(self, app_data_dir: Path, *, test_mode: bool = False) -> int:
        app_data_dir.mkdir(parents=True, exist_ok=True)
        raw_dir = str(app_data_dir).encode("utf-8")
        if test_mode:
            return int(self._lib.mahayana_app_host_create_test(raw_dir))
        return int(self._lib.mahayana_app_host_create(raw_dir))

    def dispatch_raw(self, handle: int, raw_request_bytes: bytes | None) -> str:
        ptr = self._lib.mahayana_app_host_dispatch_with_handle(
            ctypes.c_int64(handle),
            raw_request_bytes,
        )
        if not ptr:
            raise AssertionError("mahayana_app_host_dispatch_with_handle returned NULL pointer")
        try:
            return ctypes.string_at(ptr).decode("utf-8", errors="strict")
        finally:
            self._lib.mahayana_app_host_free_string(ctypes.c_void_p(ptr))

    def destroy(self, handle: int) -> None:
        self._lib.mahayana_app_host_destroy(ctypes.c_int64(handle))


class HostSession:
    """Context-managed opaque-box session over libmahayana_app_host.so."""

    def __init__(self, so_path: Path, app_data_dir: Path, *, test_mode: bool = False) -> None:
        self.lib = NativeHostLibrary.get(so_path)
        self.app_data_dir = app_data_dir
        self.test_mode = test_mode
        self.handle = self.lib.create(app_data_dir, test_mode=test_mode)
        if self.handle <= 0:
            raise AssertionError(f"Failed to create MahayanaAndroidHost handle (got {self.handle}) for {app_data_dir}")
        self._closed = False

    def __enter__(self) -> "HostSession":
        return self

    def __exit__(self, exc_type: Any, exc: Any, tb: Any) -> None:
        self.close()

    def close(self) -> None:
        if not self._closed and self.handle > 0:
            self._closed = True
            self.lib.destroy(self.handle)

    def dispatch_raw_str(self, request_json: str) -> dict[str, Any]:
        out = self.lib.dispatch_raw(self.handle, request_json.encode("utf-8"))
        try:
            parsed = json.loads(out)
        except json.JSONDecodeError as exc:
            raise AssertionError(f"Native host returned invalid JSON: {out!r}") from exc
        if not isinstance(parsed, dict):
            raise AssertionError(f"Native host response must be JSON object, got {parsed!r}")
        return parsed

    def call(self, method: str, params: dict[str, Any] | None = None) -> dict[str, Any]:
        req = json.dumps({"method": method, "params": params or {}}, ensure_ascii=False)
        return self.dispatch_raw_str(req)

    def call_any(self, methods: list[str], params: dict[str, Any] | None = None) -> dict[str, Any]:
        """Try canonical method aliases in order, returning the first supported response."""
        last_resp: dict[str, Any] | None = None
        for m in methods:
            resp = self.call(m, params)
            last_resp = resp
            err = resp.get("error")
            err_str = json.dumps(err) if isinstance(err, dict) else str(err or "")
            if resp.get("ok") is True or "Unsupported MahayanaHost method" not in err_str:
                return resp
        assert last_resp is not None
        return last_resp

    def call_ok(self, method: str, params: dict[str, Any] | None = None) -> Any:
        resp = self.call(method, params)
        return assert_rpc_ok(resp, context=method)

    def call_any_ok(self, methods: list[str], params: dict[str, Any] | None = None) -> Any:
        resp = self.call_any(methods, params)
        return assert_rpc_ok(resp, context="/".join(methods))

    @property
    def sqlite_files(self) -> list[Path]:
        return sorted(self.app_data_dir.rglob("*.sqlite*")) + sorted(self.app_data_dir.rglob("*.db*"))

    def sqlite_journal_mode(self) -> str:
        db_path = self.app_data_dir / "runtime.sqlite3"
        if not db_path.is_file():
            candidates = [p for p in self.sqlite_files if not p.name.endswith(("-wal", "-shm"))]
            if not candidates:
                raise AssertionError(f"No SQLite database created under {self.app_data_dir}")
            db_path = candidates[0]
        conn = sqlite3.connect(str(db_path))
        try:
            row = conn.execute("PRAGMA journal_mode;").fetchone()
            return str(row[0]).lower() if row else ""
        finally:
            conn.close()

    def raw_storage_contains(self, needle: str) -> bool:
        raw_needle = needle.encode("utf-8")
        for p in self.app_data_dir.rglob("*"):
            if p.is_file():
                data = p.read_bytes()
                if raw_needle in data:
                    return True
        return False


def assert_rpc_ok(resp: dict[str, Any], *, context: str = "") -> Any:
    if resp.get("ok") is not True:
        raise AssertionError(
            f"Expected RPC {context or '<call>'} to return ok=true, got ok={resp.get('ok')!r}, error={resp.get('error')!r}"
        )
    return resp.get("result")


def assert_structured_error(
    resp: dict[str, Any],
    *,
    expected_codes: tuple[str, ...] = (),
    context: str = "",
) -> dict[str, Any]:
    """Enforce PROJECT.md:43 structured error envelope: {'ok': false, 'error': {'code': '...', 'message': '...'}}."""
    if resp.get("ok") is not False:
        raise AssertionError(
            f"Expected RPC {context or '<call>'} to fail closed with ok=false, but got ok={resp.get('ok')!r}, result={str(resp.get('result'))[:240]}"
        )
    err = resp.get("error")
    if not isinstance(err, dict):
        raise AssertionError(
            f"RPC {context or '<call>'} returned legacy string error {err!r} instead of PROJECT.md §1 structured "
            "{'code': '...', 'message': '...'} error object"
        )
    code = str(err.get("code") or "").strip()
    message = str(err.get("message") or "").strip()
    if not code or not message:
        raise AssertionError(
            f"RPC {context or '<call>'} structured error missing non-empty 'code' or 'message': {err!r}"
        )
    if expected_codes and not any(exp.lower() in code.lower() for exp in expected_codes):
        raise AssertionError(
            f"RPC {context or '<call>'} error code {code!r} did not match any expected {expected_codes}"
        )
    return err


def inspect_elf64_alignment_and_machine(so_path: Path) -> dict[str, Any]:
    data = so_path.read_bytes()
    if len(data) < 64 or data[:4] != b"\x7fELF":
        raise AssertionError(f"{so_path} is not a valid ELF binary")
    ei_class, ei_data = data[4], data[5]
    e_type, e_machine = struct.unpack_from("<HH", data, 16)
    e_phoff = struct.unpack_from("<Q", data, 32)[0]
    e_phentsize, e_phnum = struct.unpack_from("<HH", data, 54)
    pt_loads: list[int] = []
    for idx in range(e_phnum):
        off = e_phoff + idx * e_phentsize
        p_type = struct.unpack_from("<I", data, off)[0]
        if p_type == 1:  # PT_LOAD
            p_align = struct.unpack_from("<Q", data, off + 48)[0]
            pt_loads.append(p_align)
    return {
        "ei_class": ei_class,
        "ei_data": ei_data,
        "e_type": e_type,
        "e_machine": e_machine,
        "pt_load_aligns": pt_loads,
    }


def list_defined_dynsyms(so_path: Path, env: dict[str, str]) -> set[str]:
    readelf_candidates = [
        "/home/antigravity/android-sdk/ndk/27.2.12479018/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-readelf",
        "llvm-readelf",
        "readelf",
    ]
    tool = next((c for c in readelf_candidates if Path(c).is_file() or "/" not in c), "readelf")
    proc = subprocess.run(
        [tool, "--dyn-syms", "--wide", str(so_path)],
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        check=False,
    )
    if proc.returncode != 0:
        raise AssertionError(f"llvm-readelf --dyn-syms failed on {so_path}: {proc.stderr}")
    defined: set[str] = set()
    for line in proc.stdout.splitlines():
        parts = line.split()
        if len(parts) >= 8 and "FUNC" in parts and "GLOBAL" in parts and "UND" not in parts:
            sym = parts[-1].split("@")[0]
            defined.add(sym)
    return defined

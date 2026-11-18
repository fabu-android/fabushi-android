"""ISO/IEC 18004 QR matrix renderer & ZXing optical decoder wrapper."""

from __future__ import annotations

import hashlib
import json
import subprocess
from pathlib import Path
from typing import Any

from e2e.harness.build_lock import GRADLE_CACHE_DIR, ensure_dir

_CANONICAL_ZXING_JAR = Path(
    "/home/antigravity/.gradle/caches/modules-2/files-2.1/"
    "com.google.zxing/core/3.5.3/ca1349214a356cd7958651b2d5a0e1f3811a9c4b/core-3.5.3.jar"
)
_PROBE_JAVA = Path(__file__).resolve().parent / "ZxingQrProbe.java"


def find_zxing_core_jar() -> Path:
    if _CANONICAL_ZXING_JAR.is_file():
        return _CANONICAL_ZXING_JAR
    matches = sorted(GRADLE_CACHE_DIR.glob("**/com.google.zxing/core/*/core-*.jar"))
    if not matches:
        matches = sorted(GRADLE_CACHE_DIR.glob("**/zxing/core/*/*/core-*.jar"))
    if not matches:
        raise FileNotFoundError("ZXing core-3.5.3.jar not found in /home/antigravity/.gradle/caches")
    return matches[-1]


class ZxingQrVerifier:
    """Compiles ZxingQrProbe.java once into scratch and decodes/encodes QR module matrices."""

    def __init__(self, classes_dir: Path, env: dict[str, str], jvm_tmp_dir: Path | None = None) -> None:
        self.classes_dir = ensure_dir(classes_dir)
        self.env = env
        self.jvm_tmp_dir = jvm_tmp_dir or ensure_dir(Path("/home/antigravity/.cache/teamwork_tmp/e2e_zxing_tmp"))
        self.zxing_jar = find_zxing_core_jar()
        self._compiled = False

    def ensure_compiled(self) -> None:
        if self._compiled and (self.classes_dir / "ZxingQrProbe.class").is_file():
            return
        proc = subprocess.run(
            [
                "javac",
                f"-J-Djava.io.tmpdir={self.jvm_tmp_dir}",
                "--release",
                "17",
                "-cp",
                str(self.zxing_jar),
                "-d",
                str(self.classes_dir),
                str(_PROBE_JAVA),
            ],
            env=self.env,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            check=False,
        )
        if proc.returncode != 0:
            raise RuntimeError(f"Failed to compile ZxingQrProbe.java:\n{proc.stdout}")
        self._compiled = True

    def encode_matrix(self, text: str) -> dict[str, Any]:
        self.ensure_compiled()
        cp = f"{self.classes_dir}:{self.zxing_jar}"
        proc = subprocess.run(
            ["java", "-Xmx192m", f"-Djava.io.tmpdir={self.jvm_tmp_dir}", "-cp", cp, "ZxingQrProbe", "encode"],
            input=text,
            env=self.env,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=False,
        )
        if proc.returncode != 0:
            raise RuntimeError(f"ZxingQrProbe encode failed: {proc.stderr}")
        return json.loads(proc.stdout)

    def decode_matrix(self, matrix: dict[str, Any]) -> tuple[bool, str]:
        """Return (True, decoded_text) on valid ISO-18004 QR, or (False, error_reason) on invalid/fake QR."""
        self.ensure_compiled()
        cp = f"{self.classes_dir}:{self.zxing_jar}"
        payload = json.dumps({"size": matrix.get("size", 0), "modules": matrix.get("modules", [])})
        proc = subprocess.run(
            ["java", "-Xmx192m", f"-Djava.io.tmpdir={self.jvm_tmp_dir}", "-cp", cp, "ZxingQrProbe", "decode"],
            input=payload,
            env=self.env,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=False,
        )
        if proc.returncode == 0:
            return True, proc.stdout
        return False, proc.stderr.strip() or f"ZXing exit {proc.returncode}"


def synthesize_fake_sha256_qr_matrix(payload: str, size: int = 29) -> dict[str, Any]:
    """Reproduce TonWalletScreen.kt:475 fake SHA-256 bit grid with 3 finder-like squares to prove ZXing rejects it."""
    digest = hashlib.sha256(payload.encode("utf-8")).digest()
    modules: list[bool] = [False] * (size * size)

    def draw_finder(r0: int, c0: int) -> None:
        for dr in range(7):
            for dc in range(7):
                border = dr in (0, 6) or dc in (0, 6)
                center = 2 <= dr <= 4 and 2 <= dc <= 4
                modules[(r0 + dr) * size + (c0 + dc)] = border or center

    draw_finder(0, 0)
    draw_finder(0, size - 7)
    draw_finder(size - 7, 0)

    bit_idx = 0
    for r in range(size):
        for c in range(size):
            in_tl = r < 8 and c < 8
            in_tr = r < 8 and c >= size - 8
            in_bl = r >= size - 8 and c < 8
            if in_tl or in_tr or in_bl:
                continue
            byte_val = digest[(bit_idx // 8) % len(digest)]
            modules[r * size + c] = bool((byte_val >> (bit_idx % 8)) & 1)
            bit_idx += 1

    return {"size": size, "modules": modules}

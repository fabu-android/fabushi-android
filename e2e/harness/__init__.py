"""Opaque-box E2E harness utilities (registry, flock snapshotter, JNI/FFI, QR, JVM probes)."""

import os
import sys

sys.dont_write_bytecode = True
os.environ.setdefault("PYTHONDONTWRITEBYTECODE", "1")

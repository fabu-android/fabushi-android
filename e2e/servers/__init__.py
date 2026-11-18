"""Ephemeral 127.0.0.1:0 loopback servers for opaque-box E2E verification."""

import os
import sys

sys.dont_write_bytecode = True
os.environ.setdefault("PYTHONDONTWRITEBYTECODE", "1")

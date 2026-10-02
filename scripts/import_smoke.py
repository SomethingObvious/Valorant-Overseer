# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""Checks every package the backend needs imports in the installed runtime."""

import importlib
import sys

REQUIRED = [
    "requests",
    "dotenv",
    "urllib3",
    "websockets",
    "websockets.sync.client",
]

failed = []
for mod in REQUIRED:
    try:
        importlib.import_module(mod)
    except Exception as e:
        failed.append(f"{mod}: {type(e).__name__}: {e}")

if failed:
    print("IMPORT SMOKE FAILED:", file=sys.stderr)
    for line in failed:
        print("  " + line, file=sys.stderr)
    sys.exit(1)

print(f"import smoke ok ({len(REQUIRED)} modules)")

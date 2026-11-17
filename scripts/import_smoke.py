# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""Checks every package the backend needs imports in the installed runtime."""

import importlib
import sys

NEEDED = ("requests", "urllib3", "websockets", "websockets.sync.client")


def main() -> int:
    """Import each package and return 1 when any of them fails."""
    broken = []
    for name in NEEDED:
        try:
            importlib.import_module(name)
        except Exception as e:
            broken.append(f"{name}: {type(e).__name__}: {e}")
    if broken:
        print("These packages won't import:", *broken, sep="\n  ", file=sys.stderr)
        return 1
    print(f"all {len(NEEDED)} packages the backend needs import")
    return 0


if __name__ == "__main__":
    sys.exit(main())

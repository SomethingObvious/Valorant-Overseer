# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""The JSON stores under backend/data, each a dict that is read once and rewritten whole."""

from __future__ import annotations

import contextlib
import json
import os
import tempfile
import time
from pathlib import Path
from typing import Any

DATA_DIR = str(Path(__file__).resolve().parent.parent / "data")


def _warn(msg: str) -> None:
    # Imported here because run.py imports this package for load_env and
    # treats overseerlog as optional, so a missing logger skips the warning.
    try:
        import overseerlog
    except ImportError:
        return
    overseerlog.get_logger("backend").warning(msg)


def data_path(name: str) -> str:
    """Path to a file in backend/data. The directory may not exist yet."""
    return str(Path(DATA_DIR) / name)


def read_json(path: str, default: Any) -> Any:
    """Return the parsed file, or `default` if it is missing or unreadable."""
    # A corrupt store is not worth refusing to launch over, so the app starts
    # from `default`. The file goes aside first, because the next save would
    # write `default` over the only copy of it.
    try:
        with Path(path).open(encoding="utf-8") as fh:
            return json.load(fh)
    except FileNotFoundError:
        return default
    except (OSError, ValueError) as e:
        kept = f"{path}.unreadable-{time.strftime('%Y%m%d-%H%M%S')}"
        try:
            # rename rather than replace, because on Windows rename refuses to
            # write over a file that is already there.
            Path(path).rename(kept)
        except OSError as move_error:
            _warn(f"unreadable store {path} ({e!r}) could not be moved aside: {move_error!r}")
        else:
            _warn(f"unreadable store {path} moved to {kept}: {e!r}")
        return default


def write_atomic(path: str, data: Any, *, prefix: str, ensure_ascii: bool = False) -> bool:
    """Write `data` as JSON to `path` through a temp file, and return False if that fails."""
    # The temp file sits in the same directory so the replace is a rename,
    # which is what makes it atomic. Every caller is a best-effort save, so
    # this reports failure instead of raising.
    try:
        Path(path).parent.mkdir(parents=True, exist_ok=True)
        fd, tmp = tempfile.mkstemp(dir=str(Path(path).parent), prefix=prefix, suffix=".tmp")
        try:
            with os.fdopen(fd, "w", encoding="utf-8") as fh:
                json.dump(data, fh, ensure_ascii=ensure_ascii, separators=(",", ":"))
            Path(tmp).replace(path)
            return True
        finally:
            # Only still there if something above failed.
            if Path(tmp).exists():
                with contextlib.suppress(OSError):
                    Path(tmp).unlink()
    except Exception as e:
        # A failed save must not take down the thread that asked for it,
        # whatever the reason.
        _warn(f"could not save {path}: {e!r}")
        return False

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

_DATA_DIR = Path(__file__).resolve().parent.parent / "data"


def _warn(msg: str) -> None:
    try:
        import overseerlog
    except ImportError:
        return
    overseerlog.get_logger("backend").warning(msg)


def data_path(name: str) -> str:
    """Path to a file in backend/data. The directory may not exist yet."""
    return str(_DATA_DIR / name)


def read_json(path: str, default: Any) -> Any:
    """Return the parsed file, or `default` if it is missing or unreadable."""
    # A corrupt store is not worth refusing to launch over, so the app starts
    # from `default`. The file goes aside first, because the next save would
    # write `default` over the only copy of it.
    for attempt in range(5):
        try:
            with Path(path).open(encoding="utf-8") as fh:
                return json.load(fh)
        except FileNotFoundError:
            return default
        except PermissionError as e:
            # Antivirus or a sync tool holding the file is not a corrupt
            # store, and reading it as empty would get it saved over. That
            # rarely lasts, and when it does the error stops whoever asked.
            if attempt == 4:
                raise
            _warn(f"store {path} is locked, trying again: {e!r}")
            time.sleep(0.2 * (attempt + 1))
        except (OSError, ValueError) as e:
            return _set_aside(path, default, e)
    return default


def _set_aside(path: str, default: Any, e: Exception) -> Any:
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


def write_atomic(path: str, data: Any, *, prefix: str, durable: bool = True) -> bool:
    """Write `data` as JSON to `path` through a temp file, and return False if that fails."""
    try:
        Path(path).parent.mkdir(parents=True, exist_ok=True)
        fd, tmp = tempfile.mkstemp(dir=str(Path(path).parent), prefix=prefix, suffix=".tmp")
        try:
            with os.fdopen(fd, "w", encoding="utf-8") as fh:
                json.dump(data, fh, ensure_ascii=False, separators=(",", ":"))
                # NTFS keeps the rename through a power cut but not the data,
                # so an unsynced file can come back empty.
                if durable:
                    fh.flush()
                    os.fsync(fh.fileno())
            Path(tmp).replace(path)
            return True
        finally:
            if Path(tmp).exists():
                with contextlib.suppress(OSError):
                    Path(tmp).unlink()
    except Exception as e:
        # A failed save must not take down the thread that asked for it,
        # whatever the reason.
        _warn(f"could not save {path}: {e!r}")
        return False

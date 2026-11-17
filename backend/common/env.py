# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""Reads backend/.env without python-dotenv."""

from __future__ import annotations

import math
import os
import sys
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from pathlib import Path


def env_number(
    name: str, default: float, *, low: float = -math.inf, high: float = math.inf
) -> float:
    """Return the number in the environment variable `name`, or `default`."""
    raw = os.environ.get(name, "").strip()
    if not raw:
        return default
    try:
        value = float(raw)
    except ValueError:
        value = math.nan
    if low <= value <= high:
        return value
    print(
        f"[env] {name} is {raw!r}, which isn't a number it can be, so {default:g} is used",
        file=sys.stderr,
        flush=True,
    )
    return default


def load_env(*paths: Path) -> None:
    """Populate os.environ from KEY=VALUE files, first file wins."""
    for path in paths:
        if not path.exists():
            continue
        # utf-8-sig because Notepad writes a BOM and the first key would
        # otherwise be named "﻿RIOT_REGION".
        for raw in path.read_text(encoding="utf-8-sig", errors="replace").splitlines():
            line = raw.strip()
            if not line or line.startswith("#") or "=" not in line:
                continue
            key, _, value = line.partition("=")
            # setdefault, so a variable already set by start.ps1 or the user's
            # shell outranks the file. That is how the launcher passes WS_PORT
            # down to a child that reads the same .env.
            os.environ.setdefault(key.strip(), value.strip().strip('"').strip("'"))

# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""Reads backend/.env without python-dotenv.

The launcher calls this before validate_runtime() has checked that
site-packages is intact, so it cannot import anything from there.
"""

from __future__ import annotations

import os
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from pathlib import Path


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

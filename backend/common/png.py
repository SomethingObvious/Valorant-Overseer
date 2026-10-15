# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""Pictures from valorant-api, saved whole or not at all."""

from __future__ import annotations

from typing import TYPE_CHECKING

import requests

if TYPE_CHECKING:
    from pathlib import Path


def save_png(url: str, path: Path, timeout: float = 15) -> None:
    """Download a PNG to `path` through a `.part` file, so nobody reads half of one.

    Raises `requests.RequestException` when it can't be fetched, `ValueError`
    when what came back isn't a PNG, and `OSError` when it can't be saved.
    """
    answer = requests.get(url, timeout=timeout)
    answer.raise_for_status()
    if not answer.content.startswith(b"\x89PNG"):
        msg = f"{url} is not a PNG"
        raise ValueError(msg)
    path.parent.mkdir(parents=True, exist_ok=True)
    part = path.with_suffix(".part")
    part.write_bytes(answer.content)
    part.replace(path)

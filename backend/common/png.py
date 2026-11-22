# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""Pictures from valorant-api, saved whole or not at all."""

from __future__ import annotations

from email.utils import formatdate
from typing import TYPE_CHECKING

import requests

if TYPE_CHECKING:
    from pathlib import Path


def save_png(url: str, path: Path, timeout: float = 15, since: float | None = None) -> bool:
    """Download a PNG to `path` through a `.part` file, so nobody reads half of one.

    Given `since`, it's only fetched if the server changed it after then, and
    False means it hadn't.
    """
    headers = {"If-Modified-Since": formatdate(since, usegmt=True)} if since else {}
    answer = requests.get(url, timeout=timeout, headers=headers)
    if answer.status_code == requests.codes.not_modified:
        return False
    answer.raise_for_status()
    if not answer.content.startswith(b"\x89PNG"):
        msg = f"{url} is not a PNG"
        raise ValueError(msg)
    path.parent.mkdir(parents=True, exist_ok=True)
    part = path.with_suffix(".part")
    part.write_bytes(answer.content)
    part.replace(path)
    return True

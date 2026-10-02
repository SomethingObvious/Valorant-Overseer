# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""The `[tag] message` console line the backend modules print."""

from __future__ import annotations

import os
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    import logging
    from collections.abc import Callable


def console_logger(
    tag: str, *, quiet_aware: bool = True, also: logging.Logger | None = None
) -> Callable[[str], None]:
    """Return a `log(msg)` that prints `[tag] msg` and also sends it to `also` if given."""

    # quiet_aware=False is for a message that has to reach the console even
    # under OVERSEER_QUIET, like the bridge failing to start.
    def log(msg: str) -> None:
        if quiet_aware and os.getenv("OVERSEER_QUIET"):
            return
        print(f"[{tag}] {msg}", flush=True)
        if also is not None:
            also.info("%s", msg)

    return log

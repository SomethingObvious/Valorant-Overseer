# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""Wakes the board loop early, when something on the board has changed.

The loop builds a board every few seconds, which left a player's K/D, or the
start of a match, waiting up to that long after the backend knew it.
"""

from __future__ import annotations

import threading

_WOKEN = threading.Event()
_STALE = {"board": False}


def soon() -> None:
    """Ask for a new board now, past the cache of the last one."""
    _STALE["board"] = True
    _WOKEN.set()


def wait(timeout: float) -> bool:
    """Wait up to `timeout` seconds for `soon`, and say whether it came."""
    return _WOKEN.wait(timeout)


def clear() -> None:
    """Forget the asks so far, once the board they asked for is being built."""
    _WOKEN.clear()


def take_stale() -> bool:
    """Whether a new board was asked for since the last one was built."""
    stale = _STALE["board"]
    _STALE["board"] = False
    return stale

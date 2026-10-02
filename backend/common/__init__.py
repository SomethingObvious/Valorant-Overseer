# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""Helpers that more than one backend module needs.

Something moves in here once two modules already carry their own copy of it,
not on the theory that it might be shared one day.
"""

from __future__ import annotations

from common.env import load_env
from common.jsonstore import DATA_DIR, data_path, read_json, write_atomic
from common.log import console_logger

__all__ = [
    "DATA_DIR",
    "console_logger",
    "data_path",
    "load_env",
    "read_json",
    "write_atomic",
]

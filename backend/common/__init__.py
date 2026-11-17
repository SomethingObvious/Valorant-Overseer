# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""Helpers that more than one backend module needs."""

from __future__ import annotations

from common.env import env_number, load_env
from common.jsonstore import data_path, read_json, write_atomic
from common.log import console_logger

__all__ = [
    "console_logger",
    "data_path",
    "env_number",
    "load_env",
    "read_json",
    "write_atomic",
]

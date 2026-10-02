# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""Logging to .overseer, with tokens and keys redacted before anything is written."""

from __future__ import annotations

import logging
import re
import time
from logging.handlers import RotatingFileHandler
from pathlib import Path

from common.check import check

OVERSEER_DIR = Path(__file__).resolve().parent.parent / ".overseer"

MAX_BYTES = 2 * 1024 * 1024
BACKUP_COUNT = 5

_REDACTIONS: list[tuple[re.Pattern[str], str]] = [
    (re.compile(r"([?&](?:s|t|token|key)=)[^&\s\"']+"), r"\1[REDACTED]"),
    (re.compile(r"\b([st]=)[A-Za-z0-9._~-]{8,}"), r"\1[REDACTED]"),
    (
        re.compile(
            # [A-Za-z_]* in front catches "accessToken", which is the field
            # Riot actually sends and is enough to act as this account.
            r'("[A-Za-z_]*(?:token|password|apiKey|api_key|key|secret|authorization)"'
            r'\s*:\s*")[^"]+(")',
            re.IGNORECASE,
        ),
        r"\1[REDACTED]\2",
    ),
    # X-Riot-Entitlements-JWT travels with the access token and is half of what
    # authenticates this account. Its header name has none of the words above,
    # so it needs its own rule.
    (
        re.compile(r"(X-Riot-Entitlements-JWT\s*[:=]\s*)\S+", re.IGNORECASE),
        r"\1[REDACTED]",
    ),
    # Anything JWT-shaped, whatever it is called: three base64url segments, the
    # first starting with the "eyJ" every JSON header encodes to. This catches
    # the field nobody thought of.
    (
        re.compile(r"\beyJ[A-Za-z0-9_-]{6,}\.[A-Za-z0-9_-]{6,}\.[A-Za-z0-9_-]+"),
        "[REDACTED-JWT]",
    ),
    (re.compile(r"\b(Basic|Bearer)\s+[A-Za-z0-9+/=_\-.]{8,}"), r"\1 [REDACTED]"),
    (
        re.compile(
            r"\b(password|token|secret|api_key|apikey|authorization)\s*[=:]\s*\S+", re.IGNORECASE
        ),
        r"\1=[REDACTED]",
    ),
    (
        re.compile(r"\b[A-Za-z0-9_\-]{6,}\.[A-Za-z0-9_\-]{6,}:[A-Za-z0-9_\-]{16,}\b"),
        "[REDACTED-ABLY-KEY]",
    ),
    (re.compile(r"\b([0-9a-fA-F]{8})[0-9a-fA-F\-]{24,}\b"), r"\1…[REDACTED]"),
    (re.compile(r"\b(\d{6})\d{11,}\b"), r"\1…[REDACTED]"),
]


def redact(text: str) -> str:
    """Return `text` with tokens, keys and JWTs replaced by placeholders."""
    for pat, repl in _REDACTIONS:
        text = pat.sub(repl, text)
    return text


class _UtcFormatter(logging.Formatter):
    @staticmethod
    def converter(timestamp: float | None) -> time.struct_time:
        return time.gmtime(timestamp)

    def format(self, record: logging.LogRecord) -> str:
        return redact(super().format(record))


def get_logger(component: str, filename: str | None = None) -> logging.Logger:
    """Return the `overseer.<component>` logger.

    It writes to .overseer/<filename or component>.log.
    """
    name = f"overseer.{component}"
    logger = logging.getLogger(name)
    if logger.handlers:
        return logger
    logger.setLevel(logging.INFO)
    logger.propagate = False
    try:
        OVERSEER_DIR.mkdir(exist_ok=True)
        handler = RotatingFileHandler(
            OVERSEER_DIR / f"{filename or component}.log",
            maxBytes=MAX_BYTES,
            backupCount=BACKUP_COUNT,
            encoding="utf-8",
        )
        handler.setFormatter(
            _UtcFormatter(
                fmt=f"%(asctime)s.%(msecs)03dZ [{component}] %(levelname)s %(message)s",
                datefmt="%Y-%m-%dT%H:%M:%S",
            )
        )
        logger.addHandler(handler)
    except OSError:
        logger.addHandler(logging.NullHandler())
    return logger


if __name__ == "__main__":
    # The entitlements header and accessToken are the two a keyword rule
    # misses, so both are here.
    _MUST_REDACT = (
        "Authorization: Bearer eyJhbGciOiJSUzI1NiJ9.eyJzdWIiOiIxIn0.sig",
        "X-Riot-Entitlements-JWT: eyJraWQiOiJzMSJ9.abcdefghijkl.sig",
        '{"accessToken": "eyJhbGciOiJSUzI1NiJ9.payloadpart.signature"}',
        '{"idToken":"eyJhbGciOiJIUzI1NiJ9.payloadpart.signature"}',
        "lockfile password=aBcD1234EfGh5678",
        "?s=Xk3mQp7ZrT9vLb2NcWy4Ee8Ff1Gg6Hh0Ii5Jj",
        "subject 5ca07a5c-0ff1-4c0d-9e00-000000000001",
    )
    for _sample in _MUST_REDACT:
        _out = redact(_sample)
        check("REDACT" in _out, f"not redacted: {_sample!r} -> {_out!r}")
        check("eyJ" not in _out or "REDACTED-JWT" in _out, f"jwt survived: {_out!r}")

    # A log that redacts everything is no use either.
    check(redact("backend started on port 5000") == "backend started on port 5000")
    check("Ascent" in redact("map Ascent, round 12"))

    print(f"overseerlog self-check OK ({len(_MUST_REDACT)} secret shapes redacted)")

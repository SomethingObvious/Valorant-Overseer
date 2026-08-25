from __future__ import annotations

import logging
import re
import time
from logging.handlers import RotatingFileHandler
from pathlib import Path

OVERSEER_DIR = Path(__file__).resolve().parent.parent / ".overseer"

MAX_BYTES = 2 * 1024 * 1024
BACKUP_COUNT = 5

_REDACTIONS: list[tuple[re.Pattern[str], str]] = [
    (re.compile(r"([?&](?:s|t|token|key)=)[^&\s\"']+"), r"\1[REDACTED]"),
    (re.compile(r"\b([st]=)[A-Za-z0-9._~-]{8,}"), r"\1[REDACTED]"),
    (
        re.compile(
            # The leading [A-Za-z_]* is the fix: the key had to be exactly
            # "token", so {"accessToken": "..."} -- which is the field Riot
            # actually sends, and the one that lets somebody act as this
            # account -- went into the log intact.
            r'("[A-Za-z_]*(?:token|password|apiKey|api_key|key|secret|authorization)"'
            r'\s*:\s*")[^"]+(")',
            re.IGNORECASE,
        ),
        r"\1[REDACTED]\2",
    ),
    # X-Riot-Entitlements-JWT travels with the access token and is half of what
    # authenticates this account. Its header name contains none of the words
    # above, so nothing was catching it.
    (
        re.compile(r"(X-Riot-Entitlements-JWT\s*[:=]\s*)\S+", re.IGNORECASE),
        r"\1[REDACTED]",
    ),
    # Backstop: anything JWT-shaped, wherever it appears and whatever it is
    # called. Three base64url segments, the first starting with the "eyJ" that
    # every JSON header encodes to. This is what catches the field nobody
    # thought of.
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
    # Every shape that has ever mattered, including the two that used to get
    # through: the entitlements header, and accessToken, which the JSON rule
    # missed because it wanted the key to be exactly "token".
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
        assert "REDACT" in _out, f"not redacted: {_sample!r} -> {_out!r}"
        assert "eyJ" not in _out or "REDACTED-JWT" in _out, f"jwt survived: {_out!r}"

    # And that it does not redact everything it sees, which would be its own
    # kind of useless.
    assert redact("backend started on port 5000") == "backend started on port 5000"
    assert "Ascent" in redact("map Ascent, round 12")

    print(f"overseerlog self-check OK ({len(_MUST_REDACT)} secret shapes redacted)")

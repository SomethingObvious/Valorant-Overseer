# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""Your VALORANT status on your Discord profile, while Discord Status is on."""

from __future__ import annotations

import io
import json
import os
import struct
import threading
import time
import uuid
from typing import Any, cast

import overseerlog
from common.check import check, present

LOG = overseerlog.get_logger("presence")

# The Valorant Overseer application on Discord's developer portal. Discord
# shows its name and icon above the lines this sends.
APP_ID = "1572079396049911988"
# Discord takes the first free pipe of ten, so a second copy of it or its
# Canary build can sit on another.
_PIPES = [rf"\\.\pipe\discord-ipc-{n}" for n in range(10)]
# Discord allows five updates in twenty seconds. A score moves about once a
# minute, so this is plenty and never near that.
_EVERY = 15
_HANDSHAKE, _FRAME = 0, 1
# Discord's replies are a few KB. Anything near this is not Discord.
_LARGEST = 1 << 20

_STATE: dict[str, Any] = {"on": False, "board": None, "thread": None}
_LOCK = threading.Lock()


def set_on(*, on: bool) -> None:
    """Turn the status on or off. Off clears it from your profile within a tick."""
    with _LOCK:
        _STATE["on"] = on and bool(APP_ID)
        thread = _STATE["thread"]
        if _STATE["on"] and (thread is None or not thread.is_alive()):
            _STATE["thread"] = threading.Thread(target=_run, daemon=True, name="presence")
            _STATE["thread"].start()


def observe(board: dict[str, Any]) -> None:
    """Keep the newest board, for the next tick to read."""
    _STATE["board"] = board


def activity(board: dict[str, Any] | None) -> dict[str, Any] | None:
    """Return what Discord shows for a board, or None when VALORANT isn't up."""
    if not board or board.get("state") not in ("MENUS", "PREGAME", "INGAME"):
        return None
    state = board["state"]
    if state == "MENUS":
        return {"details": "In the Menus"}
    where = " on ".join(w for w in (board.get("mode"), board.get("map")) if w)
    you = next((p for p in board.get("players") or [] if p.get("isSelf")), None) or {}
    out: dict[str, Any] = {"details": where or "In a Match"}
    score = board.get("score")
    if state == "PREGAME":
        out["state"] = "Agent Select"
    elif isinstance(score, dict) and "ally" in score and "enemy" in score:
        out["state"] = f"{score['ally']} to {score['enemy']}"
    else:
        out["state"] = "In a Match"
    if you.get("agent") and state == "INGAME":
        out["state"] += f" as {you['agent']}"
    return out


def _frame(op: int, body: dict[str, Any]) -> bytes:
    data = json.dumps(body).encode()
    return struct.pack("<II", op, len(data)) + data


def _exactly(pipe: io.FileIO, size: int) -> bytes:
    got = b""
    while len(got) < size:
        piece = pipe.read(size - len(got))
        if not piece:
            msg = "Discord closed the pipe"
            raise OSError(msg)
        got += piece
    return got


def _answer(pipe: io.FileIO) -> dict[str, Any]:
    size = struct.unpack("<II", _exactly(pipe, 8))[1]
    if size > _LARGEST:
        msg = f"a {size} byte reply isn't from Discord"
        raise ValueError(msg)
    reply = json.loads(_exactly(pipe, size)) if size else {}
    return reply if isinstance(reply, dict) else {}


def _connect() -> io.FileIO | None:
    for path in _PIPES:
        try:
            # Unbuffered, so each frame reaches Discord whole as it is written.
            pipe = io.FileIO(path, "r+")
        except OSError:
            continue
        try:
            pipe.write(_frame(_HANDSHAKE, {"v": 1, "client_id": APP_ID}))
            if _answer(pipe).get("evt") == "READY":
                return pipe
            LOG.warning("Discord turned the status down")
        except (OSError, ValueError) as e:
            LOG.warning("Discord's pipe didn't answer: %r", e)
        pipe.close()
    return None


def _send(pipe: io.FileIO, shown: dict[str, Any] | None) -> None:
    args: dict[str, Any] = {"pid": os.getpid()}
    if shown is not None:
        args["activity"] = shown
    pipe.write(_frame(_FRAME, {"cmd": "SET_ACTIVITY", "args": args, "nonce": uuid.uuid4().hex}))
    reply = _answer(pipe)
    if reply.get("evt") == "ERROR":
        LOG.warning("Discord didn't take the status: %s", reply.get("data"))


def _close(pipe: io.FileIO | None) -> None:
    if pipe is not None:
        try:
            pipe.close()
        except OSError as e:
            LOG.info("closing Discord's pipe failed: %r", e)


def _run() -> None:
    pipe: io.FileIO | None = None
    sent: dict[str, Any] | None = None
    began: tuple[Any, float] = (None, time.time())
    while True:
        on = _STATE["on"]
        shown = activity(_STATE["board"]) if on else None
        if shown is not None:
            # The clock on the profile runs from when this stretch began, so
            # it counts the match and not the time since Overseer opened.
            key = (shown.get("details"), (_STATE["board"] or {}).get("matchId"))
            if key != began[0]:
                began = (key, time.time())
            shown = {**shown, "timestamps": {"start": int(began[1])}}
        try:
            if shown != sent:
                if pipe is None and shown is not None:
                    pipe = _connect()
                if pipe is not None:
                    _send(pipe, shown)
                    sent = shown
        except (OSError, ValueError) as e:
            LOG.info("lost Discord, trying again in a while: %r", e)
            _close(pipe)
            pipe, sent = None, None
        except Exception:
            # Anything else would end the thread, and the status with it,
            # until the setting was turned off and on again.
            LOG.exception("the Discord status failed, trying again in a while")
            _close(pipe)
            pipe, sent = None, None
        if not on:
            _close(pipe)
            pipe, sent = None, None
            with _LOCK:
                if not _STATE["on"]:
                    _STATE["thread"] = None
                    return
        time.sleep(_EVERY)


if __name__ == "__main__":
    board: dict[str, Any] = {
        "state": "INGAME",
        "mode": "Competitive",
        "map": "Ascent",
        "score": {"ally": 7, "enemy": 5, "round": 13},
        "players": [{"isSelf": True, "agent": "Sova"}, {"agent": "Jett"}],
    }
    check(activity(board) == {"details": "Competitive on Ascent", "state": "7 to 5 as Sova"})
    check(present(activity({**board, "state": "PREGAME"}))["state"] == "Agent Select")
    check(present(activity({**board, "score": None}))["state"] == "In a Match as Sova")
    check(activity({"state": "MENUS"}) == {"details": "In the Menus"})
    check(activity({"state": "OFFLINE"}) is None and activity(None) is None)

    class _Pieces:
        """A pipe that hands a reply over three bytes at a time."""

        def __init__(self, data: bytes) -> None:
            self.data = data

        def read(self, size: int) -> bytes:
            piece, self.data = self.data[: min(size, 3)], self.data[min(size, 3) :]
            return piece

    def pieces(data: bytes) -> io.FileIO:
        """Return the stand-in as the pipe type the readers take."""
        return cast("io.FileIO", _Pieces(data))

    ready = _frame(_FRAME, {"evt": "READY", "data": {"v": 1, "user": {"id": "1"}}})
    check(_answer(pieces(ready))["evt"] == "READY", "a reply in pieces reads whole")
    for broken, why in (
        (ready[:20], "a reply cut short"),
        (struct.pack("<II", 1, _LARGEST + 1), "a reply too big to be Discord's"),
    ):
        try:
            read = _answer(pieces(broken))
        except (OSError, ValueError):
            continue
        check(read is None, f"{why} was read as {read!r}")
    print("presence self-check OK")

# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""Player card art, fetched once per card into a folder on this machine.

The window draws a player's card behind their row until they pick an agent,
the way the game's own scoreboard does. The window does not go online, so
the backend fetches the card's banner and the board carries the local path
once the file is there. A card that is still on its way is not on this board
yet, and the next one a second later has it.
"""

from __future__ import annotations

import re
import threading
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from typing import Any

import overseerlog
import requests
from common.check import check
from common.png import save_png

LOG = overseerlog.get_logger("cards")

FOLDER = Path(__file__).resolve().parent / "data" / "cards"
_CARD_ID = re.compile(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}")
_IN_URL = re.compile(r"/playercards/([0-9a-fA-F-]{36})/")
_POOL = ThreadPoolExecutor(max_workers=2, thread_name_prefix="cards")
_ASKED: set[str] = set()
_LOCK = threading.Lock()


def art(card_id: str | None) -> str | None:
    """Return the card's banner on disk, or None while it is still being fetched."""
    # The id comes from Riot and ends up in a file name, so anything that is
    # not exactly a uuid is refused rather than cleaned up.
    card = (card_id or "").lower()
    if not _CARD_ID.fullmatch(card):
        return None
    path = FOLDER / f"{card}.png"
    if path.exists():
        return str(path)
    with _LOCK:
        if card in _ASKED:
            return None
        _ASKED.add(card)
    _POOL.submit(_fetch, card, path)
    return None


def _fetch(card: str, path: Path) -> None:
    url = f"https://media.valorant-api.com/playercards/{card}/wideart.png"
    try:
        save_png(url, path, timeout=10)
    # RequestException is an OSError too, so it has to be caught first.
    except (requests.RequestException, ValueError):
        LOG.warning("could not fetch player card %s", card, exc_info=True)
    except OSError:
        # Windows refuses the replace while another process has the file open,
        # which rarely lasts. The next board asks again.
        LOG.warning("could not save player card %s", card, exc_info=True)
        with _LOCK:
            _ASKED.discard(card)


def attach(board: dict[str, Any]) -> dict[str, Any]:
    """Add `cardArt` to every player whose card is on disk."""
    for p in board.get("players") or []:
        if not isinstance(p, dict):
            continue
        found = _IN_URL.search(str(p.get("playerCard") or ""))
        local = art(found.group(1)) if found else None
        if local:
            p["cardArt"] = local
    return board


def _self_check() -> None:
    # Nothing that is not a uuid reaches the file system.
    for bad in ("", None, "../../etc", "d93ad22d-4db7-b6bc-5e9c-e5959bb9dd7", "x" * 36):
        check(art(bad) is None, bad)
    with _LOCK:
        check(not _ASKED, _ASKED)
    board = {"players": [{"playerCard": "not a card"}, "not a player"]}
    check("cardArt" not in attach(board)["players"][0])

    # A card that could not be saved is logged and asked for again.
    from unittest import mock

    card = "d93ad22d-4db7-b6bc-5e9c-e5959bb9dd7a"
    with (
        mock.patch.object(requests, "get", return_value=mock.Mock(content=b"\x89PNG")),
        mock.patch.object(Path, "mkdir", side_effect=PermissionError("read-only")),
    ):
        with _LOCK:
            _ASKED.add(card)
        _fetch(card, FOLDER / f"{card}.png")
    with _LOCK:
        check(card not in _ASKED, _ASKED)
    print("cards self-check OK (ids checked before they touch a path, failed saves retried)")


if __name__ == "__main__":
    _self_check()

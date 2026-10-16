# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""Riot's art for an agent or a map newer than this build, from valorant-api.

The window carries art for every agent and map there was when it was built.
For one that came after, it asks for this, which fetches Riot's own pictures
once into data/art, and the window cuts them down the way the carried ones
were cut.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any
from urllib.parse import urlparse

import overseerlog
import requests
import valapi
from common.check import check
from common.jsonstore import data_path
from common.png import save_png

LOG = overseerlog.get_logger("backend")

ART = Path(data_path("art"))
_MEDIA = "media.valorant-api.com"


def slug(name: object) -> str:
    """Return the key the window files art under: letters and digits, lowercased."""
    return "".join(c for c in str(name or "").lower() if c.isascii() and c.isalnum())


def _keep(url: object, path: Path) -> bool:
    """Save one picture from valorant-api's media host, unless it is here already."""
    if path.exists():
        return True
    address = urlparse(str(url or ""))
    if address.scheme != "https" or address.netloc != _MEDIA:
        return False
    try:
        save_png(str(url), path)
    except (requests.RequestException, OSError, ValueError) as e:
        LOG.warning("could not fetch %s: %r", url, e)
        return False
    return True


def fetch(kind: object, name: object) -> dict[str, Any]:
    """Fetch the art for the agent or map `name`, once, and say whether it is here."""
    key = slug(name)
    if not 0 < len(key) <= 40:
        return {"ok": False}
    if kind == "agent":
        for agent in valapi.get("agents?isPlayableCharacter=true") or []:
            if slug(agent.get("displayName")) == key:
                portrait = _keep(agent.get("displayIcon"), ART / "agents" / f"{key}.png")
                _keep(agent.get("killfeedPortrait"), ART / "killfeed" / f"{key}.png")
                return {"ok": portrait}
    elif kind == "map":
        for plan in valapi.get("maps") or []:
            if slug(plan.get("displayName")) == key:
                return {"ok": _keep(plan.get("listViewIcon"), ART / "maps" / f"{key}.png")}
    return {"ok": False}


def _self_check() -> None:
    import tempfile
    from unittest import mock

    check(slug("KAY/O") == "kayo" and slug(None) == "")
    media = "https://media.valorant-api.com/agents/x/"
    agents = [
        {
            "displayName": "New/Comer",
            "displayIcon": media + "a.png",
            "killfeedPortrait": media + "k.png",
        },
        {"displayName": "Elsewhere", "displayIcon": media.replace(_MEDIA, "elsewhere.example")},
    ]
    maps = [{"displayName": "Newmap", "listViewIcon": "https://media.valorant-api.com/maps/m.png"}]
    fetched: list[str] = []

    def fake_save(url: str, path: Path) -> None:
        fetched.append(url)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(b"png")

    def listed(path: str) -> Any:
        return maps if path == "maps" else agents

    with (
        tempfile.TemporaryDirectory() as tmp,
        mock.patch(f"{__name__}.ART", Path(tmp)),
        mock.patch(f"{__name__}.save_png", side_effect=fake_save),
        mock.patch.object(valapi, "get", side_effect=listed),
    ):
        check(fetch("agent", "newcomer") == {"ok": True})
        check(fetch("agent", "newcomer") == {"ok": True} and len(fetched) == 2, "fetched once")
        check(fetch("agent", "elsewhere") == {"ok": False}, "only valorant-api's media host")
        check(fetch("map", "Newmap") == {"ok": True})
        check(fetch("agent", "x" * 41) == {"ok": False})
        check(fetch("skin", "newcomer") == {"ok": False})
    print("art self-check OK (fetched once, one host, odd asks)")


if __name__ == "__main__":
    _self_check()

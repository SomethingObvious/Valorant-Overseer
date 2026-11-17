# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""Art and names from valorant-api.com: skins, weapons, ranks, titles and maps."""

from __future__ import annotations

import re
import threading
import time
from typing import Any

import overseerlog
import requests
from common import data_path, read_json, write_atomic

LOG = overseerlog.get_logger("backend")

BASE = "https://valorant-api.com/v1"
SKIN_SOCKET = "bcef87d6-209b-46c6-8b19-fbe40bd95abc"

cache: dict[str, Any] = {}

RETRY_SECS = 300.0
failed_at: dict[str, float] = {}
fetched_at: dict[str, float] = {}
_FETCHING = threading.Lock()
_GUNS = threading.Lock()
_ON_DISK: dict[str, Any] = {}


def _stored(path: str) -> str:
    return data_path(f"valapi/{re.sub(r'[^a-z0-9]+', '-', path.lower()).strip('-')}.json")


def _fallback(path: str) -> Any:
    if path in cache:
        return cache[path]
    if path not in _ON_DISK:
        _ON_DISK[path] = read_json(_stored(path), None)
    return _ON_DISK[path]


def get(path: str, max_age: float | None = None) -> Any:
    """Return the `data` of valorant-api's `path`, kept once it comes back."""

    def kept() -> bool:
        age = time.time() - fetched_at.get(path, 0.0)
        return path in cache and (max_age is None or 0 <= age < max_age)

    if kept():
        return cache[path]
    with _FETCHING:
        if kept():
            return cache[path]
        now = time.time()
        if now - failed_at.get(path, 0.0) < RETRY_SECS:
            return _fallback(path)
        try:
            data = requests.get(f"{BASE}/{path}", timeout=10).json().get("data")
        except (requests.RequestException, ValueError, AttributeError) as e:
            LOG.warning("valorant-api /%s failed: %r", path, e)
            data = None
        if data is None:
            failed_at[path] = now
            return _fallback(path)
        if data != cache.get(path):
            write_atomic(_stored(path), data, prefix=".valapi-")
        cache[path] = data
        _ON_DISK.pop(path, None)
        fetched_at[path] = now
        return data


def keep[T](key: str, table: T) -> T:
    """Cache a table built from get unless it is empty, which means the fetch failed."""
    if table:
        cache[key] = table
    return table


def _guns() -> tuple[dict[str, Any], dict[str, Any]]:
    with _GUNS:
        if "_weapons" not in cache:
            weapons, skins = {}, {}
            for w in get("weapons") or []:
                if w.get("uuid") and w.get("displayName"):
                    weapons[w["uuid"].lower()] = {
                        "name": w["displayName"],
                        "icon": w.get("displayIcon"),
                    }
                for skin in w.get("skins") or []:
                    skins[skin["uuid"].lower()] = {
                        "name": skin.get("displayName", "").strip(),
                        "icon": skin.get("displayIcon"),
                    }
            if not weapons:
                return weapons, skins
            cache["_weapons"], cache["_skins"] = weapons, skins
            cache.pop("weapons", None)
            _ON_DISK.pop("weapons", None)
        return cache["_weapons"], cache["_skins"]


def weapons() -> list[dict[str, Any]]:
    """Return every weapon's name and icon."""
    return list(_guns()[0].values())


def skin_from_id(skin_id: str, weapon: str) -> dict[str, Any] | None:
    """Return a skin's name without its weapon's, and its icon, by skin id."""
    entry = _guns()[1].get(skin_id.lower()) if skin_id else None
    if entry is None:
        return None
    short = entry["name"].replace(f" {weapon}", "").strip()
    return {"name": short or entry["name"], "icon": entry["icon"]}


WEAPON_ORDER = [
    "Vandal",
    "Phantom",
    "Operator",
    "Sheriff",
    "Classic",
    "Ghost",
    "Frenzy",
    "Spectre",
    "Stinger",
    "Bulldog",
    "Guardian",
    "Marshal",
    "Outlaw",
    "Bucky",
    "Judge",
    "Ares",
    "Odin",
    "Shorty",
    "Melee",
]


def weapon_name(uuid: str) -> str | None:
    """Return a weapon's name by its id."""
    if not uuid:
        return None
    entry = _guns()[0].get(uuid.lower())
    return entry["name"] if entry else None


def weapon_icon(uuid: str) -> str | None:
    """Return a weapon's icon by its id."""
    if not uuid:
        return None
    entry = _guns()[0].get(uuid.lower())
    return entry["icon"] if entry else None


def skins_for_weapon(weapon: str) -> list[Any]:
    """Return every skin a weapon has, by the weapon's name, standard ones left out."""
    key = f"_skinsfor_{weapon.lower()}"
    if key in cache:
        return cache[key]
    skins = _guns()[1]
    ending = f" {weapon.lower()}"
    found = []
    for skin in skins.values():
        full = skin.get("name") or ""
        lowered = full.lower()
        if skin.get("icon") and lowered.endswith(ending) and not lowered.startswith("standard"):
            found.append({"name": full[: -len(ending)].strip() or full, "icon": skin["icon"]})
    if skins:
        cache[key] = found
    return found


_GUN_PLACE = {name: place for place, name in enumerate(WEAPON_ORDER)}


def loadout_weapons(items: dict[str, Any]) -> list[Any]:
    """Return the weapons in a loadout, each with the skin on it."""
    found = []
    for gun_id, item in (items or {}).items():
        gun = weapon_name(gun_id)
        if not gun:
            continue
        socket = ((item or {}).get("Sockets") or {}).get(SKIN_SOCKET) or {}
        skin_id = (socket.get("Item") or {}).get("ID")
        skin = skin_from_id(skin_id, gun) if skin_id else None
        if (
            not skin
            or not skin.get("icon")
            or (skin.get("name") or "").strip().lower() == "standard"
        ):
            skin = {"name": "Standard", "icon": weapon_icon(gun_id)}
        found.append({"weapon": gun, "skin": skin})
    return sorted(found, key=lambda w: _GUN_PLACE.get(str(w["weapon"]), len(_GUN_PLACE)))


def _titles_map() -> dict[str, Any]:
    if "_titles" in cache:
        return cache["_titles"]
    out = {t["uuid"].lower(): (t.get("titleText") or "") for t in (get("playertitles") or [])}
    return keep("_titles", out)


def title_text(title_id: str | None) -> str | None:
    """Return what a player's title says, by its id."""
    if not title_id:
        return None
    return _titles_map().get(title_id.lower()) or None


def player_card(card_id: str | None) -> str | None:
    """Return the address of a player card's wide art, by its id."""
    if not card_id:
        return None
    return f"https://media.valorant-api.com/playercards/{card_id}/wideart.png"


def map_by_url(map_url: str) -> str | None:
    """Return a map's name from its match path, for maps newer than the table in vconstants."""
    urls = cache.get("_map_urls")
    if urls is None:
        urls = keep(
            "_map_urls",
            {
                (m.get("mapUrl") or "").lower(): m.get("displayName")
                for m in get("maps") or []
                if m.get("mapUrl") and m.get("displayName")
            },
        )
    return urls.get(map_url.rstrip("/").lower())


_ROMAN = {"I": 1, "V": 5, "X": 10, "L": 50, "C": 100}


def act_number(name: str) -> int | None:
    """Return the number in an act's name, from digits or a roman numeral."""
    words = (name or "").split()
    word = words[-1].upper() if words else ""
    if word.isdigit():
        return int(word)
    if not word or any(c not in _ROMAN for c in word):
        return None
    values = [_ROMAN[c] for c in word]
    return (
        sum(-v if v < after else v for v, after in zip(values, [*values[1:], 0], strict=True))
        or None
    )


def episode_label(name: str | None) -> str | None:
    """Return an episode's short label, like E9, from its name."""
    words = (name or "").split()
    # A name like E9 or V25 already is its own label.
    short = next(
        (w for w in words if any(c.isalpha() for c in w) and any(c.isdigit() for c in w)), None
    )
    if short:
        return short.upper()
    if words and words[0].upper() == "EPISODE" and (number := act_number(name or "")):
        return f"E{number}"
    return None


def _season_labels() -> dict[str, Any]:
    if "_seasonlabels" in cache:
        return cache["_seasonlabels"]
    seasons = get("seasons") or []
    names = {s["uuid"].lower(): s.get("displayName") for s in seasons if s.get("uuid")}
    labels = {}
    for act in seasons:
        number = act_number(act.get("displayName")) if "Act" in (act.get("type") or "") else None
        if number is None:
            continue
        episode = episode_label(names.get((act.get("parentUuid") or "").lower()))
        labels[act["uuid"].lower()] = f"{episode} Act {number}" if episode else f"Act {number}"
    return keep("_seasonlabels", labels)


def season_label(season_id: str) -> str | None:
    """Return a season's label, like E9 A3, by its id."""
    if not season_id:
        return None
    return _season_labels().get(season_id.lower())


def _self_check() -> None:
    import shutil
    import tempfile
    from concurrent.futures import ThreadPoolExecutor
    from pathlib import Path
    from unittest import mock

    from common.check import check

    listed = [
        {
            "uuid": "G1",
            "displayName": "Vandal",
            "displayIcon": "v.png",
            "skins": [{"uuid": "S1", "displayName": " Prime Vandal ", "displayIcon": "p.png"}],
        }
    ]

    def slow(*_: Any, **__: Any) -> Any:
        time.sleep(0.05)
        return mock.Mock(json=lambda: {"data": listed})

    kept_in = Path(tempfile.mkdtemp(prefix="valapi-check-"))
    stored = mock.patch(f"{__name__}._stored", lambda path: str(kept_in / f"{path}.json"))
    with stored, mock.patch.object(requests, "get", side_effect=slow) as fetched:
        with ThreadPoolExecutor(max_workers=8) as pool:
            names = list(pool.map(weapon_name, ["g1"] * 8))
        check(names == ["Vandal"] * 8 and fetched.call_count == 1, fetched.call_count)
        check(skin_from_id("s1", "Vandal") == {"name": "Prime", "icon": "p.png"})
        check(weapons() == [{"name": "Vandal", "icon": "v.png"}])
        check(fetched.call_count == 1 and "weapons" not in cache, "the whole answer was kept")
        fetched.side_effect = requests.ConnectionError("down")
        check(get("seasons") is None and get("seasons") is None)
        check(fetched.call_count == 2, fetched.call_count)
        for table in (cache, failed_at, fetched_at, _ON_DISK):
            table.clear()
        check(get("weapons") == listed, "the last answer is read back from disk")
        check(weapons() == [{"name": "Vandal", "icon": "v.png"}] and "weapons" not in _ON_DISK)
    shutil.rmtree(kept_in, ignore_errors=True)
    print("valapi self-check OK (one fetch between threads, the weapons' answer let go, offline)")


if __name__ == "__main__":
    _self_check()

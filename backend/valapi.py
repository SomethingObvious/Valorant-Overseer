# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""Art and names from valorant-api.com: skins, weapons, ranks, titles and maps.

Only read, cached in memory, and asked again at most every five minutes after
a failure.
"""

from __future__ import annotations

import time
from typing import Any

import overseerlog
import requests

LOG = overseerlog.get_logger("backend")

BASE = "https://valorant-api.com/v1"
SKIN_SOCKET = "bcef87d6-209b-46c6-8b19-fbe40bd95abc"

cache: dict[str, Any] = {}

# A path that failed is not asked for again until this long after, so an
# outage costs one request and one warning per path every five minutes.
RETRY_SECS = 300.0
failed_at: dict[str, float] = {}


def get(path: str) -> Any:
    """Return the `data` of valorant-api's `path`, kept once it comes back.

    None when it fails, and for RETRY_SECS after, so an outage costs one request.
    """
    if path in cache:
        return cache[path]
    if time.time() - failed_at.get(path, 0.0) < RETRY_SECS:
        return None
    try:
        data = requests.get(f"{BASE}/{path}", timeout=10).json().get("data")
    except (requests.RequestException, ValueError, AttributeError) as e:
        LOG.warning("valorant-api /%s failed: %r", path, e)
        data = None
    if data is None:
        failed_at[path] = time.time()
    else:
        cache[path] = data
    return data


def keep[T](key: str, table: T) -> T:
    """Cache a table built from get unless it is empty, which means the fetch failed."""
    if table:
        cache[key] = table
    return table


def _skins_map() -> dict[str, Any]:
    if "_skins" in cache:
        return cache["_skins"]
    out = {}
    for skin in get("weapons/skins") or []:
        out[skin["uuid"].lower()] = {
            "name": skin.get("displayName", "").strip(),
            "icon": skin.get("displayIcon"),
        }
    return keep("_skins", out)


def skin_from_id(skin_id: str, weapon: str = "Vandal") -> dict[str, Any] | None:
    """Return a skin's name without its weapon's, and its icon, by skin id."""
    if not skin_id:
        return None
    entry = _skins_map().get(skin_id.lower())
    if not entry:
        return None
    name = entry["name"].replace(f" {weapon}", "").strip() or entry["name"]
    return {"name": name, "icon": entry["icon"]}


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


def _weapons_map() -> dict[str, Any]:
    if "_weapons" in cache:
        return cache["_weapons"]
    out = {}
    for w in get("weapons") or []:
        if w.get("uuid") and w.get("displayName"):
            out[w["uuid"].lower()] = {"name": w["displayName"], "icon": w.get("displayIcon")}
    return keep("_weapons", out)


def weapon_name(uuid: str) -> str | None:
    """Return a weapon's name by its id."""
    if not uuid:
        return None
    entry = _weapons_map().get(uuid.lower())
    return entry["name"] if entry else None


def weapon_icon(uuid: str) -> str | None:
    """Return a weapon's icon by its id."""
    if not uuid:
        return None
    entry = _weapons_map().get(uuid.lower())
    return entry["icon"] if entry else None


def skins_for_weapon(weapon: str) -> list[Any]:
    """Return every skin a weapon has, by the weapon's name, standard ones left out."""
    key = "_skinsfor_" + weapon.lower()
    if key in cache:
        return cache[key]
    suffix = " " + weapon.lower()
    out = []
    skins = _skins_map()
    for v in skins.values():
        n = v.get("name") or ""
        if v.get("icon") and n.lower().endswith(suffix) and not n.lower().startswith("standard"):
            out.append({"name": n[: -len(weapon) - 1].strip() or n, "icon": v["icon"]})
    # A weapon with no skins is a real empty list, so this goes by whether the
    # skins came back.
    if skins:
        cache[key] = out
    return out


def loadout_weapons(items: dict[str, Any]) -> list[Any]:
    """Return the weapons in a loadout, each with the skin on it."""
    if not items:
        return []
    out = []
    for wuuid, item in items.items():
        wname = weapon_name(wuuid)
        if not wname:
            continue
        skin_id = (
            ((item or {}).get("Sockets", {}) or {}).get(SKIN_SOCKET, {}).get("Item", {}).get("ID")
        )
        skin = skin_from_id(skin_id, wname) if skin_id else None

        if (
            not skin
            or not skin.get("icon")
            or (skin.get("name") or "").strip().lower() == "standard"
        ):
            skin = {"name": "Standard", "icon": weapon_icon(wuuid)}
        out.append({"weapon": wname, "skin": skin})
    order = {name: i for i, name in enumerate(WEAPON_ORDER)}
    out.sort(key=lambda w: order.get(str(w["weapon"]), len(order)))
    return out


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


def player_card(card_id: str | None, kind: str = "wide") -> str | None:
    """Return the address of a player card's art, by its id and kind."""
    if not card_id:
        return None
    return f"https://media.valorant-api.com/playercards/{card_id}/{kind}art.png"


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


def act_number(name: str) -> int | None:
    """Return the number in an act's name, from digits or a roman numeral."""
    parts = (name or "").strip().split()
    tok = parts[-1] if parts else ""
    if tok.isdigit():
        return int(tok)
    roman = {"I": 1, "V": 5, "X": 10, "L": 50, "C": 100}
    total = prev = 0
    for ch in reversed(tok.upper()):
        if ch not in roman:
            return None
        v = roman[ch]
        total += -v if v < prev else v
        prev = v
    return total or None


def episode_label(name: str | None) -> str | None:
    """Return an episode's short label, like E9, from its name."""
    if not name:
        return None
    for tok in name.split():
        if any(c.isalpha() for c in tok) and any(c.isdigit() for c in tok):
            return tok.upper()
    parts = name.strip().split()
    if parts and parts[0].upper() == "EPISODE":
        n = act_number(name)
        return f"E{n}" if n else None
    return None


def _season_labels() -> dict[str, Any]:
    if "_seasonlabels" in cache:
        return cache["_seasonlabels"]
    data = get("seasons") or []
    by_id = {s["uuid"].lower(): s for s in data if s.get("uuid")}
    out = {}
    for s in data:
        if "Act" not in (s.get("type") or ""):
            continue
        num = act_number(s.get("displayName"))
        if num is None:
            continue
        ep = by_id.get((s.get("parentUuid") or "").lower())
        ep_label = episode_label((ep or {}).get("displayName"))
        out[s["uuid"].lower()] = f"{ep_label} Act {num}" if ep_label else f"Act {num}"
    return keep("_seasonlabels", out)


def season_label(season_id: str) -> str | None:
    """Return a season's label, like E9 A3, by its id."""
    if not season_id:
        return None
    return _season_labels().get(season_id.lower())

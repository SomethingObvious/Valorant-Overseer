# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""Riot's fixed tables: ranks, modes, map paths, regions and party colours."""

from __future__ import annotations

from pathlib import Path
from typing import Any

import valapi


def _read_version() -> str:
    try:
        v = (Path(__file__).resolve().parent.parent / "VERSION").read_text(encoding="utf-8").strip()
    except OSError:
        return "1.1"
    else:
        return v or "1.1"


APP_VERSION = _read_version()

# Riot account/match-history endpoints are addressed by cluster, not shard.
ROUTING = {
    "na": "americas",
    "latam": "americas",
    "br": "americas",
    "eu": "europe",
    "ap": "asia",
    "kr": "asia",
}

MAPS = [
    "Ascent",
    "Bind",
    "Haven",
    "Split",
    "Lotus",
    "Sunset",
    "Abyss",
    "Breeze",
    "Icebox",
    "Fracture",
    "Pearl",
    "Corrode",
    "Summit",
]

GAMEMODES = {
    "competitive": "Competitive",
    "unrated": "Unrated",
    "swiftplay": "Swiftplay",
    "spikerush": "Spike Rush",
    "deathmatch": "Deathmatch",
    "ggteam": "Escalation",
    "hurm": "Team Deathmatch",
    "fortcollins": "Retake",
    "skirmish2v2": "Skirmish 2v2",
    "abilitydraftarena": "Ability Draft",
    "onefa": "Replication",
    "snowball": "Snowball Fight",
    "premier": "Premier",
    "newmap": "New Map",
    "custom": "Custom",
}

_RANK_GROUPS = [
    ("Unranked", ["", "", ""], "#4A4A4A"),
    ("Iron", ["Iron 1", "Iron 2", "Iron 3"], "#5A5751"),
    ("Bronze", ["Bronze 1", "Bronze 2", "Bronze 3"], "#BB8F5A"),
    ("Silver", ["Silver 1", "Silver 2", "Silver 3"], "#AEB2B2"),
    ("Gold", ["Gold 1", "Gold 2", "Gold 3"], "#C5BA3F"),
    ("Platinum", ["Platinum 1", "Platinum 2", "Platinum 3"], "#18A7B9"),
    ("Diamond", ["Diamond 1", "Diamond 2", "Diamond 3"], "#D864C7"),
    ("Ascendant", ["Ascendant 1", "Ascendant 2", "Ascendant 3"], "#189452"),
    ("Immortal", ["Immortal 1", "Immortal 2", "Immortal 3"], "#DD4444"),
    ("Radiant", ["Radiant"], "#FFFDCD"),
]

RANKS: list[dict[str, Any]] = []
for _group, _names, _color in _RANK_GROUPS:
    for _n in _names:
        RANKS.append(
            {
                "tier": len(RANKS),
                "name": _n or "Unranked",
                "group": _group,
                "color": _color,
            }
        )


def rank_from_tier(tier: int | None) -> dict[str, Any]:
    """Return a rank's name and colour by tier, clamped to the tiers there are."""
    if tier is None:
        tier = 0
    tier = max(0, min(int(tier), len(RANKS) - 1))
    return RANKS[tier]


def map_name_from_path(map_id: str) -> str:
    """Return a map's name from its path, Unknown when there is none."""
    if not map_id:
        return "Unknown"
    leaf = map_id.rstrip("/").split("/")[-1]

    alias = {
        "Triad": "Haven",
        "Duality": "Bind",
        "Bonsai": "Split",
        "Ascent": "Ascent",
        "Port": "Icebox",
        "Foxtrot": "Breeze",
        "Canyon": "Fracture",
        "Pitt": "Pearl",
        "Jam": "Lotus",
        "Juliett": "Sunset",
        "Infinity": "Abyss",
        "Rook": "Corrode",
        "Plummet": "Summit",
        "HURM_Alley": "District",
        "HURM_Bowl": "Kasbah",
        "HURM_Helix": "Drift",
        "HURM_HighTide": "Glitch",
        "HURM_Yard": "Piazza",
        "Skirmish_A": "Skirmish A",
        "Skirmish_B": "Skirmish B",
        "Skirmish_C": "Skirmish C",
        "Skirmish_D": "Skirmish D",
        "Skirmish_E": "Skirmish E",
        "AbilityDraft": "Gauntlet",
        "Range": "The Range",
        "RangeV2": "The Range",
    }
    known = alias.get(leaf) or (leaf if leaf in MAPS else None)
    # A map newer than this table. Riot's own map list has its display name,
    # and the file name is a codename nobody sees in game.
    return known or valapi.map_by_url(map_id) or leaf or "Unknown"


# A pair per side, the same pairs the window draws. Your parties and the
# other team's never share a colour, and none is the enemy's red, your
# side's green or the flag's amber.
PARTY_OURS = ["#C58BFF", "#F28DD5"]
PARTY_THEIRS = ["#5CE1E6", "#7AA2FF"]


def party_color(index: int, *, ours: bool) -> str:
    """Return a party's colour, from our palette or theirs."""
    palette = PARTY_OURS if ours else PARTY_THEIRS
    return palette[index % len(palette)]


STATES = {
    "MENUS": "In Lobby",
    "PREGAME": "Agent Select",
    "INGAME": "In Game",
    "OFFLINE": "Offline",
}

# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""Made-up players for the demo board: names, ids and match lines."""

from __future__ import annotations

import hashlib
import random
from datetime import UTC, datetime, timedelta
from typing import Any

from agents import AGENTS
from vconstants import MAPS

AGENT_NAMES = [a["name"] for a in AGENTS]

_NAME_PARTS_A = [
    "Toxic",
    "Silent",
    "Rapid",
    "Phantom",
    "Neon",
    "Vexed",
    "Frost",
    "Echo",
    "Lurk",
    "Solo",
    "Astra",
    "Viper",
    "Crimson",
    "Hazy",
    "Quick",
    "Zero",
    "Lucid",
    "Nova",
    "Karma",
    "Sage",
    "Drift",
    "Pixel",
    "Ghost",
    "Riot",
]
_NAME_PARTS_B = [
    "Diff",
    "Lock",
    "Aim",
    "Flick",
    "Smoke",
    "Clutch",
    "Dash",
    "Frag",
    "Wisp",
    "Ace",
    "Peek",
    "Spike",
    "Recon",
    "Bolt",
    "Edge",
    "Vibe",
    "Main",
    "Andy",
    "Gamer",
    "btw",
    "ttv",
    "yt",
    "Op",
    "Ent",
]
_TAGS = ["NA1", "EUW", "APAC", "1337", "GG", "VAL", "KR", "OCE", "000", "RR"]


def _seed_int(puuid: str) -> int:
    return int.from_bytes(hashlib.sha256(puuid.encode()).digest()[:8], "big")


def make_name(rng: random.Random) -> str:
    """Return a made-up Riot ID."""
    return f"{rng.choice(_NAME_PARTS_A)}{rng.choice(_NAME_PARTS_B)}#{rng.choice(_TAGS)}"


def fake_puuid(rng: random.Random) -> str:
    """Return a made-up puuid shaped like a real one."""
    hexs = "0123456789abcdef"
    raw = "".join(rng.choice(hexs) for _ in range(32))
    return f"{raw[:8]}-{raw[8:12]}-{raw[12:16]}-{raw[16:20]}-{raw[20:]}"


def _line(rng: random.Random, rounds: int, skill: float) -> dict[str, Any]:
    per_round = rng.uniform(0.55, 0.95) * (0.85 + skill / 2)
    kills = max(3, round(per_round * rounds + rng.uniform(-3, 4)))
    deaths = max(3, round(rounds * rng.uniform(0.45, 0.8)))
    assists = round(rounds * rng.uniform(0.15, 0.5))
    # About what ACS comes to from 220 a kill and 50 an assist, give or take.
    acs = round((220 * kills + 50 * assists) / max(rounds, 1) * rng.uniform(0.9, 1.15))
    acs = max(80, min(420, acs))
    return {
        "kills": kills,
        "deaths": deaths,
        "assists": assists,
        "score": acs * rounds,
        "acs": acs,
        "hsPct": round(rng.uniform(14, 44), 1),
    }


def _result(rng: random.Random, skill: float) -> tuple[str, int, int]:
    if rng.random() < 0.04:
        return "Draw", 12, 12
    if rng.random() < 0.40 + skill * 0.30:
        return "Victory", 13, rng.randint(3, 11)
    return "Defeat", rng.randint(3, 11), 13


def _stranger(rng: random.Random) -> dict[str, Any]:
    return {"puuid": fake_puuid(rng), "name": make_name(rng), "agent": rng.choice(AGENT_NAMES)}


def generate_player(puuid: str, match_count: int = 20) -> dict[str, Any]:
    """Return a made-up player for `puuid`, the same one every time."""
    rng = random.Random(_seed_int(puuid))
    skill = rng.uniform(0.25, 0.85)
    main = rng.choice(AGENT_NAMES)
    others = rng.sample([a for a in AGENT_NAMES if a != main], 5)
    friends = [{"puuid": fake_puuid(rng), "name": make_name(rng)} for _ in range(rng.randint(2, 4))]
    now = datetime.now(UTC)
    matches = []
    for n in range(match_count):
        result, won, lost = _result(rng, skill)
        came = rng.sample(friends, rng.randint(0, min(3, len(friends))))
        team = [{**f, "agent": rng.choice(AGENT_NAMES)} for f in came]
        team += [_stranger(rng) for _ in range(4 - len(team))]
        matches.append(
            {
                "matchId": fake_puuid(rng),
                "map": rng.choice(MAPS),
                "mode": "Competitive"
                if rng.random() < 0.78
                else rng.choice(["Unrated", "Swiftplay"]),
                "date": (now - timedelta(hours=n * rng.uniform(5, 30))).isoformat(),
                "result": result,
                "roundsWon": won,
                "roundsLost": lost,
                "agent": main if rng.random() < 0.45 else rng.choice(others),
                "stats": _line(rng, won + lost, skill),
                "teammates": team,
            }
        )
    return {
        "puuid": puuid,
        "rankTier": 3 + int(skill * 21),
        "rr": rng.randint(8, 98),
        "matches": matches,
        "source": "demo",
    }

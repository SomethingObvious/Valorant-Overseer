# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""A whole made-up lobby for the demo, and every screen's data to go with it."""

from __future__ import annotations

import random
import time
from datetime import datetime
from typing import Any

import past_games
import sample_data
import valapi
from agents import resolve_agent
from career import career_summary, form_streak
from live_match import assemble_player, finalize
from smurf import compute_smurf
from vconstants import GAMEMODES, MAPS, STATES, party_color, rank_from_tier

_SKINS = [
    "Prime",
    "Reaver",
    "Glitchpop",
    "Sovereign",
    "Elderflame",
    "Oni",
    "RGX 11z Pro",
    "Araxys",
    "Champions 2022",
    "Recon",
    "Ion",
    "Prelude to Chaos",
    "Sentinels of Light",
    "Gaia's Vengeance",
]
_TITLES = [
    "Hardcore",
    "Mastermind",
    "Marksman",
    "Ace",
    "Clutch",
    "Tactician",
    "Legend",
    "Sharpshooter",
    "First Blood",
    "Rookie",
    "Vandalizer",
]

_WEAPONS = [
    "Vandal",
    "Phantom",
    "Operator",
    "Sheriff",
    "Classic",
    "Ghost",
    "Spectre",
    "Marshal",
    "Guardian",
    "Judge",
    "Bulldog",
    "Odin",
]

_CARDS = [
    "1711d20d-4b1c-c64a-14be-d4ae58a457c6",
    "c8b2f5fd-4331-b172-f3b7-c8a26f356a1f",
    "eef542d2-4724-bc47-f53f-239f8c9c2623",
    "d32e58b1-4191-7315-ad4a-9da58b3f23dd",
    "d2d3caf9-499f-2ac8-9722-54961c3bcbf5",
    "e8787c31-4a39-9636-94a5-77b298d26ba7",
]


def _queue_state() -> dict[str, Any]:
    return {
        "available": True,
        "partyId": "demo-party",
        "queueId": "competitive",
        "queueName": GAMEMODES["competitive"],
        "eligible": [{"id": q, "name": n} for q, n in GAMEMODES.items() if q != "custom"],
        "state": "DEFAULT",
        "inQueue": False,
        "queuedAt": None,
        "partySize": 1,
        "isOwner": True,
        "allReady": True,
        "demo": True,
    }


def _weapons(rng: random.Random) -> list[dict[str, Any]]:
    weapons = []
    for weapon in _WEAPONS:
        skins = valapi.skins_for_weapon(weapon)
        skin = rng.choice(skins) if skins else {"name": rng.choice(_SKINS), "icon": None}
        weapons.append({"weapon": weapon, "skin": skin})
    return weapons


def _form(rng: random.Random, *, hot: bool) -> list[str]:
    won = ["W"] * rng.randint(3, 4) if hot else []
    return won + [rng.choice("WL") for _ in range(5 - len(won))]


def _habits(rng: random.Random) -> dict[str, Any]:
    rounds = rng.randint(90, 120)
    carried = {"Vandal": rounds // 3, "Phantom": rounds // 4}
    carried[rng.choice(["Operator", "Outlaw", "Judge", "Marshal", "Spectre"])] = rng.choice(
        [2, rounds // 4]
    )
    pistols = rng.randint(2, 5)
    return {
        "matches": 5,
        "rounds": rounds,
        "carried": carried,
        "kills": rng.randint(60, 100),
        "killsWith": {},
        "lostPistols": pistols,
        "forced": rng.choice([0, 1, pistols - 1, pistols]),
        "firstBloods": rng.randint(6, 12) + (14 if rng.random() < 0.2 else 0),
        "firstDeaths": rng.randint(6, 12),
        "clutches": rng.choice([0, 1, 1, 2, 3]),
        "aces": rng.choice([0, 0, 0, 1]),
        "plants": rng.randint(4, 14),
        "asked": 20,
    }


def _intel(
    rng: random.Random,
    main_agent: str | None = None,
    map_name: str | None = None,
    *,
    hot: bool = False,
) -> dict[str, Any]:
    picks = [a for a in rng.sample(sample_data.AGENT_NAMES, 4) if a != main_agent][:2]
    main = main_agent or picks.pop(0)
    played = [(main, rng.randint(3, 6)), *((a, rng.randint(1, 3)) for a in picks)]
    form = _form(rng, hot=hot)
    wins = {}
    if map_name:
        games = rng.randint(2, 6)
        wins[map_name] = [rng.randint(0, games), games]
    return {
        "topAgents": [
            {"agent": a, "games": n, "role": (resolve_agent(a) or {}).get("role")}
            for a, n in played
        ],
        "form": form,
        "streak": form_streak(form),
        "mapWins": wins,
        "habits": _habits(rng),
    }


def _figures(rng: random.Random, around: int, *, smurf: bool) -> dict[str, Any]:
    tier = max(3, min(27, around + rng.randint(-3, 3)))
    figures = {
        "tier": tier,
        "peak": min(27, tier + rng.randint(0, 4)),
        "games": rng.randint(20, 400),
        "winRate": rng.randint(38, 64),
        "kd": round(rng.uniform(0.6, 1.8), 2),
        "level": rng.randint(20, 480),
        "hs": rng.randint(12, 28),
    }
    if smurf:
        figures.update(
            tier=max(tier, 20),
            peak=max(figures["peak"], 22),
            kd=round(rng.uniform(1.4, 2.1), 2),
            winRate=max(figures["winRate"], 64),
            level=rng.randint(18, 55),
            hs=rng.randint(30, 38),
        )
    figures["lb"] = rng.randint(1, 500) if figures["tier"] >= 24 else 0
    return figures


def _row(rng: random.Random, figures: dict[str, Any], **known: Any) -> dict[str, Any]:
    smurf, why = compute_smurf(
        level=figures["level"],
        peak_tier=figures["peak"],
        rank_tier=figures["tier"],
        kd=figures["kd"],
        win_rate=figures["winRate"],
        games=figures["games"],
        kd_matches=5,
        hs=float(figures["hs"]),
    )
    return assemble_player(
        rank_tier=figures["tier"],
        rr=rng.randint(0, 99),
        leaderboard=figures["lb"],
        peak_tier=figures["peak"],
        prev_tier=max(0, figures["tier"] - rng.randint(0, 3)),
        win_rate=figures["winRate"],
        games=figures["games"],
        kd=figures["kd"],
        hs=figures["hs"],
        level=figures["level"],
        peak_act=f"V{rng.randint(25, 26)} Act {rng.randint(1, 5)}",
        title=rng.choice(_TITLES),
        player_card=valapi.player_card(rng.choice(_CARDS)),
        smurf=smurf,
        smurf_reasons=why,
        **known,
    )


def generate(seed: int = 7) -> dict[str, Any]:
    """Return a made-up board for a match in progress."""
    rng = random.Random(seed)
    agents = rng.sample(sample_data.AGENT_NAMES, 10)
    around = rng.randint(11, 24)
    map_name = rng.choice(MAPS)
    seats = [(team, n) for team in ("Blue", "Red") for n in range(5)]
    people: dict[tuple[str, int], dict[str, Any]] = {
        seat: {
            "puuid": sample_data.fake_puuid(rng),
            "name": sample_data.make_name(rng),
            "agent": agents.pop(),
            "hiddenName": rng.random() < 0.35,
            "hiddenLevel": rng.random() < 0.2,
        }
        for seat in seats
    }
    me = people["Blue", 0]["puuid"]
    parties: dict[str, dict[str, Any]] = {}
    for number, (team, members) in enumerate((("Blue", (0, 1, 2)), ("Red", (0, 1))), 1):
        tag = {"id": sample_data.fake_puuid(rng), "color": party_color(0, ours=team == "Blue")}
        for n in members:
            parties[people[team, n]["puuid"]] = {**tag, "number": number}
    smurfs = {("Blue", 1), ("Red", 1)}
    players = []
    for seat in seats:
        person = people[seat]
        figures = _figures(rng, around, smurf=seat in smurfs)
        weapons = _weapons(rng)
        players.append(
            _row(
                rng,
                figures,
                puuid=person["puuid"],
                name=person["name"],
                name_hidden=person["hiddenName"],
                team=seat[0],
                is_self=person["puuid"] == me,
                agent_id=person["agent"],
                level_hidden=person["hiddenLevel"],
                party=parties.get(person["puuid"]),
                skin=next(w["skin"] for w in weapons if w["weapon"] == "Vandal"),
                weapons=weapons,
                rr_earned=rng.randint(-24, 28),
                intel=_intel(rng, person["agent"], map_name, hot=seat in smurfs),
            )
        )
    ally, enemy = sorted([rng.randint(0, 13), rng.randint(0, 13)], reverse=True)
    board = finalize(
        players,
        state="INGAME",
        source="demo",
        self_team="Blue",
        map_name=map_name,
        queue="competitive",
        match_id=f"demo-{seed}",
        score={"ally": ally, "enemy": enemy, "round": ally + enemy + 1},
    )
    board["sourceDetail"] = "Demo lobby (open VALORANT for live data)"
    board["queue"] = _queue_state()
    board["session"] = session(seed)
    return board


def generate_pregame(seed: int = 7) -> dict[str, Any]:
    """Agent select as the game shows it: your team only, no score, three still to lock in."""
    board = generate(seed)
    mine = next((p["team"] for p in board["players"] if p.get("isSelf")), "Blue")
    players = [p for p in board["players"] if p.get("team") == mine]
    for p in players[2:]:
        p.update(
            agent=None,
            agentId=None,
            agentPortrait=None,
            agentArt=None,
            agentColor=None,
            role=None,
            selection="",
        )
    board.update(
        players=players,
        state="PREGAME",
        stateLabel=STATES["PREGAME"],
        score=None,
        sourceDetail="Demo agent select (open VALORANT for live data)",
    )
    return board


def generate_lobby(seed: int = 7) -> dict[str, Any]:
    """Return a made-up lobby board for a party in menus."""
    rng = random.Random(seed * 31 + 5)
    size = rng.randint(2, 5)
    around = rng.randint(11, 24)
    party = {"id": "lobby", "color": party_color(0, ours=True), "number": 1, "size": size}
    players = [
        _row(
            rng,
            _figures(rng, around, smurf=n == 1),
            puuid=sample_data.fake_puuid(rng),
            name=sample_data.make_name(rng),
            name_hidden=False,
            team="Blue",
            is_self=n == 0,
            agent_id="",
            level_hidden=False,
            party=party,
            intel=_intel(rng, hot=n == 1),
        )
        for n in range(size)
    ]
    board = finalize(
        players,
        state="MENUS",
        source="demo",
        self_team="Blue",
        map_name=None,
        queue="Lobby",
        match_id=f"demo-lobby-{seed}",
    )
    board["sourceDetail"] = "Demo lobby (open VALORANT for live data)"
    board["queue"] = _queue_state()
    board["session"] = session(seed)
    return board


def session(seed: int = 7) -> dict[str, Any]:
    """Return a made-up play session with its rating points."""
    rng = random.Random(seed * 13 + 1)
    games = rng.randint(5, 9)
    now = int(time.time())
    rating = rng.randint(11, 24) * 100 + rng.randint(20, 80)
    points = []
    for n in range(games):
        delta = rng.choice([1, 1, -1]) * rng.randint(12, 28)
        rating += delta
        points.append(
            {
                "matchId": f"demo-s{n}",
                "ts": now - (games - n) * 2400,
                "map": rng.choice(MAPS),
                "result": "Victory" if delta > 0 else "Defeat",
                "delta": delta,
                "tier": max(3, min(27, rating // 100)),
                "rr": rating % 100,
            }
        )
    return {"startedAt": points[0]["ts"], "net": sum(p["delta"] for p in points), "points": points}


def career(puuid: str, tier: int | None = None, rr: int | None = None) -> dict[str, Any]:
    """Return a made-up career whose newest match ends on the board's `tier` and `rr`."""
    raw = sample_data.generate_player(puuid, match_count=10)
    matches = []
    rr_rng = random.Random(sum(ord(ch) for ch in puuid) + 41)
    tier_after = tier or raw.get("rankTier") or 16
    rr_after = rr if rr is not None else (raw.get("rr") or 50)
    for m in raw["matches"]:
        st = m["stats"]
        agent = resolve_agent(m["agent"]) or {}
        is_comp = m["mode"] == "Competitive"
        delta = (
            (
                rr_rng.randint(14, 24)
                if m["result"] == "Victory"
                else -rr_rng.randint(12, 22)
                if m["result"] == "Defeat"
                else 0
            )
            if is_comp
            else None
        )
        rank = rank_from_tier(tier_after)
        matches.append(
            {
                "matchId": m["matchId"],
                "map": m["map"],
                "mode": m["mode"],
                "startMillis": int(datetime.fromisoformat(m["date"]).timestamp() * 1000),
                "result": m["result"],
                "score": m.get("roundsWon"),
                "agent": m["agent"],
                "agentColor": agent.get("color", "#8B978F"),
                "kills": st["kills"],
                "deaths": st["deaths"],
                "assists": st["assists"],
                "kd": round(st["kills"] / st["deaths"], 2) if st["deaths"] else float(st["kills"]),
                "acs": st["acs"],
                "hsPct": round(st["hsPct"]),
                "partySize": len(m["teammates"]) + 1,
                "rrDelta": delta,
                "tierAfter": tier_after if is_comp else None,
                "rrAfter": rr_after if is_comp else None,
                "rankAfter": rank["name"] if is_comp else None,
                "teammates": [
                    {"puuid": t["puuid"], "name": t["name"], "agent": t["agent"]}
                    for t in m["teammates"]
                ],
            }
        )
        if delta is not None:
            before = rr_after - delta
            tier_after += before // 100
            rr_after = before % 100
            if tier_after < 3:
                tier_after, rr_after = 3, 0
            elif tier_after > 27:
                tier_after = 27
    rating = [
        {
            "matchId": m["matchId"],
            "startMillis": m["startMillis"],
            "map": m["map"],
            "tierAfter": m["tierAfter"],
            "rrAfter": m["rrAfter"],
            "rrDelta": m["rrDelta"],
            "rankAfter": m["rankAfter"],
        }
        for m in matches
        if m["tierAfter"]
    ]
    return {
        "source": "demo",
        "puuid": puuid,
        "matches": matches,
        "rating": rating,
        **career_summary(matches),
    }


def history(count: int) -> dict[str, Any]:
    """Return a run of past matches for the History screen, newest first."""
    rng = random.Random(29)
    now = int(time.time() * 1000)
    games = []
    for n in range(max(1, min(past_games.MOST, count))):
        agents = rng.sample(sample_data.AGENT_NAMES, 10)
        won = rng.random() < 0.55
        lost = rng.randint(4, 11)
        ours, theirs = (13, lost) if won else (lost, 13)
        players: list[dict[str, Any]] = [
            {
                "puuid": "demo-self" if i == 0 else sample_data.fake_puuid(rng),
                "name": "This player" if i == 0 else sample_data.make_name(rng),
                "team": "Blue" if i < 5 else "Red",
                "agent": agents[i],
                "kills": rng.randint(6, 28),
                "deaths": rng.randint(8, 22),
                "assists": rng.randint(1, 12),
                "acs": rng.randint(110, 330),
                "adr": rng.randint(70, 210),
                "hsPct": rng.randint(10, 38),
                "kast": rng.randint(55, 88),
                "firstBloods": rng.randint(0, 6),
            }
            for i in range(10)
        ]
        players.sort(key=lambda p: -p["acs"])
        match_id = f"demo-{n:04d}"
        board = {
            "matchId": match_id,
            "map": rng.choice(MAPS),
            "mode": "Competitive",
            "startedAt": now - (n + 1) * 2_700_000 - rng.randint(0, 900_000),
            "lengthMs": (ours + theirs) * 100_000,
            "rounds": ours + theirs,
            "teams": [
                {"id": "Blue", "roundsWon": ours, "won": won},
                {"id": "Red", "roundsWon": theirs, "won": not won},
            ],
            "players": players,
        }
        delta = rng.randint(14, 24) if won else -rng.randint(10, 21)
        games.append(past_games.view(board, "demo-self", {match_id: delta}))
    for game in games:
        game["usual"] = past_games.usual(game, games, "demo-self")
    return {"games": games, "asked": count}

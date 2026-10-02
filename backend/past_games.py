# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""Your last few matches with their scoreboards, for the History screen.

A finished match never changes, so its scoreboard is kept on disk after the
first fetch and asking again costs one history call.
"""

from __future__ import annotations

import re
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from typing import Any

import encounter_log
import history
import overseerlog
from agents import resolve_agent
from common.check import check, present
from common.jsonstore import data_path, read_json, write_atomic
from live_match import _fallback_name, _is_throttled, _mode_label
from rounds import round_stats
from vconstants import map_name_from_path

LOG = overseerlog.get_logger("past_games")

# The most the window may ask for. Every match not on disk yet is a request
# to Riot, and forty is two pages of history.
MOST = 40
# Riot's match history answers twenty at a time.
_PAGE = 20
_DIR = Path(data_path("games"))
# Match ids are UUIDs, and nothing else may become a file name.
_ID = re.compile(r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$")
# Three at a time, so a first look at forty games doesn't trip the rate limit.
_POOL = ThreadPoolExecutor(max_workers=3, thread_name_prefix="past-games")


def _match_ids(auth: Any, puuid: str, count: int) -> tuple[list[str], bool]:
    """Return the newest `count` match ids, and whether Riot cut the list short."""
    ids: list[str] = []
    for start in range(0, count, _PAGE):
        end = min(count, start + _PAGE)
        page = auth.pd_get(
            f"/match-history/v1/history/{puuid}?startIndex={start}&endIndex={end}", retries=2
        )
        if _is_throttled(page):
            return ids, True
        entries = page.get("History") if isinstance(page, dict) else None
        found = [e["MatchID"] for e in entries or [] if isinstance(e, dict) and e.get("MatchID")]
        ids.extend(found)
        if len(found) < end - start:
            break
    return ids[:count], False


def scoreboard(md: dict[str, Any]) -> dict[str, Any] | None:
    """One match's end-of-game scoreboard, the same whoever asks for it."""
    info = md.get("matchInfo") or {}
    raw = [p for p in md.get("players") or [] if isinstance(p, dict) and p.get("subject")]
    if not raw or not info.get("matchId"):
        return None
    teams = [t for t in md.get("teams") or [] if isinstance(t, dict) and t.get("teamId")]
    # Played rounds, not rounds won, since a surrender ends a match early.
    rounds = (
        len(md.get("roundResults") or []) or sum(int(t.get("roundsWon") or 0) for t in teams) or 1
    )
    extra = round_stats(md, rounds)
    players = []
    for p in raw:
        sub = p["subject"]
        st = p.get("stats") or {}
        more = extra.get(sub) or {}
        shots = more.get("shots") or 0
        players.append(
            {
                "puuid": sub,
                "name": f"{p['gameName']}#{p.get('tagLine') or ''}"
                if p.get("gameName")
                else _fallback_name(sub),
                "team": p.get("teamId"),
                "agent": (resolve_agent(p.get("characterId") or "") or {}).get("name"),
                "kills": int(st.get("kills") or 0),
                "deaths": int(st.get("deaths") or 0),
                "assists": int(st.get("assists") or 0),
                "acs": round(int(st.get("score") or 0) / rounds),
                "adr": more.get("adr"),
                "hsPct": round(more.get("headshots", 0) / shots * 100) if shots else None,
                "kast": more.get("kastPct"),
                "firstBloods": more.get("firstBloods"),
            }
        )
    players.sort(key=lambda p: -p["acs"])
    return {
        "matchId": info["matchId"],
        "map": map_name_from_path(info.get("mapId") or ""),
        "mode": _mode_label(info.get("queueID") or info.get("queueId") or ""),
        "startedAt": info.get("gameStartMillis"),
        "lengthMs": info.get("gameLengthMillis"),
        "rounds": len(md.get("roundResults") or []),
        "teams": [
            {
                "id": t["teamId"],
                "roundsWon": int(t.get("roundsWon") or 0),
                "won": bool(t.get("won")),
            }
            for t in teams
        ],
        "players": players,
    }


def view(board: dict[str, Any], puuid: str, deltas: dict[str, int]) -> dict[str, Any]:
    """Return a scoreboard as `puuid` saw it: their side, the score from it, and their RR."""
    game = dict(board)
    game["players"] = [{**p, "isSubject": p.get("puuid") == puuid} for p in board["players"]]
    mine = next((p.get("team") for p in game["players"] if p["isSubject"]), None)
    teams = board.get("teams") or []
    ours = next((t for t in teams if t["id"] == mine), None)
    theirs = next((t for t in teams if t["id"] != mine), None)
    # A deathmatch has a side per player, so it has no score to read.
    if len(teams) == 2 and ours and theirs:
        game["score"] = [ours["roundsWon"], theirs["roundsWon"]]
        game["result"] = "Victory" if ours["won"] else "Defeat" if theirs["won"] else "Draw"
    game["yourTeam"] = mine
    game["rrDelta"] = deltas.get(board["matchId"])
    return game


def _board(auth: Any, match_id: str) -> dict[str, Any] | None:
    """Return a match's scoreboard from disk, or from Riot the first time."""
    path = _DIR / f"{match_id}.json"
    kept = read_json(str(path), None)
    if isinstance(kept, dict) and kept.get("players"):
        return kept
    md = auth.pd_get(f"/match-details/v1/matches/{match_id}", retries=2)
    if _is_throttled(md) or not isinstance(md, dict):
        return None
    board = scoreboard(md)
    if board:
        # ponytail: never pruned, about 3 KB a match, so a year of play is a few MB.
        write_atomic(str(path), board, prefix=".game-")
    return board


def recent(auth: Any, puuid: str, count: int) -> dict[str, Any]:
    """Your newest `count` matches, newest first. `partial` says Riot held some back."""
    count = max(1, min(MOST, count))
    ids, throttled = _match_ids(auth, puuid, count)
    ids = [i for i in ids if _ID.match(i)]
    boards = list(_POOL.map(lambda i: _board(auth, i), ids))
    try:
        history.refresh(auth)
    except Exception as e:
        LOG.warning("rr refresh for the history screen failed: %r", e)
    deltas = history.deltas(puuid)
    games = [view(b, puuid, deltas) for b in boards if b]
    # Everybody in them counts as met, even in games played while
    # Overseer was closed.
    try:
        encounter_log.record_games(puuid, games)
    except Exception as e:
        LOG.warning("logging the history's players failed: %r", e)
    out: dict[str, Any] = {"games": games, "asked": count}
    if throttled or len(games) < len(ids):
        out["partial"] = True
    return out


def _self_check() -> None:
    import tempfile
    from types import SimpleNamespace
    from unittest import mock

    def md(match_id: str, *, blue_won: bool) -> dict[str, Any]:
        def player(sub: str, team: str, kills: int) -> dict[str, Any]:
            return {
                "subject": sub,
                "teamId": team,
                "gameName": "Me" if sub == "me" else "",
                "tagLine": "1",
                "characterId": "",
                "stats": {"kills": kills, "deaths": 2, "assists": 1, "score": kills * 200},
            }

        hit = {"receiver": "them", "damage": 150, "headshots": 1, "bodyshots": 3, "legshots": 0}
        return {
            "matchInfo": {"matchId": match_id, "mapId": "", "queueID": "competitive"},
            "players": [player("me", "Blue", 4), player("them", "Red", 2)],
            "teams": [
                {"teamId": "Blue", "roundsWon": 2 if blue_won else 1, "won": blue_won},
                {"teamId": "Red", "roundsWon": 1 if blue_won else 2, "won": not blue_won},
            ],
            "roundResults": [
                {"playerStats": [{"subject": "me", "damage": [hit]}, {"subject": "them"}]}
            ]
            * 2,
        }

    # The scoreboard reads per round, so the ACS is over the two rounds played
    # and the headshot rate is one in four.
    board = scoreboard(md("00000000-0000-0000-0000-000000000001", blue_won=True))
    board = present(board)
    me = next(p for p in board["players"] if p["puuid"] == "me")
    check((me["name"], me["acs"], me["hsPct"], me["adr"]) == ("Me#1", 400, 25, 150), me)
    check(next(p for p in board["players"] if p["puuid"] == "them")["name"].startswith("Player-"))

    # The score and result read from your side, and the RR joins by match.
    seen = view(board, "them", {"00000000-0000-0000-0000-000000000001": -15})
    check((seen["score"], seen["result"], seen["rrDelta"]) == ([1, 2], "Defeat", -15), seen)
    draw = {**board, "teams": [{**t, "won": False} for t in board["teams"]]}
    check(view(draw, "me", {})["result"] == "Draw")

    # Three ids, one not a UUID. One match is throttled, so the answer is
    # partial, and a second look fetches only that one again.
    ids = [f"00000000-0000-0000-0000-00000000000{n}" for n in (1, 2)] + ["../../evil"]
    calls: list[str] = []

    def pd_get(path: str, **_options: int) -> dict[str, Any]:
        calls.append(path)
        if path.startswith("/match-history"):
            return {"History": [{"MatchID": i} for i in ids]}
        if path.endswith("0002"):
            return {"status": 429}
        return md(path.rsplit("/", 1)[-1], blue_won=False)

    auth = SimpleNamespace(pd_get=pd_get)
    with (
        tempfile.TemporaryDirectory() as tmp,
        mock.patch.object(history, "refresh"),
        mock.patch.object(history, "deltas", return_value={}),
        mock.patch.object(encounter_log, "record_games"),
        mock.patch(f"{__name__}._DIR", Path(tmp)),
    ):
        first = recent(auth, "me", 10)
        check([g["matchId"] for g in first["games"]] == [ids[0]] and first["partial"], first)
        check(not any("evil" in c for c in calls), calls)
        calls.clear()
        again = recent(auth, "me", 10)
        details = [c for c in calls if c.startswith("/match-details")]
        check(details == [f"/match-details/v1/matches/{ids[1]}"], details)
        check(again["games"][0]["result"] == "Defeat")
    print("past_games self-check OK (scoreboard, your side, disk cache, throttling)")


if __name__ == "__main__":
    _self_check()

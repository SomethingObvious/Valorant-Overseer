# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""Your last few matches with their scoreboards, for the History screen."""

from __future__ import annotations

import os
import re
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from typing import Any

import encounter_log
import history
import overseerlog
import requests
from agents import resolve_agent
from common.check import check, present
from common.jsonstore import data_path, read_json, write_atomic
from live_match import _fallback_name, _is_throttled, _mode_label, played
from rounds import round_stats, rounds_played
from vconstants import map_name_from_path

LOG = overseerlog.get_logger("past_games")

# The most the window may ask for. Every match not on disk yet is a request
# to Riot, and forty is two pages of history.
MOST = 40
# Riot's match history answers twenty at a time.
_PAGE = 20
_DIR = Path(data_path("games"))
_ID = re.compile(r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$")
_POOL = ThreadPoolExecutor(max_workers=3, thread_name_prefix="past-games")


def _match_ids(auth: Any, puuid: str, count: int) -> tuple[list[str], bool]:
    ids: list[str] = []
    for start in range(0, count, _PAGE):
        end = min(count, start + _PAGE)
        page = auth.pd_get(
            f"/match-history/v1/history/{puuid}?startIndex={start}&endIndex={end}",
            retries=2,
            urgent=True,
        )
        if _is_throttled(page):
            return ids, True
        entries = page.get("History") if isinstance(page, dict) else None
        if not isinstance(entries, list):
            msg = f"Riot's match history didn't come back: {str(page)[:200]}"
            raise TypeError(msg)
        found = [e["MatchID"] for e in entries if isinstance(e, dict) and e.get("MatchID")]
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
    rounds = rounds_played(md)
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
    if len(teams) == 2 and ours and theirs:
        game["score"] = [ours["roundsWon"], theirs["roundsWon"]]
        game["result"] = "Victory" if ours["won"] else "Defeat" if theirs["won"] else "Draw"
    game["yourTeam"] = mine
    game["rrDelta"] = deltas.get(board["matchId"])
    return game


_USUAL = 20
_USUAL_LEAST = 3
_USUAL_READ = 150


def usual(game: dict[str, Any], boards: list[dict[str, Any]], puuid: str) -> dict[str, Any] | None:
    """Return your averages over the 20 games of `game`'s mode before it, or None under 3."""

    def started(board: dict[str, Any]) -> float:
        at = board.get("startedAt")
        return at if isinstance(at, (int, float)) and not isinstance(at, bool) else 0

    began = started(game)
    earlier = sorted(
        (b for b in boards if b.get("mode") == game.get("mode") and 0 < started(b) < began),
        key=started,
        reverse=True,
    )
    lines = [
        p
        for b in earlier
        if isinstance(players := b.get("players"), list)
        for p in players
        if isinstance(p, dict) and p.get("puuid") == puuid
    ][:_USUAL]
    if len(lines) < _USUAL_LEAST:
        return None

    def numbers(key: str) -> list[float]:
        return [
            n
            for line in lines
            if isinstance(n := line.get(key), (int, float)) and not isinstance(n, bool)
        ]

    def mean(key: str) -> float | None:
        had = numbers(key)
        return round(sum(had) / len(had), 1) if had else None

    kills = sum(numbers("kills"))
    deaths = sum(numbers("deaths"))
    return {
        "games": len(lines),
        "kd": round(kills / max(deaths, 1), 2),
        **{key: mean(key) for key in ("acs", "adr", "hsPct", "kast")},
    }


def _newest(count: int) -> list[dict[str, Any]]:
    files = sorted(_DIR.glob("*.json"), key=lambda f: f.stat().st_mtime, reverse=True)
    return [
        b
        for f in files[:count]
        if isinstance(b := read_json(str(f), None), dict) and b.get("players") and b.get("matchId")
    ]


def keep(auth: Any, match_id: str) -> None:
    """Save a match's scoreboard as it ends, so History has it with VALORANT closed."""
    if _ID.match(match_id):
        _safely(auth, match_id)


def _safely(auth: Any, match_id: str) -> dict[str, Any] | None:
    try:
        return _board(auth, match_id)
    except requests.RequestException as e:
        LOG.warning("match %s didn't load: %r", match_id, e)
        return None
    except Exception:
        LOG.exception("match %s couldn't be read", match_id)
        return None


def _board(auth: Any, match_id: str) -> dict[str, Any] | None:
    path = _DIR / f"{match_id}.json"
    kept = read_json(str(path), None)
    if isinstance(kept, dict) and kept.get("players"):
        _date(path, kept)
        return kept
    md = played(auth, match_id, retries=2)
    if _is_throttled(md) or not isinstance(md, dict):
        return None
    board = scoreboard(md)
    if board:
        # Never pruned, at about 3 KB a match, so a year of play is a few MB.
        write_atomic(str(path), board, prefix=".game-")
        _date(path, board)
    return board


def _date(path: Path, board: dict[str, Any]) -> None:
    try:
        began = float(board.get("startedAt") or 0) / 1000
        if began > 0:
            os.utime(path, (began, began))
    except (OSError, TypeError, ValueError, OverflowError) as e:
        LOG.warning("couldn't date %s by its game: %r", path.name, e)


def recent(auth: Any, puuid: str, count: int) -> dict[str, Any]:
    """Your newest `count` matches, newest first. `partial` says Riot held some back."""
    count = max(1, min(MOST, count))
    ids, throttled = _match_ids(auth, puuid, count)
    ids = [i for i in ids if _ID.match(i)]
    boards = list(_POOL.map(lambda i: _safely(auth, i), ids))
    try:
        history.refresh(auth)
    except Exception as e:
        LOG.warning("rr refresh for the history screen failed: %r", e)
    deltas = history.deltas(puuid)
    games = [view(b, puuid, deltas) for b in boards if b]
    kept = _newest(_USUAL_READ)
    for game in games:
        game["usual"] = usual(game, kept, puuid)
    try:
        encounter_log.record_games(puuid, games)
    except Exception as e:
        LOG.warning("logging the history's players failed: %r", e)
    out: dict[str, Any] = {"games": games, "asked": count}
    if throttled or len(games) < len(ids):
        out["partial"] = True
    return out


def saved(puuid: str | None, count: int) -> dict[str, Any] | None:
    """Your newest `count` games from the disk cache, for when Riot can't be asked."""
    count = max(1, min(MOST, count))
    boards = _newest(max(count * 3, _USUAL_READ))
    if not puuid:
        seen: dict[str, int] = {}
        for board in boards:
            for player in board["players"]:
                seen[player.get("puuid")] = seen.get(player.get("puuid"), 0) + 1
        puuid = max(seen, key=lambda p: seen[p]) if seen else None
    mine = [b for b in boards if any(p.get("puuid") == puuid for p in b["players"])]
    if not puuid or not mine:
        return None
    mine.sort(key=lambda b: b.get("startedAt") or 0, reverse=True)
    mine = mine[:count]
    deltas = history.deltas(puuid)
    newest = mine[0]
    ended = ((newest.get("startedAt") or 0) + (newest.get("lengthMs") or 0)) / 1000
    games = [view(b, puuid, deltas) for b in mine]
    for game in games:
        game["usual"] = usual(game, boards, puuid)
    return {"games": games, "asked": count, "savedAt": ended or None}


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
            "matchInfo": {
                "matchId": match_id,
                "mapId": "",
                "queueID": "competitive",
                "gameStartMillis": 1_700_000_000_000 + int(match_id[-1]) * 1000,
            },
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

    board = present(scoreboard(md("00000000-0000-0000-0000-000000000001", blue_won=True)))
    me = next(p for p in board["players"] if p["puuid"] == "me")
    check((me["name"], me["acs"], me["hsPct"], me["adr"]) == ("Me#1", 400, 25, 150), me)

    seen = view(board, "them", {"00000000-0000-0000-0000-000000000001": -15})
    check((seen["score"], seen["result"], seen["rrDelta"]) == ([1, 2], "Defeat", -15), seen)

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
        dated = (Path(tmp) / f"{ids[0]}.json").stat().st_mtime
        check(dated == 1_700_000_001, dated)

    with (
        tempfile.TemporaryDirectory() as tmp,
        mock.patch.object(history, "deltas", return_value={}),
        mock.patch(f"{__name__}._DIR", Path(tmp)),
    ):
        check(saved("me", 5) is None, "nothing saved yet")

        def game(mid: str, at: int, *who: str) -> None:
            board = {
                "matchId": mid,
                "startedAt": at,
                "lengthMs": 1000,
                "teams": [],
                "players": [{"puuid": p, "team": "Blue"} for p in who],
            }
            write_atomic(str(Path(tmp) / f"{mid}.json"), board, prefix=".game-")

        game("old", 1000, "me", "a")
        game("new", 5000, "me", "b")
        game("theirs", 9000, "c")
        mine = saved("me", 5)
        check(mine is not None and [g["matchId"] for g in mine["games"]] == ["new", "old"], mine)
        check(mine is not None and mine["savedAt"] == 6.0, mine)
        check(saved(None, 5) == mine, "the player in most games is you")
        check(saved("someone else", 5) is None, "not another account's")

    def past(at: int, mode: str, kills: int, acs: int) -> dict[str, Any]:
        line = {"puuid": "me", "kills": kills, "deaths": 10, "acs": acs, "adr": None}
        return {"matchId": str(at), "startedAt": at, "mode": mode, "players": [line]}

    games = [past(n, "Competitive", 10 + n, 200 + 10 * n) for n in range(1, 6)]
    games.append(past(3, "Swiftplay", 40, 900))
    against = present(usual(games[3], games, "me"))
    check((against["games"], against["kd"], against["acs"]) == (3, 1.2, 220.0), against)
    check(against["adr"] is None, "a figure nobody has is left out")
    check(usual(games[2], games, "me") is None, "two games aren't an average")
    broken: list[dict[str, Any]] = [
        {"startedAt": "yesterday", "mode": "Competitive", "players": [{"puuid": "me"}]},
        {"startedAt": 2, "mode": "Competitive", "players": None},
        {"startedAt": 2, "mode": "Competitive", "players": ["me", 7]},
    ]
    check(usual(games[3], games + broken, "me") == against, "broken saved games are passed over")
    print("past_games self-check OK (scoreboard, your side, disk cache, throttling, saved)")


if __name__ == "__main__":
    _self_check()

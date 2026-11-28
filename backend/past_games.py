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
import riot_client
from agents import resolve_agent
from common.check import check, present
from common.jsonstore import data_path, read_json, write_atomic
from live_match import (
    _fallback_name,
    _is_throttled,
    _kept_detail,
    _mode_label,
    _rank_from_mmr,
    played,
    riot_names,
)
from rounds import round_stats, rounds_played
from vconstants import map_name_from_path, rank_from_tier

LOG = overseerlog.get_logger("past_games")

# The most the window may ask for. Every match not on disk yet is a request
# to Riot, and forty is two pages of history.
MOST = 40
# Riot's match history answers twenty at a time.
_PAGE = 20
_DIR = Path(data_path("games"))
# The last board you were on, kept by app.py, which has your own Riot ID.
_YOU = data_path("you.json")
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
                "tier": int(p.get("competitiveTier") or 0),
                "level": int(p.get("accountLevel") or 0) or None,
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


def view(board: dict[str, Any], puuid: str, rated: dict[str, dict[str, Any]]) -> dict[str, Any]:
    """Return a scoreboard as `puuid` saw it: their side, the score from it, and their RR."""
    game = dict(board)
    game["players"] = [
        {**p, **_ranked(p), "isSubject": p.get("puuid") == puuid} for p in board["players"]
    ]
    mine = next((p.get("team") for p in game["players"] if p["isSubject"]), None)
    teams = board.get("teams") or []
    ours = next((t for t in teams if t["id"] == mine), None)
    theirs = next((t for t in teams if t["id"] != mine), None)
    if len(teams) == 2 and ours and theirs:
        game["score"] = [ours["roundsWon"], theirs["roundsWon"]]
        game["result"] = "Victory" if ours["won"] else "Defeat" if theirs["won"] else "Draw"
    game["yourTeam"] = mine
    rating = rated.get(board["matchId"]) or {}
    game["rrDelta"] = rating.get("delta")
    if rating.get("tier") is not None:
        game["rrAfter"] = rating.get("rr")
        game["tierAfter"] = rating["tier"]
        game["rankAfter"] = rank_from_tier(rating["tier"])["name"]
        game["tierBefore"] = rating.get("tierBefore")
    return game


def _ranked(line: dict[str, Any]) -> dict[str, Any]:
    out: dict[str, Any] = {}
    if line.get("tier"):
        out["rank"] = rank_from_tier(line["tier"])["name"]
    if line.get("peak"):
        out["peakRank"] = rank_from_tier(line["peak"])["name"]
    return out


def _filled(boards: list[dict[str, Any]], auth: Any = None) -> None:
    """Fill in what an older saved scoreboard lacks.

    Ranks and levels come from the match details on disk, your own Riot ID
    from the last board you were on, and the rest from one name service
    request.
    """
    _yours(boards)
    for board in boards:
        if all("tier" in p for p in board["players"]):
            continue
        md = _kept_detail(str(board["matchId"])) or {}
        by = {p.get("subject"): p for p in md.get("players") or [] if isinstance(p, dict)}
        changed = False
        for line in board["players"]:
            if "tier" not in line and line.get("puuid") in by:
                riot = by[line["puuid"]]
                line["tier"] = int(riot.get("competitiveTier") or 0)
                line["level"] = int(riot.get("accountLevel") or 0) or None
                changed = True
        if changed:
            _write(board)
    if auth is None:
        return
    unnamed = {
        str(p["puuid"]): board
        for board in boards
        for p in board["players"]
        if p.get("puuid") and p.get("name") == _fallback_name(str(p["puuid"]))
    }
    if not unnamed:
        return
    # Riot answers a hundred at a time.
    found: dict[str, str] = {}
    asked = list(unnamed)
    for at in range(0, len(asked), 100):
        found.update(riot_names(auth, asked[at : at + 100]))
    touched: dict[str, dict[str, Any]] = {}
    for puuid, name in found.items():
        board = unnamed[puuid]
        for line in board["players"]:
            if line.get("puuid") == puuid:
                line["name"] = name
        touched[str(board["matchId"])] = board
    for board in touched.values():
        _write(board)


# Rank lookups share a small budget with agent select, so History takes only
# what is spare and leaves enough for the next lobby. A peak is kept in its
# game once asked, so the rest fill in over the next few opens.
_PEAKS_LEAVE = 10
_PEAKS_MOST = 14


def _peaks(auth: Any, boards: list[dict[str, Any]]) -> None:
    """Ask Riot for the peaks of players Overseer never saw live, newest games first."""
    if riot_client.held_secs("/mmr/") > 0:
        return
    room = min(_PEAKS_MOST, riot_client.mmr_spare() - _PEAKS_LEAVE)
    wanted: list[str] = []
    for board in boards:
        for line in board["players"]:
            puuid = line.get("puuid")
            if puuid and "peak" not in line and puuid not in wanted:
                wanted.append(str(puuid))
    wanted = wanted[: max(0, room)]
    if not wanted:
        return
    found = dict(zip(wanted, _POOL.map(lambda p: _peak_of(auth, p), wanted), strict=True))
    for board in boards:
        asked = False
        for line in board["players"]:
            peak = found.get(line.get("puuid"))
            if peak is not None and "peak" not in line:
                line["peak"] = peak
                asked = True
        if asked:
            _write(board)


def _peak_of(auth: Any, puuid: str) -> int | None:
    """Return the highest tier a player has won at, 0 if never ranked, None on no answer."""
    try:
        mmr = auth.pd_get(f"/mmr/v1/players/{puuid}")
    except requests.RequestException as e:
        LOG.warning("peak lookup for %s failed: %r", puuid[:8], e)
        return None
    if not isinstance(mmr, dict) or "QueueSkills" not in mmr:
        return None
    return int(_rank_from_mmr(mmr, "", None)["peak"])


def _count_met(puuid: str, games: list[dict[str, Any]]) -> None:
    for game in games:
        met = encounter_log.met_elsewhere(
            puuid, str(game.get("matchId")), [str(p.get("puuid")) for p in game["players"]]
        )
        for line in game["players"]:
            line["met"] = met.get(str(line.get("puuid")), 0)


def _yours(boards: list[dict[str, Any]]) -> None:
    """Put your Riot ID on your own lines, which needs no lookup that can fail."""
    you = read_json(_YOU, None)
    me = you.get("player") if isinstance(you, dict) else None
    if not isinstance(me, dict) or "#" not in str(me.get("name") or ""):
        return
    puuid = str(me.get("puuid") or "")
    for board in boards:
        for line in board["players"]:
            if line.get("puuid") == puuid and line.get("name") == _fallback_name(puuid):
                line["name"] = me["name"]
                _write(board)


def _write(board: dict[str, Any]) -> None:
    path = _DIR / f"{board['matchId']}.json"
    write_atomic(str(path), board, prefix=".game-")
    _date(path, board)


def _met(board: dict[str, Any], seen: list[Any] | None) -> bool:
    """Take names and peaks from the live board's players, true if any changed."""
    live = {p.get("puuid"): p for p in seen or [] if isinstance(p, dict)}
    changed = False
    for line in board["players"]:
        them = live.get(line.get("puuid"))
        if not them:
            continue
        name = str(them.get("name") or "")
        if "#" in name and not them.get("nameHidden") and line.get("name") != name:
            line["name"] = name
            changed = True
        peak = them.get("peakRankTier")
        if isinstance(peak, int) and peak and line.get("peak") != peak:
            line["peak"] = peak
            changed = True
    return changed


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


def keep(auth: Any, match_id: str, seen: list[Any] | None = None) -> None:
    """Save a match's scoreboard as it ends, so History has it with VALORANT closed.

    `seen` is the live board's players, whose names and peaks Riot's match
    details don't carry.
    """
    if _ID.match(match_id):
        board = _safely(auth, match_id)
        if board and _met(board, seen):
            _write(board)


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
        _write(board)
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
    found = [b for b in boards if b]
    try:
        _filled(found, auth)
    except Exception as e:
        LOG.warning("filling in the history's names and ranks failed: %r", e)
    try:
        _peaks(auth, found)
    except Exception as e:
        LOG.warning("filling in the history's peaks failed: %r", e)
    rated = history.outcomes(puuid)
    games = [view(b, puuid, rated) for b in found]
    kept = _newest(_USUAL_READ)
    for game in games:
        game["usual"] = usual(game, kept, puuid)
    try:
        encounter_log.record_games(puuid, games)
    except Exception as e:
        LOG.warning("logging the history's players failed: %r", e)
    _count_met(puuid, games)
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
    try:
        _filled(mine)
    except Exception as e:
        LOG.warning("filling in the saved games' ranks failed: %r", e)
    rated = history.outcomes(puuid)
    newest = mine[0]
    ended = ((newest.get("startedAt") or 0) + (newest.get("lengthMs") or 0)) / 1000
    games = [view(b, puuid, rated) for b in mine]
    _count_met(puuid, games)
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

    rated = {"00000000-0000-0000-0000-000000000001": {"delta": -15, "tier": 13, "rr": 90}}
    rated["00000000-0000-0000-0000-000000000001"]["tierBefore"] = 14
    seen = view(board, "them", rated)
    check((seen["score"], seen["result"], seen["rrDelta"]) == ([1, 2], "Defeat", -15), seen)
    check((seen["rankAfter"], seen["tierBefore"], seen["rrAfter"]) == ("Gold 2", 14, 90), seen)
    check(all("rank" not in p for p in seen["players"]), "no rank where Riot gave tier 0")
    live = [{"puuid": "them", "name": "Them#NA1", "peakRankTier": 18}]
    named = {**board, "players": [dict(p) for p in board["players"]]}
    check(_met(named, live), "the live board's names and peaks are taken")
    them = next(p for p in named["players"] if p["puuid"] == "them")
    check((them["name"], them["peak"]) == ("Them#NA1", 18), them)
    hidden = [{"puuid": "them", "name": "Jett", "nameHidden": True}]
    check(not _met(named, hidden), "a hidden name is never written over a real one")

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
        mock.patch.object(history, "outcomes", return_value={}),
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
        mock.patch.object(history, "outcomes", return_value={}),
        mock.patch(f"{__name__}._DIR", Path(tmp)),
        mock.patch(f"{__name__}._YOU", str(Path(tmp) / "you.json")),
    ):
        check(saved("me", 5) is None, "nothing saved yet")
        mine_unnamed: dict[str, Any] = {
            "matchId": "w",
            "players": [{"puuid": "me", "name": _fallback_name("me")}],
        }
        _filled([mine_unnamed])
        check(mine_unnamed["players"][0]["name"] == _fallback_name("me"), "no you.json yet")
        write_atomic(_YOU, {"player": {"puuid": "me", "name": "Me#NA1"}}, prefix=".you-")
        _filled([mine_unnamed])
        check(mine_unnamed["players"][0]["name"] == "Me#NA1", "your own name, offline")
        (Path(tmp) / "w.json").unlink()

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

        unnamed: dict[str, Any] = {
            "matchId": "x",
            "players": [{"puuid": "zz", "name": _fallback_name("zz")}],
        }
        asked: list[list[str]] = []

        def pd_put(_path: str, who: list[str], **_options: bool) -> list[dict[str, str]]:
            asked.append(who)
            return [{"Subject": "zz", "GameName": "Zed", "TagLine": "EUW"}]

        _filled([unnamed], SimpleNamespace(pd_put=pd_put))
        check(unnamed["players"][0]["name"] == "Zed#EUW", unnamed)
        _filled([unnamed], SimpleNamespace(pd_put=pd_put))
        check(asked == [["zz"]], "a name once found is never asked for again")

        acts = {"act": {"WinsByTier": {"12": 3, "16": 1}}}
        ranked: dict[str, Any] = {"QueueSkills": {"competitive": {"SeasonalInfoBySeasonID": acts}}}
        looked: list[str] = []

        def rank_get(path: str) -> dict[str, Any]:
            looked.append(path)
            return ranked if "/zz" in path else {"QueueSkills": {}}

        peakless: dict[str, Any] = {"matchId": "y", "players": [{"puuid": "zz"}, {"puuid": "yy"}]}
        with (
            mock.patch("riot_client.mmr_spare", return_value=24),
            mock.patch("riot_client.held_secs", return_value=0.0),
        ):
            _peaks(SimpleNamespace(pd_get=rank_get), [peakless])
            check([p.get("peak") for p in peakless["players"]] == [16, 0], peakless)
            _peaks(SimpleNamespace(pd_get=rank_get), [peakless])
            check(len(looked) == 2, "a peak once asked is never asked for again")
        with mock.patch("riot_client.mmr_spare", return_value=_PEAKS_LEAVE):
            _peaks(
                SimpleNamespace(pd_get=rank_get), [{"matchId": "z", "players": [{"puuid": "x"}]}]
            )
            check(len(looked) == 2, "the next lobby's lookups are left alone")

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

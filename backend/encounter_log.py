# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""Everyone you have played with or against, and how those games went.

Logged from live boards and backfilled from past games, per signed-in account.
"""

from __future__ import annotations

import threading
import time
from pathlib import Path
from typing import Any

from common import data_path, read_json, write_atomic
from common.check import check, present

_PATH = data_path("encounters.json")
# Riot's three words for how a match ended, and the counter each one feeds.
# A timeline entry stores the word in lowercase, though backfilled entries
# already on disk may say win or loss, so anything reading them takes both.
_OUTCOMES = {"victory": "wins", "defeat": "losses", "draw": "draws"}
# The fields a board or a career row can refresh on a stored player.
_PROFILE_FIELDS = (
    "name",
    "rank",
    "peakRank",
    "rankTier",
    "peakTier",
    "rankIcon",
    "rankColor",
    "kd",
    "winRate",
    "level",
)

# Two hundred lobbies is a few months of play and about a hundred kilobytes.
# Older ones say little, because people stop queueing together.
_ROSTER_LIMIT = 200
_ROSTER_MIN = 4
_LOCK = threading.RLock()


def _empty_store() -> dict[str, Any]:
    return {"version": 2, "accounts": {}, "discardedLegacyPlayers": 0}


def _load() -> dict[str, Any]:
    raw = read_json(_PATH, None)
    if isinstance(raw, dict) and raw.get("version") == 2 and isinstance(raw.get("accounts"), dict):
        return raw
    out = _empty_store()
    if isinstance(raw, dict):
        out["discardedLegacyPlayers"] = sum(isinstance(v, dict) for v in raw.values())
    return out


def _save() -> None:
    write_atomic(_PATH, _STORE, prefix=".encounters-")


_STORE = _load()
# Only when there is no file yet. Saving unconditionally would write over a
# store that could not be moved aside.
if not Path(_PATH).exists():
    _save()


def _blank_entry(puuid: str) -> dict[str, Any]:
    return {
        "puuid": puuid,
        "name": None,
        "withCount": 0,
        "againstCount": 0,
        "winsWith": 0,
        "lossesWith": 0,
        "winsAgainst": 0,
        "lossesAgainst": 0,
        "lastSeen": 0,
        "agents": [],
    }


def _account(owner: str) -> dict[str, Any]:
    return _STORE.setdefault("accounts", {}).setdefault(str(owner), {"players": {}})


def _players(owner: str) -> dict[str, Any]:
    return _account(owner).setdefault("players", {})


def _record_roster(owner: str, match_id: str, board: dict[str, Any], now: int) -> bool:
    """Remember who was on which side, which is what a stack is guessed from."""
    # The per-player counts are all relative to me. Telling that two other
    # people queue together needs the sides as they really were.
    account = _account(owner)
    rosters = account.setdefault("rosters", [])
    if any(r.get("matchId") == match_id for r in rosters):
        return False
    teams = {
        p["puuid"]: p["team"]
        for p in board.get("players") or []
        if isinstance(p, dict) and p.get("puuid") and p.get("team")
    }
    # A half read lobby says nothing about who queued with whom.
    if len(teams) < _ROSTER_MIN:
        return False
    # Both sides or none. Agent select shows only the ally team, so every pair
    # on a pregame board shares a side whoever they are, and storing that
    # would turn four strangers into a five stack.
    if len(set(teams.values())) < 2:
        return False
    rosters.append({"matchId": match_id, "at": now, "teams": teams})
    account["rosters"] = rosters[-_ROSTER_LIMIT:]
    return True


def rosters_for(owner: str | None) -> list[dict[str, Any]]:
    """Return the stored lobbies, newest last. Copied, so a caller cannot edit history."""
    if not owner:
        return []
    with _LOCK:
        return [dict(r) for r in _account(str(owner)).get("rosters") or []]


# When the store was last written by record_board, and how often a board that
# only moved everyone's last seen writes it.
_SAVED = {"at": 0}
_SEEN_EVERY = 60


def record_board(board: dict[str, Any] | None) -> None:
    """Log everyone on a live board as a teammate or an opponent of its owner."""
    if not isinstance(board, dict) or board.get("source") != "local":
        return
    owner = board.get("selfPuuid")
    match_id = board.get("matchId")
    if not owner or not match_id or not isinstance(board.get("players"), list):
        return
    self_team = board.get("selfTeam")
    now = int(time.time())
    changed = False
    with _LOCK:
        store = _players(owner)
        for player in board["players"]:
            if not isinstance(player, dict) or player.get("isSelf") or not player.get("puuid"):
                continue
            puuid = player["puuid"]
            entry = store.setdefault(puuid, _blank_entry(puuid))
            match_ids = entry.setdefault("matchIds", [])
            legacy_match_id = entry.get("lastMatchId")
            if legacy_match_id and legacy_match_id not in match_ids:
                match_ids.append(legacy_match_id)
            if match_id not in match_ids:
                same_team = self_team is not None and player.get("team") == self_team
                key = "withCount" if same_team else "againstCount"
                entry[key] = int(entry.get(key) or 0) + 1
                match_ids.append(match_id)
                entry["matchIds"] = match_ids[-80:]
                entry["lastMatchId"] = match_id
                changed = True
            for key in _PROFILE_FIELDS:
                if player.get(key) is not None and entry.get(key) != player.get(key):
                    entry[key] = player.get(key)
                    changed = True
            entry["lastSeen"] = now
            agent = player.get("agent")
            if agent and agent != "Unknown" and agent not in entry.setdefault("agents", []):
                entry["agents"].append(agent)
                entry["agents"] = entry["agents"][-8:]
                changed = True
        if _record_roster(owner, match_id, board, now):
            changed = True
        # A board comes every few seconds and only moves when someone is seen
        # again, so that alone is written once a minute, not each time.
        if changed or now - _SAVED["at"] >= _SEEN_EVERY:
            _save()
            _SAVED["at"] = now


def record_result(board: dict[str, Any] | None, result: str | None) -> None:
    """Record how a lobby ended for everyone in it, from Riot's Victory, Defeat or Draw."""
    outcome = (result or "").strip().lower()
    if outcome not in _OUTCOMES or not isinstance(board, dict) or board.get("source") != "local":
        return
    owner = board.get("selfPuuid")
    match_id = board.get("matchId")
    if not owner or not match_id:
        return
    self_team = board.get("selfTeam")
    changed = False
    with _LOCK:
        store = _players(owner)
        for player in board.get("players") or []:
            if not isinstance(player, dict) or player.get("isSelf") or not player.get("puuid"):
                continue
            entry = store.get(player["puuid"])
            if not entry:
                continue
            result_ids = entry.setdefault("resultMatchIds", [])
            legacy_result_id = entry.get("lastResultMatchId")
            if legacy_result_id and legacy_result_id not in result_ids:
                result_ids.append(legacy_result_id)
            if match_id in result_ids:
                continue
            same_team = self_team is not None and player.get("team") == self_team
            key = f"{_OUTCOMES[outcome]}{'With' if same_team else 'Against'}"
            entry[key] = int(entry.get(key) or 0) + 1
            result_ids.append(match_id)
            entry["resultMatchIds"] = result_ids[-80:]
            entry["lastResultMatchId"] = match_id
            timeline = entry.setdefault("timeline", [])
            timeline.append(
                {
                    "matchId": match_id,
                    "at": int(time.time()),
                    "side": "with" if same_team else "against",
                    "result": outcome,
                    "agent": player.get("agent"),
                }
            )
            entry["timeline"] = timeline[-20:]
            changed = True
        if changed:
            _save()


def record_games(owner: str | None, games: list[dict[str, Any]] | None) -> int:
    """Log everybody in past games from the History screen.

    Each goes in as a teammate or an opponent. A match already logged, live or
    from an earlier look, is left alone. Returns how many accounts gained a
    match.
    """
    if not owner or not isinstance(games, list):
        return 0
    changed = 0
    with _LOCK:
        store = _players(owner)
        for game in games:
            match_id = str(game.get("matchId") or "") if isinstance(game, dict) else ""
            if not match_id:
                continue
            ours = game.get("yourTeam")
            outcome = str(game.get("result") or "").lower()
            seen_at = int((game.get("startedAt") or 0) / 1000) or int(time.time())
            for player in game.get("players") or []:
                puuid = player.get("puuid") if isinstance(player, dict) else None
                if not puuid or puuid == owner:
                    continue
                entry = store.setdefault(puuid, _blank_entry(puuid))
                match_ids = entry.setdefault("matchIds", [])
                if match_id in match_ids:
                    continue
                side = "With" if ours is not None and player.get("team") == ours else "Against"
                entry[f"{side.lower()}Count"] = int(entry.get(f"{side.lower()}Count") or 0) + 1
                match_ids.append(match_id)
                entry["matchIds"] = match_ids[-80:]
                if outcome in _OUTCOMES:
                    key = f"{_OUTCOMES[outcome]}{side}"
                    entry[key] = int(entry.get(key) or 0) + 1
                    entry.setdefault("resultMatchIds", []).append(match_id)
                    entry["resultMatchIds"] = entry["resultMatchIds"][-80:]
                if player.get("name") and not str(player["name"]).startswith("Player-"):
                    entry["name"] = player["name"]
                agent = player.get("agent")
                if agent and agent not in entry.setdefault("agents", []):
                    entry["agents"] = [*entry["agents"], agent][-8:]
                entry["lastSeen"] = max(int(entry.get("lastSeen") or 0), seen_at)
                entry.setdefault("timeline", []).append(
                    {
                        "matchId": match_id,
                        "at": seen_at,
                        "side": side.lower(),
                        "result": outcome if outcome in _OUTCOMES else None,
                        "agent": agent,
                        "map": game.get("map"),
                    }
                )
                entry["timeline"] = sorted(entry["timeline"], key=lambda t: t.get("at") or 0)[-40:]
                changed += 1
        if changed:
            _save()
    return changed


def teammates(owner: str | None, limit: int = 5) -> list[dict[str, Any]]:
    """Return the accounts most often on your side, for a career's Played With.

    Every lobby on record counts, at two lobbies or more, since one is a random.
    """
    if not owner:
        return []
    with _LOCK:
        rows = [
            {
                "puuid": entry.get("puuid"),
                "name": entry.get("name"),
                "sharedMatches": int(entry.get("withCount") or 0),
                "agents": [],
            }
            for entry in _players(owner).values()
            if int(entry.get("withCount") or 0) >= 2 and entry.get("name")
        ]
    rows.sort(key=lambda row: -row["sharedMatches"])
    return rows[:limit]


def get_one(owner: str | None, puuid: str | None) -> dict[str, Any] | None:
    """Return what is logged about one player, for `owner`."""
    if not owner or not puuid:
        return None
    with _LOCK:
        entry = _players(owner).get(puuid)
        return dict(entry) if entry else None


def encounter_for(owner: str | None, puuid: str | None) -> dict[str, Any] | None:
    """Return a player's with and against counts and results, for their row."""
    entry = get_one(owner, puuid)
    if not entry:
        return None
    keys = (
        "withCount",
        "againstCount",
        "winsWith",
        "lossesWith",
        "drawsWith",
        "winsAgainst",
        "lossesAgainst",
        "drawsAgainst",
    )
    return {key: int(entry.get(key) or 0) for key in keys}


def _no_save() -> None:
    """Stand in for _save while the self-check runs, so nothing reaches disk."""


def _self_check() -> None:
    # The "seen before" number is the one thing this file exists to get right,
    # and it is easy to get wrong in two directions: counting the same lobby
    # twice because a board is polled every few seconds, and counting a player
    # on the wrong side when they change teams between matches.
    # It must never write over the real encounter history, so the store, the
    # writer and the path are all swapped out while it runs.
    import tempfile
    from unittest import mock

    mock.patch(f"{__name__}._STORE", _empty_store()).start()
    mock.patch(f"{__name__}._save", _no_save).start()
    try:
        # A store that will not parse goes aside under a new name, and the
        # load starts empty, so the next save cannot write over it.
        with tempfile.TemporaryDirectory() as tmp:
            mock.patch(f"{__name__}._PATH", str(Path(tmp) / "encounters.json")).start()
            Path(_PATH).write_text('{"version": 2, "accou', encoding="utf-8")
            check(_load() == _empty_store())
            check(not Path(_PATH).exists())
            kept = list(Path(tmp).glob("encounters.json.unreadable-*"))
            check([k.read_text(encoding="utf-8") for k in kept] == ['{"version": 2, "accou'], kept)

        me, mate, foe = "self-1", "mate-1", "foe-1"

        def board(match_id: str, mate_team: str) -> dict[str, Any]:
            return {
                "source": "local",
                "selfPuuid": me,
                "matchId": match_id,
                "selfTeam": "Blue",
                "players": [
                    {"puuid": me, "isSelf": True, "team": "Blue"},
                    {"puuid": mate, "team": mate_team, "name": "Mate"},
                    {"puuid": foe, "team": "Red", "name": "Foe"},
                ],
            }

        first = board("m1", "Blue")
        record_board(first)
        # A board is rebuilt every few seconds. The same lobby must not count
        # again each time it is polled.
        record_board(first)
        record_board(first)

        seen = encounter_for(me, mate)
        seen = present(seen)
        check(seen["withCount"] == 1, seen)
        check(seen["againstCount"] == 0, seen)

        record_result(first, "Victory")
        record_result(first, "Victory")
        after_win = encounter_for(me, mate)
        after_win = present(after_win)
        check(after_win["winsWith"] == 1, after_win)

        foe_seen = encounter_for(me, foe)
        foe_seen = present(foe_seen)
        check(foe_seen["againstCount"] == 1, foe_seen)
        check(foe_seen["winsAgainst"] == 1, foe_seen)
        check(foe_seen["withCount"] == 0, foe_seen)
        check(foe_seen["drawsAgainst"] == 0, foe_seen)

        # A draw is a result like any other and is recorded for everyone in it.
        drawn = board("m3", "Blue")
        record_board(drawn)
        record_result(drawn, "Draw")
        after_draw = encounter_for(me, mate)
        after_draw = present(after_draw)
        check(after_draw["drawsWith"] == 1, after_draw)
        check(after_draw["winsWith"] == 1, after_draw)

        # Same account, other side of the map. The two tallies are separate:
        # two lobbies alongside by now, and this one against.
        record_board(board("m4", "Red"))
        swapped = encounter_for(me, mate)
        swapped = present(swapped)
        check(swapped["withCount"] == 2, swapped)
        check(swapped["againstCount"] == 1, swapped)

        # The timeline keeps each result in lowercase, in the order played.
        stored = present(get_one(me, mate))
        words = [item["result"] for item in stored["timeline"]]
        check(words == ["victory", "draw"], words)

        # Past games from History log teammates and opponents once, however
        # many times the screen is opened, and never the owner.
        game = {
            "matchId": "hist-1",
            "yourTeam": "Blue",
            "result": "Victory",
            "startedAt": 1_790_000_000_000,
            "players": [
                {"puuid": "owner-h", "team": "Blue", "name": "Me#1"},
                {"puuid": "mate-h", "team": "Blue", "name": "Mate#1", "agent": "Sova"},
                {"puuid": "foe-h", "team": "Red", "name": "Foe#1", "agent": "Jett"},
            ],
        }
        check(record_games("owner-h", [game]) == 2)
        check(record_games("owner-h", [game]) == 0)
        mate, foe = _players("owner-h")["mate-h"], _players("owner-h")["foe-h"]
        check((mate["withCount"], mate["winsWith"], mate["name"]) == (1, 1, "Mate#1"), mate)
        check((foe["againstCount"], foe["winsAgainst"], foe["agents"]) == (1, 1, ["Jett"]), foe)
        check("owner-h" not in _players("owner-h"))

        print("encounter_log self-check OK (one count per lobby, sides kept apart)")
    finally:
        mock.patch.stopall()


if __name__ == "__main__":
    _self_check()

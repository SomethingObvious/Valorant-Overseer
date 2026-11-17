# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""Everyone you have played with or against, and how those games went."""

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

_ROSTER_LIMIT = 200
# Most players are met once and never again, at about 560 bytes each, and the
# whole store is rewritten every match. Those go after a month.
_STRANGER_DAYS = 30
_ROSTER_MIN = 4
_LOCK = threading.RLock()


def _empty_store() -> dict[str, Any]:
    return {"version": 2, "accounts": {}}


def _load() -> dict[str, Any]:
    raw = read_json(_PATH, None)
    if isinstance(raw, dict) and raw.get("version") == 2 and isinstance(raw.get("accounts"), dict):
        _forget_strangers(raw, time.time())
        return raw
    return _empty_store()


def _forget_strangers(store: dict[str, Any], now: float) -> None:
    cutoff = now - _STRANGER_DAYS * 86400
    for account in store["accounts"].values():
        players = account.get("players") if isinstance(account, dict) else None
        if not isinstance(players, dict):
            continue
        for puuid, entry in list(players.items()):
            if not isinstance(entry, dict):
                continue
            try:
                met = int(entry.get("withCount") or 0) + int(entry.get("againstCount") or 0)
                seen = float(entry.get("lastSeen") or 0)
            except (TypeError, ValueError):
                continue
            if met <= 1 and seen < cutoff:
                del players[puuid]


def _save() -> None:
    write_atomic(_PATH, _STORE, prefix=".encounters-")


_STORE = _load()
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
    account = _account(owner)
    rosters = account.setdefault("rosters", [])
    if any(r.get("matchId") == match_id for r in rosters):
        return False
    teams = {
        p["puuid"]: p["team"]
        for p in board.get("players") or []
        if isinstance(p, dict) and p.get("puuid") and p.get("team")
    }
    if len(teams) < _ROSTER_MIN:
        return False
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


_REGULAR_OVER = 3
_MONTH = 30 * 86400


def regulars(owner: str | None, now: float | None = None) -> set[str]:
    """Return who `owner` keeps meeting: over three lobbies in a month, the last within one."""
    if not owner:
        return set()
    now = time.time() if now is None else now
    met: dict[str, list[float]] = {}
    with _LOCK:
        for roster in _account(str(owner)).get("rosters") or []:
            for puuid in roster.get("teams") or {}:
                met.setdefault(puuid, []).append(float(roster.get("at") or 0))
    met.pop(str(owner), None)
    return {
        puuid
        for puuid, times in met.items()
        if now - max(times) <= _MONTH
        and any(
            later - first <= _MONTH
            for first, later in zip(sorted(times), sorted(times)[_REGULAR_OVER:], strict=False)
        )
    }


_SAVED = {"at": 0}
_SEEN_EVERY = 60


def _others(board: Any) -> tuple[str, str, Any, list[dict[str, Any]]] | None:
    if not isinstance(board, dict) or board.get("source") != "local":
        return None
    owner, match_id, listed = board.get("selfPuuid"), board.get("matchId"), board.get("players")
    if not owner or not match_id or not isinstance(listed, list):
        return None
    others = [p for p in listed if isinstance(p, dict) and p.get("puuid") and not p.get("isSelf")]
    return owner, match_id, board.get("selfTeam"), others


def _side(player: dict[str, Any], ours: Any) -> str:
    return "With" if ours is not None and player.get("team") == ours else "Against"


def _bump(entry: dict[str, Any], key: str) -> None:
    entry[key] = int(entry.get(key) or 0) + 1


def _ids(entry: dict[str, Any], key: str, older: str) -> list[str]:
    ids = entry.setdefault(key, [])
    if entry.get(older) and entry[older] not in ids:
        ids.append(entry[older])
    return ids


def record_board(board: dict[str, Any] | None) -> None:
    """Log everyone on a live board as a teammate or an opponent of its owner."""
    seen = _others(board)
    if seen is None or board is None:
        return
    owner, match_id, ours, players = seen
    now = int(time.time())
    with _LOCK:
        store = _players(owner)
        counted = False
        for player in players:
            entry = store.setdefault(player["puuid"], _blank_entry(player["puuid"]))
            played = _ids(entry, "matchIds", "lastMatchId")
            if match_id != "lobby" and match_id not in played:
                _bump(entry, f"{_side(player, ours).lower()}Count")
                entry["matchIds"] = [*played, match_id][-80:]
                entry["lastMatchId"] = match_id
                counted = True
            entry.update({k: player[k] for k in _PROFILE_FIELDS if player.get(k) is not None})
            entry["lastSeen"] = now
            agent = player.get("agent")
            if agent and agent != "Unknown" and agent not in entry.setdefault("agents", []):
                entry["agents"] = [*entry["agents"], agent][-8:]
        counted = _record_roster(owner, match_id, board, now) or counted
        if counted or now - _SAVED["at"] >= _SEEN_EVERY:
            _save()
            _SAVED["at"] = now


def record_result(board: dict[str, Any] | None, result: str | None) -> None:
    """Record how a lobby ended for everyone in it, from Riot's Victory, Defeat or Draw."""
    outcome = (result or "").strip().lower()
    seen = _others(board) if outcome in _OUTCOMES else None
    if seen is None:
        return
    owner, match_id, ours, players = seen
    with _LOCK:
        store = _players(owner)
        counted = False
        for player in players:
            entry = store.get(player["puuid"])
            if not entry:
                continue
            settled = _ids(entry, "resultMatchIds", "lastResultMatchId")
            if match_id in settled:
                continue
            side = _side(player, ours)
            _bump(entry, _OUTCOMES[outcome] + side)
            entry["resultMatchIds"] = [*settled, match_id][-80:]
            entry["lastResultMatchId"] = match_id
            moment = {
                "matchId": match_id,
                "at": int(time.time()),
                "side": side.lower(),
                "result": outcome,
                "agent": player.get("agent"),
            }
            entry["timeline"] = [*entry.get("timeline", []), moment][-40:]
            counted = True
        if counted:
            _save()


def record_games(owner: str | None, games: list[dict[str, Any]] | None) -> int:
    """Log everybody in past games from the History screen."""
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
                if "#" in str(player.get("name") or ""):
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


def teammates(owner: str | None) -> list[dict[str, Any]]:
    """Return the accounts most often on your side, for a career's Played With."""
    if not owner:
        return []

    def rate(entry: dict[str, Any]) -> int | None:
        wins = int(entry.get("winsWith") or 0)
        done = wins + int(entry.get("lossesWith") or 0) + int(entry.get("drawsWith") or 0)
        return round(100 * wins / done) if done else None

    with _LOCK:
        rows = [
            {
                "puuid": entry.get("puuid"),
                "name": entry.get("name"),
                "sharedMatches": int(entry.get("withCount") or 0),
                "winRate": rate(entry),
                "agents": [],
            }
            for entry in _players(owner).values()
            if int(entry.get("withCount") or 0) >= 2 and entry.get("name")
        ]
    rows.sort(key=lambda row: -row["sharedMatches"])
    return rows[:5]


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


def _self_check() -> None:
    import tempfile
    from unittest import mock

    mock.patch(f"{__name__}._STORE", _empty_store()).start()
    mock.patch(f"{__name__}._save").start()
    try:
        with tempfile.TemporaryDirectory() as tmp:
            mock.patch(f"{__name__}._PATH", str(Path(tmp) / "encounters.json")).start()
            Path(_PATH).write_text('{"version": 2, "accou', encoding="utf-8")
            check(_load() == _empty_store())
            check(not Path(_PATH).exists())
            kept = list(Path(tmp).glob("encounters.json.unreadable-*"))
            check([k.read_text(encoding="utf-8") for k in kept] == ['{"version": 2, "accou'], kept)

        day = 86400
        old = {"withCount": 1, "lastSeen": 0}
        aged = {"accounts": {"a": {"players": {"once": old, "twice": {**old, "againstCount": 1}}}}}
        _forget_strangers(aged, _STRANGER_DAYS * day + day)
        check(list(aged["accounts"]["a"]["players"]) == ["twice"], aged)

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
        record_board(first)
        record_board(first)

        seen = present(encounter_for(me, mate))
        check(seen["withCount"] == 1, seen)
        check(seen["againstCount"] == 0, seen)

        record_result(first, "Victory")
        record_result(first, "Victory")
        after_win = present(encounter_for(me, mate))
        check(after_win["winsWith"] == 1, after_win)

        foe_seen = present(encounter_for(me, foe))
        check(foe_seen["againstCount"] == 1, foe_seen)
        check(foe_seen["winsAgainst"] == 1, foe_seen)
        check(foe_seen["withCount"] == 0, foe_seen)

        drawn = board("m3", "Blue")
        record_board(drawn)
        record_result(drawn, "Draw")
        after_draw = present(encounter_for(me, mate))
        check(after_draw["drawsWith"] == 1, after_draw)
        check(after_draw["winsWith"] == 1, after_draw)

        record_board(board("m4", "Red"))
        swapped = present(encounter_for(me, mate))
        check(swapped["withCount"] == 2, swapped)
        check(swapped["againstCount"] == 1, swapped)

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

        day = 86400

        def lobbies(owner: str, who: str, days: list[int]) -> None:
            _account(owner)["rosters"] = [
                {"matchId": f"r{at}", "at": at * day, "teams": {owner: "Blue", who: "Red"}}
                for at in days
            ]

        lobbies("owner-r", "often", [1, 9, 20, 29])
        check(regulars("owner-r", now=40 * day) == {"often"})
        check(regulars("owner-r", now=58 * day) == {"often"}, "a month from the last time")
        check(regulars("owner-r", now=60 * day) == set(), "kept past a month")
        lobbies("owner-r", "thrice", [1, 9, 20])
        check(regulars("owner-r", now=21 * day) == set(), "three is not more than three")
        lobbies("owner-r", "rarely", [1, 20, 40, 60])
        check(regulars("owner-r", now=61 * day) == set(), "four over two months")
        check(regulars(None) == set())

        print("encounter_log self-check OK (one count per lobby, sides kept apart)")
    finally:
        mock.patch.stopall()


if __name__ == "__main__":
    _self_check()

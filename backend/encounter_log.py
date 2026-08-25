from __future__ import annotations

import json
import threading
import time
from pathlib import Path
from typing import Any

from common import data_path, write_atomic

_PATH = data_path("encounters.json")
# Two hundred lobbies is a few months of play and about a hundred kilobytes.
# Older ones say little, because people stop queueing together.
# Riot's three words for how a match ended, and the counter each one feeds.
# A draw is rare and real: it used to be dropped, taking the whole lobby with
# it, because the caller could only say won or lost.
_OUTCOMES = {"victory": "wins", "defeat": "losses", "draw": "draws"}

_ROSTER_LIMIT = 200
_ROSTER_MIN = 4
_LOCK = threading.RLock()


def _empty_store() -> dict[str, Any]:
    return {"version": 2, "accounts": {}, "discardedLegacyPlayers": 0}


def _quarantine() -> None:
    """Move a store that would not parse aside, rather than writing over it.

    It is the only copy of every account this machine has ever seen, and the
    reason it will not parse may be nothing worse than a half finished write or
    a byte order mark. Keeping it costs a file; overwriting it costs the lot.
    """
    src = Path(_PATH)
    for n in range(1, 100):
        kept = src.with_name(f"{src.stem}.unreadable{'' if n == 1 else n}.json")
        if kept.exists():
            continue
        try:
            src.replace(kept)
        except Exception:
            return
        return


def _load() -> dict[str, Any]:
    try:
        with Path(_PATH).open(encoding="utf-8") as fh:
            raw = json.load(fh)
        if (
            isinstance(raw, dict)
            and raw.get("version") == 2
            and isinstance(raw.get("accounts"), dict)
        ):
            return raw
        out = _empty_store()
        if isinstance(raw, dict):
            out["discardedLegacyPlayers"] = sum(isinstance(v, dict) for v in raw.values())
        return out
    except Exception:
        # Unreadable rather than merely old. The legacy branch above is a
        # migration and is meant to be written over; this is not, and the
        # _save() below would do exactly that within a millisecond of import.
        _quarantine()
        return _empty_store()


def _save() -> None:
    write_atomic(_PATH, _STORE, prefix=".encounters-")


_STORE = _load()
# Only to bring the file into existence. Saving unconditionally here is what
# turned an unreadable store into a deleted one.
if not Path(_PATH).exists():
    _save()


def _public_entry(source: dict[str, Any]) -> dict[str, Any]:
    row = dict(source)

    # Totals across both sides. The views want "how often have I seen this
    # person and how did those go", not six separate counters, and they were
    # reading names that nothing produced: every row drew 0x 0W 0L while the
    # store underneath held the real numbers all along.
    def total(*keys: str) -> int:
        return sum(int(source.get(k) or 0) for k in keys)

    row["seen"] = total("withCount", "againstCount")
    row["wins"] = total("winsWith", "winsAgainst")
    row["losses"] = total("lossesWith", "lossesAgainst")
    row["draws"] = total("drawsWith", "drawsAgainst")
    stats = source.get("withStats") or {}
    games = int(stats.get("games") or 0)
    deaths = int(stats.get("deaths") or 0)
    hits = int(stats.get("shotsHit") or 0)
    if games:
        kills = int(stats.get("kills") or 0)
        row["withKd"] = round(kills / deaths, 2) if deaths else float(kills)
        row["withAcs"] = round(float(stats.get("acsTotal") or 0) / games)
        row["withHsPct"] = round(100 * int(stats.get("headshots") or 0) / hits) if hits else None
        row["withStatGames"] = games
    agent_counts = source.get("agentCounts") or {}
    if agent_counts:
        top_agent = max(agent_counts, key=lambda name: (int(agent_counts.get(name) or 0), name))
        row["topAgent"] = top_agent
        row["topAgentGames"] = int(agent_counts.get(top_agent) or 0)
        row["topAgentPortrait"] = (source.get("agentPortraits") or {}).get(top_agent)
        row["topAgentColor"] = (source.get("agentColors") or {}).get(top_agent)
    return row


def _account(owner: str) -> dict[str, Any]:
    return _STORE.setdefault("accounts", {}).setdefault(str(owner), {"players": {}})


def _players(owner: str) -> dict[str, Any]:
    return _account(owner).setdefault("players", {})


def _record_roster(owner: str, match_id: str, board: dict[str, Any], now: int) -> bool:
    """Remember who was on which side, which is what a stack is guessed from.

    The per-player counts above are all relative to me: they say how often I
    have been with or against someone. Working out that two other people queue
    together needs the sides of the lobby as they actually were.
    """
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
    # Both sides, or none. Agent select shows the ally team only, so a pregame
    # board is five accounts that are all on one team by construction: every
    # pair in it shares a side, whoever they are. Storing that as evidence of
    # queueing together turns four strangers into a five stack, which is
    # exactly what it did.
    if len(set(teams.values())) < 2:
        return False
    rosters.append({"matchId": match_id, "at": now, "teams": teams})
    account["rosters"] = rosters[-_ROSTER_LIMIT:]
    return True


def rosters_for(owner: str | None) -> list[dict[str, Any]]:
    """The stored lobbies, newest last. Copied, so a caller cannot edit history."""
    if not owner:
        return []
    with _LOCK:
        return [dict(r) for r in _account(str(owner)).get("rosters") or []]


def record_board(board: dict[str, Any] | None) -> None:
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
            entry = store.setdefault(
                puuid,
                {
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
                },
            )
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
            for key in (
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
            ):
                if player.get(key) is not None:
                    entry[key] = player.get(key)
            entry["lastSeen"] = now
            agent = player.get("agent")
            if agent and agent != "Unknown" and agent not in entry.setdefault("agents", []):
                entry["agents"].append(agent)
                entry["agents"] = entry["agents"][-8:]
            changed = True
        if _record_roster(owner, match_id, board, now):
            changed = True
        if changed:
            _save()


def record_result(board: dict[str, Any] | None, result: str | None) -> None:
    """Record how a lobby ended for everyone in it.

    `result` is Riot's own word for it: Victory, Defeat or Draw. It used to be
    a bool, which had no way to say Draw and dropped those matches on the floor
    along with everyone who played in them.
    """
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


def backfill_career(owner: str | None, matches: list[dict[str, Any]] | None) -> int:
    if not owner or not isinstance(matches, list):
        return 0
    changed = 0
    dirty = False
    with _LOCK:
        store = _players(owner)
        for match in matches:
            if not isinstance(match, dict) or not match.get("matchId"):
                continue
            match_id = str(match["matchId"])
            result = match.get("result")
            seen_at = int((match.get("startMillis") or 0) / 1000) or int(time.time())
            for teammate in match.get("teammates") or []:
                if not isinstance(teammate, dict) or not teammate.get("puuid"):
                    continue
                puuid = str(teammate["puuid"])
                entry = store.setdefault(
                    puuid,
                    {
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
                    },
                )
                match_ids = entry.setdefault("matchIds", [])
                for legacy in (entry.get("lastMatchId"),):
                    if legacy and legacy not in match_ids:
                        match_ids.append(legacy)
                if match_id not in match_ids:
                    entry["withCount"] = int(entry.get("withCount") or 0) + 1
                    match_ids.append(match_id)
                    entry["matchIds"] = match_ids[-80:]
                    changed += 1
                    dirty = True

                result_ids = entry.setdefault("resultMatchIds", [])
                legacy_result = entry.get("lastResultMatchId")
                if legacy_result and legacy_result not in result_ids:
                    result_ids.append(legacy_result)
                if result in ("Victory", "Defeat") and match_id not in result_ids:
                    key = "winsWith" if result == "Victory" else "lossesWith"
                    entry[key] = int(entry.get(key) or 0) + 1
                    result_ids.append(match_id)
                    entry["resultMatchIds"] = result_ids[-80:]
                    dirty = True

                if teammate.get("name") and entry.get("name") != teammate["name"]:
                    entry["name"] = teammate["name"]
                    dirty = True
                for key in ("rank", "peakRank", "kd", "winRate", "level"):
                    if teammate.get(key) is not None and entry.get(key) != teammate.get(key):
                        entry[key] = teammate.get(key)
                        dirty = True
                previous_seen = int(entry.get("lastSeen") or 0)
                entry["lastSeen"] = max(previous_seen, seen_at)
                dirty = dirty or entry["lastSeen"] != previous_seen
                entry["lastMatchId"] = match_id
                if result in ("Victory", "Defeat"):
                    entry["lastResultMatchId"] = match_id
                agent = teammate.get("agent")
                if agent and agent != "Unknown" and agent not in entry.setdefault("agents", []):
                    entry["agents"].append(agent)
                    entry["agents"] = entry["agents"][-8:]
                    dirty = True
                stat_match_ids = entry.setdefault("withStatMatchIds", [])
                if match_id not in stat_match_ids:
                    stats = entry.setdefault(
                        "withStats",
                        {
                            "games": 0,
                            "kills": 0,
                            "deaths": 0,
                            "assists": 0,
                            "acsTotal": 0,
                            "shotsHit": 0,
                            "headshots": 0,
                        },
                    )
                    stats["games"] = int(stats.get("games") or 0) + 1
                    for field in ("kills", "deaths", "assists", "shotsHit", "headshots"):
                        stats[field] = int(stats.get(field) or 0) + int(teammate.get(field) or 0)
                    stats["acsTotal"] = float(stats.get("acsTotal") or 0) + float(
                        teammate.get("acs") or 0
                    )
                    stat_match_ids.append(match_id)
                    entry["withStatMatchIds"] = stat_match_ids[-80:]
                    if agent and agent != "Unknown":
                        counts = entry.setdefault("agentCounts", {})
                        counts[agent] = int(counts.get(agent) or 0) + 1
                        if teammate.get("agentPortrait"):
                            portraits = entry.setdefault("agentPortraits", {})
                            portraits[agent] = teammate["agentPortrait"]
                        if teammate.get("agentColor"):
                            entry.setdefault("agentColors", {})[agent] = teammate["agentColor"]
                    dirty = True
                timeline = entry.setdefault("timeline", [])
                if not any(item.get("matchId") == match_id for item in timeline):
                    timeline.append(
                        {
                            "matchId": match_id,
                            "at": seen_at,
                            "side": "with",
                            "result": "win"
                            if result == "Victory"
                            else "loss"
                            if result == "Defeat"
                            else None,
                            "agent": agent,
                            "map": match.get("map"),
                        }
                    )
                    entry["timeline"] = sorted(timeline, key=lambda item: item.get("at") or 0)[-40:]
                    dirty = True
        if dirty:
            _save()
    return changed


def enrich_player(owner: str | None, puuid: str | None, fields: dict[str, Any] | None) -> None:
    if not owner or not puuid or not isinstance(fields, dict):
        return
    with _LOCK:
        entry = _players(owner).get(str(puuid))
        if not entry:
            return
        dirty = False
        for key in (
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
        ):
            if fields.get(key) is not None and entry.get(key) != fields.get(key):
                entry[key] = fields.get(key)
                dirty = True
        if dirty:
            _save()


def get_all(owner: str | None, limit: int = 200) -> list[dict[str, Any]]:
    if not owner:
        return []
    with _LOCK:
        entries = [_public_entry(entry) for entry in _players(owner).values()]
    entries.sort(
        reverse=True,
        key=lambda entry: int(entry.get("withCount") or 0) + int(entry.get("againstCount") or 0),
    )
    return entries[:limit] if limit is not None and limit >= 0 else entries


def account_count() -> int:
    with _LOCK:
        return sum(
            1
            for account in _STORE.get("accounts", {}).values()
            if isinstance(account, dict) and account.get("players")
        )


def get_all_accounts(limit: int = 200) -> list[dict[str, Any]]:
    merged: dict[str, dict[str, Any]] = {}
    counter_keys = (
        "withCount",
        "againstCount",
        "winsWith",
        "lossesWith",
        "winsAgainst",
        "lossesAgainst",
    )
    with _LOCK:
        accounts = list((_STORE.get("accounts") or {}).items())
        for owner, account in accounts:
            for puuid, source in ((account or {}).get("players") or {}).items():
                row = merged.setdefault(puuid, {"puuid": puuid, "accountsSeen": []})
                if owner not in row["accountsSeen"]:
                    row["accountsSeen"].append(owner)
                for key in counter_keys:
                    row[key] = int(row.get(key) or 0) + int(source.get(key) or 0)
                if int(source.get("lastSeen") or 0) >= int(row.get("lastSeen") or 0):
                    for key in (
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
                        "lastSeen",
                        "lastMatchId",
                    ):
                        if source.get(key) is not None:
                            row[key] = source.get(key)
                row["agents"] = list(
                    dict.fromkeys((row.get("agents") or []) + (source.get("agents") or []))
                )[-8:]
                row["timeline"] = sorted(
                    (row.get("timeline") or []) + (source.get("timeline") or []),
                    key=lambda item: item.get("at") or 0,
                )[-40:]
                source_stats = source.get("withStats") or {}
                stats = row.setdefault("withStats", {})
                for key in (
                    "games",
                    "kills",
                    "deaths",
                    "assists",
                    "acsTotal",
                    "shotsHit",
                    "headshots",
                ):
                    stats[key] = float(stats.get(key) or 0) + float(source_stats.get(key) or 0)
                counts = row.setdefault("agentCounts", {})
                for agent, count in (source.get("agentCounts") or {}).items():
                    counts[agent] = int(counts.get(agent) or 0) + int(count or 0)
                row["agentPortraits"] = {
                    **(row.get("agentPortraits") or {}),
                    **(source.get("agentPortraits") or {}),
                }
                row["agentColors"] = {
                    **(row.get("agentColors") or {}),
                    **(source.get("agentColors") or {}),
                }
    entries = [_public_entry(entry) for entry in merged.values()]
    entries.sort(
        key=lambda entry: int(entry.get("withCount") or 0) + int(entry.get("againstCount") or 0),
        reverse=True,
    )
    return entries[:limit] if limit is not None and limit >= 0 else entries


def get_one(owner: str | None, puuid: str | None) -> dict[str, Any] | None:
    if not owner or not puuid:
        return None
    with _LOCK:
        entry = _players(owner).get(puuid)
        return dict(entry) if entry else None


def encounter_for(owner: str | None, puuid: str | None) -> dict[str, Any] | None:
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
    # This check runs against a store of its own and must never write over the
    # real encounter history, so both the store and the writer are swapped for
    # the duration and put back in the finally.
    global _STORE, _save
    saved_store, saved_save = _STORE, _save
    _STORE = _empty_store()
    _save = _no_save
    try:
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
        assert seen is not None
        assert seen["withCount"] == 1, seen
        assert seen["againstCount"] == 0, seen

        record_result(first, "Victory")
        record_result(first, "Victory")
        after_win = encounter_for(me, mate)
        assert after_win is not None
        assert after_win["winsWith"] == 1, after_win

        foe_seen = encounter_for(me, foe)
        assert foe_seen is not None
        assert foe_seen["againstCount"] == 1, foe_seen
        assert foe_seen["winsAgainst"] == 1, foe_seen
        assert foe_seen["withCount"] == 0, foe_seen
        assert foe_seen["drawsAgainst"] == 0, foe_seen

        # A draw is a result like any other, and used to be dropped entirely:
        # the caller could only say won or lost, so a drawn match recorded
        # nothing for anyone who played in it.
        drawn = board("m3", "Blue")
        record_board(drawn)
        record_result(drawn, "Draw")
        after_draw = encounter_for(me, mate)
        assert after_draw is not None
        assert after_draw["drawsWith"] == 1, after_draw
        assert after_draw["winsWith"] == 1, after_draw

        # And the totals the views read, which named fields nothing produced.
        row = next(r for r in get_all(me) if r.get("puuid") == mate)
        assert row["seen"] == row["withCount"] + row["againstCount"], row
        assert row["wins"] == 1 and row["draws"] == 1, row

        # Same account, other side of the map. The two tallies are separate:
        # two lobbies alongside by now, and this one against.
        record_board(board("m4", "Red"))
        swapped = encounter_for(me, mate)
        assert swapped is not None
        assert swapped["withCount"] == 2, swapped
        assert swapped["againstCount"] == 1, swapped

        print("encounter_log self-check OK (one count per lobby, sides kept apart)")
    finally:
        _STORE, _save = saved_store, saved_save


if __name__ == "__main__":
    _self_check()

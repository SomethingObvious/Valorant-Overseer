# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""Today's ranked session for the board's footer, and each match's result as it ends.

A session is the run of competitive matches with no gap of two hours or more
between them, read from Riot's own list of your ranked games. That list has
the games played while Overseer was closed, and the ones whose details Riot
was slow to post, which watching a match end can miss.
"""

from __future__ import annotations

import threading
import time
from typing import Any

import encounter_log
import history
import overseerlog
from common import data_path, read_json, write_atomic
from common.check import check
from vconstants import ladder_place, rr_moved

LOG = overseerlog.get_logger("backend")

_PATH = data_path("sessions.json")
_LOCK = threading.RLock()
_GAP = 2 * 3600
# Riot dates a match by when it started, and a ranked game runs about forty
# minutes, so the gap is measured from a start plus this.
_GAME = 45 * 60
_MAX_POINTS = 40
# How often Riot's list is read again, and when after a match ends, since it
# posts the match's rating a little after.
_REFRESH = 120
_AFTER_MATCH = (15, 45, 120)
# Retries for a match's details, which Riot is slow to post too.
_DETAIL_WAITS = (0, 20, 60)

_STATE: dict[str, Any] = {
    "prev_state": None,
    "ingame_board": None,
    "recorded": set(),
    "fetching": False,
}
# Riot's competitive updates for each account, and when they were read.
_UPDATES: dict[str, tuple[float, list[dict[str, Any]]]] = {}
# Where each account stood on the ladder and when it was read. A dodge costs
# rating without a match to show for it, so only a fresh read catches it.
_STANDING: dict[str, tuple[int, float]] = {}
# Riot takes a moment to charge a dodge.
_DODGE_WAIT = 20


def _load() -> dict[str, list[dict[str, Any]]]:
    raw = read_json(_PATH, None)
    accounts = raw.get("accounts") if isinstance(raw, dict) else None
    out: dict[str, list[dict[str, Any]]] = {}
    for puuid, account in (accounts or {}).items():
        if not isinstance(account, dict):
            continue
        points = account.get("points")
        if isinstance(points, list):
            out[str(puuid)] = [p for p in points if isinstance(p, dict)][-_MAX_POINTS:]
    return out


_POINTS = _load()


def _save() -> None:
    store = {"version": 3, "accounts": {k: {"points": v} for k, v in _POINTS.items()}}
    write_atomic(_PATH, store, prefix=".sessions-")


def session(
    matches: list[dict[str, Any]], results: dict[str, Any], now: float
) -> list[dict[str, Any]]:
    """Return the session's matches as points, oldest first.

    `matches` are Riot's competitive updates. `results` are the results seen
    when matches ended, by match id, since Riot's list has none and a draw
    can move the rating either way.
    """
    played = sorted(
        (
            m
            for m in matches
            if m.get("MatchID")
            and m.get("MapID")
            and isinstance(m.get("MatchStartTime"), (int, float))
        ),
        key=lambda m: float(m["MatchStartTime"]),
    )
    kept: list[dict[str, Any]] = []
    later = now
    for match in reversed(played):
        at = float(match["MatchStartTime"]) / 1000
        if later - at >= _GAP + _GAME:
            break
        kept.append(match)
        later = at
    points = []
    for match in reversed(kept):
        earned = match.get("RankedRatingEarned")
        guess = None
        if isinstance(earned, int) and earned:
            guess = "Victory" if earned > 0 else "Defeat"
        points.append(
            {
                "matchId": match["MatchID"],
                "ts": int(float(match["MatchStartTime"]) / 1000),
                "result": results.get(match["MatchID"]) or guess,
                "delta": earned,
                "actual": rr_moved(match),
                "tier": match.get("TierAfterUpdate"),
                "rr": match.get("RankedRatingAfterUpdate"),
            }
        )
    return points


def _fetch(lm: Any, waits: tuple[float, ...] = (0,)) -> None:
    """Read Riot's list of your ranked games, then where you stand now."""
    try:
        for wait in waits:
            time.sleep(wait)
            me = str(lm.self_puuid)
            asked = time.time()
            try:
                cu = lm.auth.pd_get(
                    f"/mmr/v1/players/{me}/competitiveupdates"
                    f"?startIndex=0&endIndex=20&queue=competitive"
                )
            except Exception as e:
                LOG.warning("competitive updates lookup failed: %r", e)
                continue
            matches = cu.get("Matches") if isinstance(cu, dict) else None
            if not isinstance(matches, list):
                continue
            # Read after the list, so it is never older than the last match.
            _read_standing(lm)
            with _LOCK:
                _UPDATES[me] = (asked, [m for m in matches if isinstance(m, dict)])
    finally:
        _STATE["fetching"] = False


def _refresh(lm: Any, waits: tuple[float, ...] = (0,), *, force: bool = False) -> None:
    """Read Riot's list on its own thread, when it is due or `force` says so."""
    me = str(getattr(lm, "self_puuid", None) or "")
    if not me or (_STATE["fetching"] and not force):
        return
    with _LOCK:
        fetched = _UPDATES.get(me, (0.0, []))[0]
    if not force and time.time() - fetched < _REFRESH:
        return
    _STATE["fetching"] = True
    threading.Thread(target=_fetch, args=(lm, waits), daemon=True, name="session").start()


def _self_rr(lm: Any, match_id: str) -> dict[str, Any]:
    try:
        cu = lm.auth.pd_get(
            f"/mmr/v1/players/{lm.self_puuid}/competitiveupdates"
            f"?startIndex=0&endIndex=5&queue=competitive"
        )
        for match in (cu or {}).get("Matches", []) or []:
            if match.get("MatchID") == match_id:
                return {
                    "delta": match.get("RankedRatingEarned"),
                    "actual": rr_moved(match),
                    "tier": match.get("TierAfterUpdate"),
                    "rr": match.get("RankedRatingAfterUpdate"),
                }
    except Exception as e:
        LOG.warning("rr lookup for match %s failed: %r", match_id, e)
    return {}


def _point(lm: Any, board: dict[str, Any]) -> dict[str, Any] | None:
    """Return the match a board was showing as one point, once it has ended."""
    match_id = board.get("matchId")
    if not match_id or match_id == "lobby":
        return None
    detail = lm.match_detail(match_id, lm.self_puuid)
    if not isinstance(detail, dict) or detail.get("error"):
        return None
    you = next((p for p in detail.get("players") or [] if p.get("isSubject")), None)
    if not you:
        return None
    competitive = (board.get("mode") or "").lower() == "competitive"
    rr = _self_rr(lm, match_id) if competitive else {}
    return {
        "matchId": match_id,
        "puuid": lm.self_puuid,
        "ts": int(time.time()),
        "map": detail.get("map"),
        "mode": detail.get("mode"),
        "result": detail.get("result"),
        "scores": detail.get("scores"),
        "delta": rr.get("delta"),
        "actual": rr.get("actual"),
        "tier": rr.get("tier"),
        "rr": rr.get("rr"),
        **{k: you.get(k) for k in ("agent", "kills", "deaths", "assists", "kd", "acs", "hsPct")},
        "resultExact": True,
    }


def _record(lm: Any, board: dict[str, Any]) -> None:
    try:
        point = None
        for wait in _DETAIL_WAITS:
            time.sleep(wait)
            point = _point(lm, board)
            competitive = point and (point.get("mode") or "").lower() == "competitive"
            if point and (not competitive or point.get("delta") is not None):
                break
        if not point:
            LOG.warning("match %s never had details to record", board.get("matchId"))
            return
        encounter_log.record_result(board, point.get("result"))
        history.record(point)
        if (point.get("mode") or "").lower() != "competitive":
            return
        with _LOCK:
            points = _POINTS.setdefault(str(point["puuid"]), [])
            if all(p.get("matchId") != point["matchId"] for p in points):
                points.append(point)
                del points[:-_MAX_POINTS]
                _save()
            at = ladder_place(point.get("tier"), point.get("rr"))
            if at is not None:
                _STANDING[str(point["puuid"])] = (at, time.time())
    except Exception:
        LOG.exception("recording match %s failed", board.get("matchId"))


def _read_standing(lm: Any, wait: float = 0) -> None:
    """Read where you stand on the ladder now, from your current act's rank."""
    time.sleep(wait)
    try:
        season = lm.season_id()
        r = lm.auth.pd_get(f"/mmr/v1/players/{lm.self_puuid}")
        acts = (((r or {}).get("QueueSkills") or {}).get("competitive") or {}).get(
            "SeasonalInfoBySeasonID"
        ) or {}
        cur = (acts.get(season) or {}) if season else {}
        at = ladder_place(cur.get("CompetitiveTier"), cur.get("RankedRating"))
        if at is not None:
            with _LOCK:
                _STANDING[str(lm.self_puuid)] = (at, time.time())
    except Exception as e:
        LOG.warning("standing lookup failed: %r", e)


def observe(board: dict[str, Any], lm: Any) -> None:
    """Follow a board's state, and record a match's result once it ends."""
    state = board.get("state")
    prev, _STATE["prev_state"] = _STATE["prev_state"], state
    _refresh(lm)
    if prev == "PREGAME" and state == "MENUS":
        threading.Thread(
            target=_read_standing, args=(lm, _DODGE_WAIT), daemon=True, name="standing"
        ).start()
    if state == "INGAME" and board.get("matchId"):
        _STATE["ingame_board"] = board
        return
    snap, _STATE["ingame_board"] = _STATE["ingame_board"], None
    if state != "MENUS" or prev != "INGAME" or not snap:
        return
    _refresh(lm, _AFTER_MATCH, force=True)
    key = (getattr(lm, "self_puuid", None), snap.get("matchId"))
    with _LOCK:
        if key in _STATE["recorded"]:
            return
        _STATE["recorded"].add(key)
    # The match's details take a few calls to Riot, so the board doesn't wait.
    name = f"record-{str(key[1])[:8]}"
    threading.Thread(target=_record, args=(lm, snap), daemon=True, name=name).start()


def attach(board: dict[str, Any]) -> dict[str, Any]:
    """Add today's session to a board."""
    me = str(board.get("selfPuuid"))
    with _LOCK:
        fetched, matches = _UPDATES.get(me, (0.0, []))
        results = {str(p.get("matchId")): p.get("result") for p in _POINTS.get(me, [])}
        standing = _STANDING.get(me)
    points = session(matches, results, time.time())
    # A read from before Riot's list could be missing its newest match.
    fresh = standing[0] if standing and standing[1] >= fetched else None
    deltas = [p["delta"] for p in points if isinstance(p.get("delta"), (int, float))]
    board["session"] = {"net": sum(deltas), "actual": actual(points, fresh), "points": points}
    return board


def actual(points: list[dict[str, Any]], standing: int | None = None) -> int | None:
    """How far the session really moved the rating, or None for one too old to say.

    It runs from where the first match started to where the rating stands
    now, so a dodge between or after the matches counts too.
    """
    if not points:
        return None
    first, last = points[0], points[-1]
    after_first = ladder_place(first.get("tier"), first.get("rr"))
    moved = first.get("actual")
    if after_first is None or not isinstance(moved, int):
        return None
    end = ladder_place(last.get("tier"), last.get("rr")) if standing is None else standing
    return None if end is None else end - (after_first - moved)


if __name__ == "__main__":
    hour = 3600
    games = ((0, 20), (5, -15), (6, 18), (7, 21))
    played = [
        {"MatchID": f"m{t}", "MapID": "/Game/Maps/Ascent", "MatchStartTime": t * hour * 1000}
        | {"RankedRatingEarned": d}
        for t, d in games
    ]
    played.append({"MatchID": "dodge", "MapID": "", "MatchStartTime": 7 * hour * 1000})
    now_ids = [p["matchId"] for p in session(played, {}, 8 * hour)]
    check(now_ids == ["m5", "m6", "m7"], now_ids)
    # Two hours idle after a game's length, the session has ended.
    check(session(played, {}, 7 * hour + 2 * hour + _GAME) == [])
    check(session(played[::-1], {}, 8 * hour)[0]["matchId"] == "m5", "any order")
    seen = session(played, {"m7": "Draw"}, 8 * hour)
    check([p["result"] for p in seen] == ["Defeat", "Victory", "Draw"], seen)
    # A rank up keeps the whole gain, derank protection keeps the rating
    # where it was, and Immortal counts on without starting over.
    up = {"TierBeforeUpdate": 12, "RankedRatingBeforeUpdate": 90}
    check(rr_moved({**up, "TierAfterUpdate": 13, "RankedRatingAfterUpdate": 10}) == 20)
    held = {"TierBeforeUpdate": 13, "RankedRatingBeforeUpdate": 0}
    check(rr_moved({**held, "TierAfterUpdate": 13, "RankedRatingAfterUpdate": 0}) == 0)
    top = {"TierBeforeUpdate": 24, "RankedRatingBeforeUpdate": 95}
    check(rr_moved({**top, "TierAfterUpdate": 25, "RankedRatingAfterUpdate": 118}) == 23)
    check(rr_moved({"TierAfterUpdate": 13, "RankedRatingAfterUpdate": 10}) is None)
    # Gold 1 at 50, won 20, dodged for 3, then lost 15 and dodged again.
    won = {"ts": 100, "tier": 12, "rr": 70, "actual": 20}
    lost = {"ts": 200, "tier": 12, "rr": 52, "actual": -15}
    check(actual([won]) == 20)
    check(actual([won, lost]) == 2, "a dodge between matches counts")
    check(actual([won, lost], 1249) == -1, "a dodge after the last match counts")
    check(actual([{"ts": 1, "delta": -18}]) is None, "an older point can't say")
    check(actual([]) is None)
    print("session_tracker self-check OK")

# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""Today's ranked session for the board's footer, and each match's result as it ends.

A session is the run of competitive matches with no gap of six hours or more
between them, so it starts over on a new day of play.
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

LOG = overseerlog.get_logger("backend")

_PATH = data_path("sessions.json")
_LOCK = threading.RLock()
_GAP = 6 * 3600
_MAX_POINTS = 40

_STATE: dict[str, Any] = {"prev_state": None, "ingame_board": None, "recorded": set()}


def _load() -> dict[str, list[dict[str, Any]]]:
    raw = read_json(_PATH, None)
    accounts = raw.get("accounts") if isinstance(raw, dict) else None
    out: dict[str, list[dict[str, Any]]] = {}
    for puuid, account in (accounts or {}).items():
        if not isinstance(account, dict):
            continue
        # Files from older versions keep the points under "active".
        points = account.get("points", (account.get("active") or {}).get("points"))
        if isinstance(points, list):
            out[str(puuid)] = [p for p in points if isinstance(p, dict)][-_MAX_POINTS:]
    return out


_POINTS = _load()


def _save() -> None:
    store = {"version": 3, "accounts": {k: {"points": v} for k, v in _POINTS.items()}}
    write_atomic(_PATH, store, prefix=".sessions-")


def today(points: list[dict[str, Any]], now: float) -> list[dict[str, Any]]:
    """Return the points since the last gap of six hours, oldest first."""
    kept: list[dict[str, Any]] = []
    later = now
    for point in reversed(points):
        at = float(point.get("ts") or 0)
        if later - at >= _GAP:
            break
        kept.append(point)
        later = at
    return kept[::-1]


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
        "tier": rr.get("tier"),
        "rr": rr.get("rr"),
        **{k: you.get(k) for k in ("agent", "kills", "deaths", "assists")},
        **{k: you.get(k) for k in ("kd", "acs", "hsPct")},
        "resultExact": True,
    }


def _record(lm: Any, board: dict[str, Any]) -> None:
    try:
        point = _point(lm, board)
        if not point:
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
    except Exception:
        LOG.exception("recording match %s failed", board.get("matchId"))


def observe(board: dict[str, Any], lm: Any) -> None:
    """Follow a board's state, and record a match's result once it ends."""
    state = board.get("state")
    prev, _STATE["prev_state"] = _STATE["prev_state"], state
    if state == "INGAME" and board.get("matchId"):
        _STATE["ingame_board"] = board
        return
    snap, _STATE["ingame_board"] = _STATE["ingame_board"], None
    if state != "MENUS" or prev != "INGAME" or not snap:
        return
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
    with _LOCK:
        points = today(_POINTS.get(str(board.get("selfPuuid")), []), time.time())
    deltas = [p["delta"] for p in points if isinstance(p.get("delta"), (int, float))]
    board["session"] = {"net": sum(deltas), "points": points}
    return board


if __name__ == "__main__":
    hour = 3600
    played = [{"ts": t * hour, "delta": d} for t, d in ((0, 20), (10, -15), (11, 18), (13, 21))]
    check(today(played, 14 * hour) == played[1:], today(played, 14 * hour))
    # Six hours on from the last match, the session has ended.
    check(today(played, 19 * hour) == [], today(played, 19 * hour))
    check(today([], 0) == [], "an account with no matches")
    print("session_tracker self-check OK")

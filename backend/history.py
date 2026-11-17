# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""Your ranked rating over time, per account, for the History screen."""

from __future__ import annotations

import threading
import time
from collections import Counter
from typing import Any

import overseerlog
from common import data_path, read_json, write_atomic
from common.check import check
from vconstants import map_name_from_path

LOG = overseerlog.get_logger("history")

_PATH = data_path("rr_history.json")
_LOCK = threading.RLock()
_MAX_POINTS = 2000
_REFRESH_TTL = 600.0
_refresh_at: dict[str, float] = {}


def _load(raw: Any) -> dict[str, Any]:
    if isinstance(raw, dict) and raw.get("version") == 3 and isinstance(raw.get("accounts"), dict):
        return raw
    if raw is not None:
        LOG.warning("%s parsed, but not as rr history, starting empty", _PATH)
    return {"version": 3, "accounts": {}}


def _save() -> None:
    write_atomic(_PATH, _STORE, prefix=".rrhist-")


_STORE = _load(read_json(_PATH, None))


def _quality(point: dict[str, Any]) -> int:
    known = sum(point.get(field) is not None for field in ("result", "delta", "tier", "rr"))
    seen = 10 if point.get("source") == "live" else 1
    return seen + known + (3 if point.get("resultExact") else 0)


_KEPT = (
    "matchId",
    "ts",
    "map",
    "result",
    "delta",
    "tier",
    "rr",
    "actId",
    "agent",
    "agentColor",
    "partySize",
    "mode",
    "scores",
    "kills",
    "deaths",
    "assists",
    "kd",
    "acs",
    "hsPct",
    "resultExact",
)


def _cleaned(point: dict[str, Any], source: str) -> dict[str, Any]:
    kept = {key: value for key in _KEPT if (value := point.get(key)) is not None}
    return {"resultExact": source == "live", **kept, "source": source}


def _merged(
    points: list[dict[str, Any]], point: dict[str, Any], source: str
) -> list[dict[str, Any]]:
    fresh = _cleaned(point, source)
    match_id = fresh.get("matchId")
    if not match_id or not fresh.get("ts"):
        return points
    by_match = {p["matchId"]: p for p in points if p.get("matchId") and p.get("ts")}
    old = by_match.get(match_id)
    if old is None:
        by_match[match_id] = fresh
    else:
        better = _quality(fresh) >= _quality(old)
        by_match[match_id] = {
            **old,
            **{key: value for key, value in fresh.items() if better or old.get(key) is None},
        }
    return sorted(by_match.values(), key=lambda p: p.get("ts") or 0)[-_MAX_POINTS:]


def _ensure_account(puuid: str) -> dict[str, Any]:
    account: dict[str, Any] = _STORE.setdefault("accounts", {}).setdefault(puuid, {"points": []})
    return account


def record(point: dict[str, Any]) -> None:
    """Record one rating point for its account, keeping the best copy of each match."""
    owner = str(point.get("puuid") or "")
    if not owner or owner.startswith("demo"):
        return
    with _LOCK:
        account = _ensure_account(owner)
        account["points"] = _merged(account.get("points", []), point, "live")
        _save()


def _point_from_update(update: dict[str, Any]) -> dict[str, Any] | None:
    started = int((update.get("MatchStartTime") or 0) / 1000)
    if not update.get("MatchID") or not started:
        return None
    earned = update.get("RankedRatingEarned")
    result = None
    # RR won or lost gives the result away, and a 0 says neither.
    if isinstance(earned, (int, float)) and earned:
        result = "Victory" if earned > 0 else "Defeat"
    return {
        "matchId": update["MatchID"],
        "ts": started,
        "map": map_name_from_path(update.get("MapID") or ""),
        "result": result,
        "resultExact": False,
        "delta": earned,
        "tier": update.get("TierAfterUpdate"),
        "rr": update.get("RankedRatingAfterUpdate"),
    }


def refresh(auth: Any) -> None:
    """Backfill the signed-in account's rating from Riot."""
    try:
        auth.headers()
        puuid = auth.puuid
    except Exception as e:
        LOG.warning("rr refresh could not reach the client: %r", e)
        return
    if not puuid:
        return
    with _LOCK:
        _ensure_account(puuid)
        if 0 <= time.time() - _refresh_at.get(puuid, 0) < _REFRESH_TTL:
            _save()
            return
        _refresh_at[puuid] = time.time()
    try:
        updates = _competitive_updates(auth, puuid)
        with _LOCK:
            account = _ensure_account(puuid)
            points = account.get("points", [])
            for point in filter(None, map(_point_from_update, updates)):
                points = _merged(points, point, "backfill")
            account["points"] = points
            _save()
    except Exception as e:
        LOG.warning("rr backfill failed: %r", e)
        with _LOCK:
            _refresh_at.pop(puuid, None)


def _competitive_updates(auth: Any, puuid: str) -> list[dict[str, Any]]:
    matches: list[dict[str, Any]] = []
    for start in (0, 20):
        cu = auth.pd_get(
            f"/mmr/v1/players/{puuid}/competitiveupdates"
            f"?startIndex={start}&endIndex={start + 20}&queue=competitive",
            urgent=True,
        )
        if not isinstance(cu, dict) or cu.get("errorCode"):
            code = (
                cu.get("errorCode", "MMR_REQUEST_FAILED")
                if isinstance(cu, dict)
                else "MMR_REQUEST_FAILED"
            )
            raise RuntimeError(code)
        page = cu.get("Matches") or []
        matches.extend(page)
        if len(page) < 20:
            break
    return matches


def deltas(puuid: str) -> dict[str, int]:
    """Return the RR each recorded competitive match gave or took, by match id."""
    with _LOCK:
        points = ((_STORE.get("accounts") or {}).get(puuid) or {}).get("points") or []
        return {
            p["matchId"]: int(p["delta"])
            for p in points
            if p.get("matchId") and isinstance(p.get("delta"), (int, float))
        }


def _main(points: list[dict[str, Any]]) -> str | None:
    played = Counter(
        p.get("agent") for p in points[-20:] if p.get("agent") not in (None, "", "Unknown")
    )
    return played.most_common(1)[0][0] if played else None


def _last_played(account: dict[str, Any]) -> int:
    return max((int(p.get("ts") or 0) for p in account.get("points") or []), default=0)


def main_agent() -> str | None:
    """Return the agent the most recently played account plays most."""
    with _LOCK:
        accounts = (_STORE.get("accounts") or {}).values()
        account: dict[str, Any] = max(accounts, key=_last_played, default={})
        return _main(list(account.get("points") or []))


if __name__ == "__main__":
    played = [{"agent": "Jett"}, {"agent": "Sova"}, {"agent": "Sova"}, {"agent": "Unknown"}, {}]
    check(_main(played) == "Sova")
    older = [{"agent": "Omen"}] * 30
    check(_main(older + [{"agent": "Jett"}] * 20) == "Jett", "only the last 20 count")

    filled = _merged([], {"matchId": "m", "ts": 5, "delta": 18, "tier": 12}, "backfill")
    both = _merged(filled, {"matchId": "m", "ts": 5, "delta": 20, "kills": 9}, "live")
    check(len(both) == 1, both)
    check((both[0]["delta"], both[0]["tier"], both[0]["kills"]) == (20, 12, 9), both)
    check(both[0]["source"] == "live" and both[0]["resultExact"], both)
    check(_merged(both, {"matchId": "n"}, "live") == both, "a point with no time was kept")
    back = _merged(both, {"matchId": "m", "ts": 5, "delta": 1}, "backfill")
    check(back[0]["delta"] == 20, "a backfill overwrote a live point")
    print("history self-check OK")

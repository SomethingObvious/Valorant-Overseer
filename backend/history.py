# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""Your ranked rating over time, per account, for the History screen.

Recorded as matches end and backfilled from Riot's competitive updates.
"""

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
_MIN_TOTAL = 20
_MIN_BUCKET = 8
_refresh_at: dict[str, float] = {}
_enrich_at: dict[str, float] = {}


def _empty_store() -> dict[str, Any]:
    return {"version": 3, "accounts": {}, "discardedOrphanPoints": 0}


def _normalise_store(raw: Any) -> dict[str, Any]:
    if not isinstance(raw, dict):
        return _empty_store()
    version = raw.get("version")
    if version == 3 and isinstance(raw.get("accounts"), dict):
        out = _empty_store()
        out.update(raw)
        return out
    if version == 2 and isinstance(raw.get("accounts"), dict):
        out = _empty_store()
        discarded = len(raw.get("legacyPoints") or [])
        for puuid, account in raw["accounts"].items():
            if not puuid or not isinstance(account, dict):
                continue
            points = account.get("points") or []
            kept = [p for p in points if isinstance(p, dict) and p.get("source") != "legacy"]
            discarded += len(points) - len(kept)
            out["accounts"][puuid] = {**account, "points": kept}
        out["discardedOrphanPoints"] = discarded
        return out
    out = _empty_store()
    out["discardedOrphanPoints"] = len(raw.get("points") or [])
    return out


def _load(raw: Any) -> dict[str, Any]:
    try:
        return _normalise_store(raw)
    except TypeError as e:
        LOG.warning("%s parsed, but not as rr history, starting empty: %r", _PATH, e)
        return _empty_store()


def _save() -> None:
    write_atomic(_PATH, _STORE, prefix=".rrhist-")


_raw = read_json(_PATH, None)
_STORE = _load(_raw)
if isinstance(_raw, dict) and _raw.get("version") != 3:
    _save()


def _quality(point: dict[str, Any]) -> int:
    score = 10 if point.get("source") == "live" else 1
    score += sum(point.get(k) is not None for k in ("result", "delta", "tier", "rr"))
    if point.get("resultExact"):
        score += 3
    return score


def _clean_point(point: dict[str, Any], source: str) -> dict[str, Any]:
    allowed = (
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
    out = {k: point.get(k) for k in allowed if point.get(k) is not None}
    out["source"] = source
    if "resultExact" not in out:
        out["resultExact"] = source == "live"
    return out


def _upsert_points(
    points: list[dict[str, Any]], point: dict[str, Any], source: str
) -> list[dict[str, Any]]:
    cleaned = _clean_point(point, source)
    match_id = cleaned.get("matchId")
    if not match_id or not cleaned.get("ts"):
        return points
    existing = next((p for p in points if p.get("matchId") == match_id), None)
    if existing is None:
        points.append(cleaned)
    else:
        merged = dict(existing)
        for key, value in cleaned.items():
            if value is None:
                continue
            if _quality(cleaned) >= _quality(existing) or merged.get(key) is None:
                merged[key] = value
        existing.clear()
        existing.update(merged)
    deduped = {p.get("matchId"): p for p in points if p.get("matchId") and p.get("ts")}
    return sorted(deduped.values(), key=lambda p: p.get("ts") or 0)[-_MAX_POINTS:]


def _ensure_account(puuid: str) -> dict[str, Any]:
    account: dict[str, Any] = _STORE.setdefault("accounts", {}).setdefault(puuid, {"points": []})
    return account


def record(
    point: dict[str, Any],
) -> None:
    """Record one rating point for its account, keeping the best copy of each match."""
    owner = point.get("puuid")
    if not owner or str(owner).startswith("demo"):
        return
    with _LOCK:
        account = _ensure_account(str(owner))
        account["points"] = _upsert_points(account.get("points", []), point, "live")
        _save()


def refresh(auth: Any) -> str | None:
    """Backfill the signed-in account's rating from Riot, and return its puuid."""
    try:
        auth.headers()
        puuid = auth.puuid
    except Exception as e:
        LOG.warning("rr refresh could not reach the client: %r", e)
        return None
    if not puuid:
        return None

    now = time.time()
    with _LOCK:
        account = _ensure_account(puuid)
        if now - _refresh_at.get(puuid, 0) < _REFRESH_TTL:
            _save()
            return puuid
        _refresh_at[puuid] = now
    try:
        matches = _competitive_updates(auth, puuid)
        with _LOCK:
            account = _ensure_account(puuid)
            points = account.get("points", [])
            for match in matches:
                delta = match.get("RankedRatingEarned")
                ts = int((match.get("MatchStartTime") or 0) / 1000) or None
                if not match.get("MatchID") or not ts:
                    continue
                result = None
                if isinstance(delta, (int, float)) and delta:
                    result = "Victory" if delta > 0 else "Defeat"
                point = {
                    "matchId": match.get("MatchID"),
                    "ts": ts,
                    "map": map_name_from_path(match.get("MapID") or ""),
                    "result": result,
                    "resultExact": False,
                    "delta": delta,
                    "tier": match.get("TierAfterUpdate"),
                    "rr": match.get("RankedRatingAfterUpdate"),
                }
                points = _upsert_points(points, point, "backfill")
            account["points"] = points
            _save()
    except Exception as e:
        LOG.warning("rr backfill failed: %r", e)
        with _LOCK:
            _refresh_at.pop(puuid, None)
    return puuid


def _competitive_updates(auth: Any, puuid: str) -> list[dict[str, Any]]:
    """Return the account's last 40 competitive matches with what each did to its RR.

    Raises RuntimeError with Riot's own error code when it answers with one.
    """
    matches: list[dict[str, Any]] = []
    for start in (0, 20):
        cu = auth.pd_get(
            f"/mmr/v1/players/{puuid}/competitiveupdates"
            f"?startIndex={start}&endIndex={start + 20}&queue=competitive"
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
    """Return the agent played most over the last 20 recorded matches."""
    played = Counter(
        p.get("agent") for p in points[-20:] if p.get("agent") not in (None, "", "Unknown")
    )
    return played.most_common(1)[0][0] if played else None


def _last_played(account: dict[str, Any]) -> int:
    """When the account's newest recorded match was, as a Unix time."""
    return max((int(p.get("ts") or 0) for p in account.get("points") or []), default=0)


def main_agent(puuid: str | None = None) -> str | None:
    """Return the agent `puuid` plays most.

    With none given, the most recently played account's, so it answers while
    the Riot client is closed too.
    """
    with _LOCK:
        accounts = list((_STORE.get("accounts") or {}).items())
        if puuid:
            account = dict(accounts).get(puuid) or {}
        else:
            account = max((a for _, a in accounts), key=_last_played, default={})
        return _main(list(account.get("points") or []))


_DAYPARTS = (("morning", 5, 12), ("afternoon", 12, 17), ("evening", 17, 22), ("night", 22, 29))


if __name__ == "__main__":
    played = [{"agent": "Jett"}, {"agent": "Sova"}, {"agent": "Sova"}, {"agent": "Unknown"}, {}]
    check(_main(played) == "Sova")
    check(_main([]) is None)
    older = [{"agent": "Omen"}] * 30
    check(_main(older + [{"agent": "Jett"}] * 20) == "Jett", "only the last 20 count")
    print("history self-check OK")

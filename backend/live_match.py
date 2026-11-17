# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""The live board: who is in your match, their ranks, history and parties."""

from __future__ import annotations

import base64
import contextlib
import gzip
import itertools
import json
import os
import re
import statistics
import sys
import threading
import time
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from typing import TYPE_CHECKING, Any

import brief
import encounter_log
import refresh
import requests
import riot_client
import valapi
from agents import UUID_TO_NAME, resolve_agent
from career import STANDARD_QUEUES, career_mids, career_summary, form_streak, rating_run
from common import console_logger
from common.check import check, present
from common.jsonstore import data_path, read_json, write_atomic
from match_cache import MatchCache
from rounds import buy_habits, habits_of, round_stats, rounds_played
from smurf import compute_smurf, smurf_evidence
from vconstants import (
    GAMEMODES,
    ROUTING,
    STATES,
    map_name_from_path,
    party_color,
    rank_from_tier,
    rr_moved,
)

if TYPE_CHECKING:
    from collections.abc import Iterator


def _mode_label(queue: str) -> str:
    return GAMEMODES.get(queue.lower(), queue.replace("_", " ").title()) if queue else "Custom"


# Acts from before Ascendant existed. Their tiers above Diamond 3 are three
# lower than today's, so a peak from one of them moves up by three.
BEFORE_ASCENDANT = {
    "0df5adb9-4dcb-6899-1306-3e9860661dd3",
    "3f61c772-4560-cd3f-5d3f-a7ab5abda6b3",
    "0530b9c4-4980-f2ee-df5d-09864cd00542",
    "46ea6166-4573-1128-9cea-60a15640059b",
    "fcf2c8f4-4324-e50b-2e23-718e4a3ab046",
    "97b6e739-44cc-ffa7-49ad-398ba502ceb0",
    "ab57ef51-4e59-da91-cc8d-51a5a2b9b8ff",
    "52e9749a-429b-7060-99fe-4595426a0cf7",
    "71c81c67-4fae-ceb1-844c-aab2bb8710fa",
    "2a27e5d2-4d30-c9e2-b15a-93b8909a442c",
    "4cb622e1-4244-6da3-7276-8daaf1c01be2",
    "a16955a5-4ad0-f761-5e9e-389df1c892fb",
    "97b39124-46ce-8b55-8fd1-7cbf7ffe173f",
    "573f53ac-41a5-3a7d-d9ce-d6a6298e5704",
    "d929bc38-4ab6-7da4-94f0-ee84f8ac141e",
    "3e47230a-463c-a301-eb7d-67bb60357d4f",
    "808202d6-4f2b-a8ff-1feb-b3a0590ad79f",
}

_CACHE: dict[str, dict[str, Any]] = {}

_MATCH_META: dict[str, dict[str, Any]] = {}
# Matches left behind, to a dodge or the end of the game. Their K/D fill
# stops, since it spends the same workers and Riot budget the next match's
# board waits on.
_ENDED: set[str] = set()
# How long a board waits before asking Riot again for names it lacks.
NAME_RETRY_SECS = 10.0

_LOBBY_CACHE: dict[str, Any] = {"key": None, "at": 0.0, "board": None}

# The last good answer for each account seen in the lobby. A lookup that
# fails, to a dropped connection or Riot throttling, says nothing about the
# player, so the row keeps what it had rather than going blank for a rebuild.
_LOBBY_KEPT: dict[str, dict[str, Any]] = {}

_LAST_BOARD: dict[str, Any] = {"board": None, "at": 0.0}
_HOLD_SECS = 90.0

_OWN_PARTY: dict[str, Any] = {"id": "", "size": 0, "at": 0.0, "members": []}
_OWN_PARTY_SECS = 300.0

# How long a failed lookup waits before it is asked again. The board asks on
# every tick, and a service that is down costs 8 seconds a request.
_RETRY_SECS = 60.0

# A Riot ID from account-v1 and the time it may be asked for again. A real
# answer keeps for the session.
_ACCT_CACHE: dict[str, tuple[str | None, float]] = {}

_CONTENT_CACHE: dict[str, Any] = {"seasons": None, "at": 0.0, "failed_at": 0.0}

_LEVEL_CACHE: dict[str, int] = {}

_KD_FILL_LOCK = threading.Lock()
_KD_FILLING: dict[str, set[str]] = {}

_KD_CACHE: dict[str, tuple[tuple[Any, ...], tuple[Any, ...], int]] = {}
_READS_CACHE: dict[str, dict[str, Any] | None] = {}
_READS_MAX = 4000
_KD_CACHE_MAX = 300

# Everybody's match list, kept for the whole match, since nobody in it can
# finish another game until it ends. It is dropped as you get back to menus,
# when yours and your party's just grew, and in menus it is read again after
# five minutes. Reading it every minute would cost 10 or more of Riot's 60 or
# so a minute while a lobby read back to 20.
_MIDS_CACHE: dict[str, tuple[list[str], bool, float]] = {}
_MIDS_TTL = 300.0
_STATE_SEEN: dict[str, Any] = {"state": "", "matched": 0.0}
_IN_MATCH = ("PREGAME", "INGAME")
# How many games each row is read from, as the window's setting says. Every
# row gets five first, then they step toward it together, through the rungs
# between, and a step with no room in Riot's minute goes again once there is.
GAMES_READ = (5, 10, 20)
# Only games from the last this many days count, 0 for any age.
_DEPTH = {"games": 20, "days": 30}
# When each match id in a history started, in Riot's milliseconds.
_STARTS: dict[str, int] = {}
_STARTS_MAX = 5000


# The match details behind your party's reads, kept on disk for as many days
# as the window says, 0 for none. A detail never changes once the game is
# over, so after a restart only the games played since are asked of Riot,
# and a read still only counts the newest games in its history.
_KEPT_DIR = Path(data_path("matches"))
_KEEP = {"days": 14}
_MONTH_DAYS = 30
_MONTH_SUFFIX = ".month.json.gz"
# Retention limits apply together: age, file count and compressed bytes. A
# match is about 30 KB, so the count is the one that binds.
_KEPT_MAX_MATCHES = 400
_KEPT_MAX_BYTES = 64 * 1024 * 1024
_KEPT_LOCK = threading.RLock()
_MATCH_ID = re.compile(r"[0-9a-fA-F-]{36}")
_PARTY: set[str] = set()


def set_depth(games: int, days: int = 30, keep: int = 14) -> None:
    """Read `games` games for each row from now on, the nearest choice the window offers."""
    _DEPTH["games"] = min(GAMES_READ, key=lambda g: abs(g - games))
    _KEEP["days"] = max(0, keep)
    _prune_kept()
    if _DEPTH["days"] == max(0, days):
        return
    _DEPTH["days"] = max(0, days)
    _KD_CACHE.clear()
    for entry in [*_CACHE.values(), *_LOBBY_KEPT.values()]:
        if "deep" in entry:
            entry["deep"] = 0


# Drop unused fields and duplicate top-level kills, and share repeated strings.
# The Tendencies section needs playerLocations and victimLocation on each round's kills for
# spacing, spawn distance and gun range, so they stay on disk. In memory they
# go once the match is measured (see _remember), as they double its size.
_UNREAD = {
    "match": ("kills", "bots", "coaches"),
    "round": (
        "playerEconomies",
        "playerScores",
        "plantPlayerLocations",
        "defusePlayerLocations",
        "plantLocation",
        "defuseLocation",
    ),
    "stats": ("ability",),
    "player": (
        "roundDamage",
        "scores",
        "newPlayerExperienceDetails",
        "behaviorFactors",
        "bonsaiBloomline",
        "platformInfo",
        "xpModifications",
    ),
}


def _interned(value: Any) -> Any:
    if isinstance(value, dict):
        return {sys.intern(k): _interned(v) for k, v in value.items()}
    if isinstance(value, list):
        return [_interned(v) for v in value]
    return sys.intern(value) if isinstance(value, str) else value


def _slim(md: dict[str, Any]) -> dict[str, Any]:

    def without(part: Any, kind: str) -> Any:
        if not isinstance(part, dict):
            return part
        return {k: v for k, v in part.items() if k not in _UNREAD[kind]}

    slim = without(md, "match")
    if isinstance(slim.get("players"), list):
        slim["players"] = [without(p, "player") for p in slim["players"]]
    rounds = []
    for full_round in slim.get("roundResults") or []:
        rr = without(full_round, "round")
        if isinstance(rr, dict):
            rr["playerStats"] = [without(ps, "stats") for ps in rr.get("playerStats") or []]
        rounds.append(rr)
    if "roundResults" in slim:
        slim["roundResults"] = rounds
    return _interned(slim)


def _kept_path(mid: str, *, month: bool = False) -> Path | None:
    # Riot's ids only, so a strange one never names a file outside the folder.
    if not _MATCH_ID.fullmatch(mid):
        return None
    return _KEPT_DIR / (mid + (_MONTH_SUFFIX if month else ".json.gz"))


def _kept_days(path: Path) -> int:
    month = _KEEP["days"] and path.name.endswith(_MONTH_SUFFIX)
    return _MONTH_DAYS if month else _KEEP["days"]


def _kept_file(mid: str) -> Path | None:
    for month in (True, False):
        path = _kept_path(mid, month=month)
        if path is None:
            return None
        with contextlib.suppress(OSError):
            if path.stat().st_mtime >= time.time() - _kept_days(path) * 86400:
                return path
    return None


def _kept_detail(mid: str) -> dict[str, Any] | None:
    path = _kept_file(mid)
    if path is None:
        return None
    try:
        with gzip.open(path, "rt", encoding="utf-8") as fh:
            md = json.load(fh)
    except (OSError, EOFError, ValueError):
        return None
    return _slim(md) if isinstance(md, dict) and "players" in md else None


def _keep_detail(mid: str, md: dict[str, Any], *, month: bool = False) -> None:
    path = _kept_path(mid, month=month)
    longer = _kept_path(mid, month=True)
    if not _KEEP["days"] or path is None or longer is None or path.exists() or longer.exists():
        return
    started = ((md.get("matchInfo") or {}).get("gameStartMillis") or 0) / 1000
    if started < time.time() - _kept_days(path) * 86400:
        return
    tmp = path.with_name(f".{mid}.{threading.get_ident()}.tmp")
    try:
        payload = gzip.compress(json.dumps(md, separators=(",", ":")).encode("utf-8"))
        with _KEPT_LOCK:
            if not _KEEP["days"] or path.exists() or not _prune_kept(incoming=len(payload)):
                return
            _KEPT_DIR.mkdir(parents=True, exist_ok=True)
            tmp.write_bytes(payload)
            os.utime(tmp, (started, started))
            tmp.replace(path)
            if month:
                path.with_name(f"{mid}.json.gz").unlink(missing_ok=True)
    except OSError as e:
        _log(f"couldn't keep match {mid[:8]}: {e}")
        with contextlib.suppress(OSError):
            tmp.unlink()


def _prune_kept(*, incoming: int = 0) -> bool:
    with _KEPT_LOCK:
        if incoming > _KEPT_MAX_BYTES:
            return False
        now = time.time()
        kept = []
        # A process interrupted during a write may leave a temporary file.
        # Under the writer lock none of these belongs to an active write.
        paths = [*_KEPT_DIR.glob("*.json.gz"), *_KEPT_DIR.glob(".*.tmp")]
        for path in paths:
            try:
                stat = path.stat()
            except FileNotFoundError:
                continue
            except OSError as e:
                _log(f"couldn't inspect match cache: {e}")
                return False
            kept.append((0.0 if path.suffix == ".tmp" else stat.st_mtime, path, stat.st_size))
        total = sum(size for _, _, size in kept)
        count = len(kept)
        for modified, path, size in sorted(kept):
            if (
                modified >= now - _kept_days(path) * 86400
                and count + bool(incoming) <= _KEPT_MAX_MATCHES
                and total + incoming <= _KEPT_MAX_BYTES
            ):
                continue
            try:
                path.unlink()
            except OSError as e:
                _log(f"couldn't prune match cache: {e}")
                continue
            total -= size
            count -= 1
        return count + bool(incoming) <= _KEPT_MAX_MATCHES and total + incoming <= _KEPT_MAX_BYTES


def _list_holds(read_at: float) -> bool:
    in_match = _STATE_SEEN["state"] in _IN_MATCH
    return read_at >= (_STATE_SEEN["matched"] if in_match else time.time()) - _MIDS_TTL


def _recent(mids: list[str]) -> list[str]:
    if not _DEPTH["days"]:
        return mids
    since = (time.time() - _DEPTH["days"] * 86400) * 1000
    return [m for m in mids if _STARTS.get(m, since) >= since]


def _reading(rows: list[tuple[str, int | None]]) -> dict[str, Any] | None:
    depth = _DEPTH["games"]
    got = want = unread = guessed = 0
    missing: set[str] = set()
    for puuid, have in rows:
        hit = _MIDS_CACHE.get(puuid)
        if hit is not None:
            mids = _recent(hit[0])[:depth]
            want += len(mids)
            for mid in mids:
                if mid in _DETAIL_READ or mid in _MATCH_DETAIL_CACHE:
                    got += 1
                else:
                    missing.add(mid)
            continue
        want += depth
        if have is None:
            unread += 1
        else:
            got += min(have, depth)
            guessed += depth - min(have, depth)
    left = len(missing) + guessed + unread * (depth + 1)
    if not left or not want:
        return None
    return {
        "progress": round(got / want, 3),
        "doneAt": round(time.time() + riot_client.budget_eta(left)),
        "games": depth,
    }


def _due(at: float) -> bool:
    # Riot's waits run five minutes at most, so one further off than ten is a
    # clock that was set back since, and would hold the row for the jump.
    now = time.time()
    return now >= at or at - now > 600


def _deeper_at() -> float:
    # When a deeper read held back by Riot's budget, or by a lockout, is
    # worth trying again.
    wait = max(
        riot_client.budget_wait(),
        riot_client.held_secs("/match-details/v1/matches/"),
    )
    return time.time() + max(1.0, wait)


# Match details being fetched right now, so two rows reading a game they
# played together ask Riot for it once.
_FETCHING: dict[str, tuple[threading.Lock, int]] = {}
_FETCHING_LOCK = threading.Lock()
# How many times a deeper read Riot held back goes again before the pass
# leaves that row for the next board.
_DEEP_TRIES = 5
_DEEPEN_LOCK = threading.Lock()


@contextlib.contextmanager
def _fetching(mid: str) -> Iterator[None]:
    # Counted, since a lock dropped while a thread still waits on it lets the
    # next one make a new lock and fetch alongside that thread.
    with _FETCHING_LOCK:
        lock, users = _FETCHING.get(mid, (threading.Lock(), 0))
        _FETCHING[mid] = (lock, users + 1)
    try:
        with lock:
            yield
    finally:
        with _FETCHING_LOCK:
            left = _FETCHING[mid][1] - 1
            if left:
                _FETCHING[mid] = (lock, left)
            else:
                del _FETCHING[mid]


_SAVED_FILE = data_path("current-match.json")
_SAVED_SECS = 3 * 3600.0
_SAVED_META = ("names", "loadouts", "loadouts_for", "parties")
_SAVED_TEXT = {"text": ""}


def _save_match(match_id: str, puuids: list[str]) -> None:
    keys = {p: f"{match_id}:{p}" for p in puuids}
    meta = _MATCH_META.get(match_id) or {}
    snap = {
        "match": match_id,
        "meta": {k: meta[k] for k in _SAVED_META if k in meta},
        "rows": {p: _CACHE[k] for p, k in keys.items() if k in _CACHE},
        "levels": {p: _LEVEL_CACHE[p] for p in puuids if p in _LEVEL_CACHE},
        "mids": {p: _MIDS_CACHE[p] for p in puuids if p in _MIDS_CACHE},
    }
    try:
        text = json.dumps(snap, sort_keys=True)
    except (TypeError, ValueError, RuntimeError) as e:
        # A fill thread changing a row mid-dump is the RuntimeError, and the
        # next board saves it.
        _log(f"couldn't save the match: {type(e).__name__}: {e}")
        return
    if text == _SAVED_TEXT["text"]:
        return
    if write_atomic(
        _SAVED_FILE, {**json.loads(text), "at": time.time()}, prefix="match-", durable=False
    ):
        _SAVED_TEXT["text"] = text


def _restore_match(match_id: str) -> dict[str, Any]:
    saved = read_json(_SAVED_FILE, {})
    if not isinstance(saved, dict) or saved.get("match") != match_id:
        return {}
    try:
        if time.time() - float(saved.get("at") or 0) > _SAVED_SECS:
            return {}
        rows = {p: e for p, e in (saved.get("rows") or {}).items() if isinstance(e, dict)}
        for puuid, entry in rows.items():
            _cache_put(_CACHE, _KD_CACHE_MAX, f"{match_id}:{puuid}", entry)
        for puuid, level in (saved.get("levels") or {}).items():
            _cache_put(_LEVEL_CACHE, _KD_CACHE_MAX, puuid, int(level))
        for puuid, (mids, comp, at) in (saved.get("mids") or {}).items():
            _MIDS_CACHE.setdefault(puuid, (list(mids), bool(comp), float(at)))
            _STATE_SEEN["matched"] = min(_STATE_SEEN["matched"], float(at))
        meta = saved.get("meta") or {}
    except (TypeError, ValueError, AttributeError) as e:
        _log(f"couldn't read the saved match: {type(e).__name__}: {e}")
        return {}
    _log(f"picked up match {match_id[:8]} where it was left, {len(rows)} rows")
    return {k: meta[k] for k in _SAVED_META if k in meta}


def _note_state(state: str) -> None:
    if state == "MENUS" and _STATE_SEEN["state"] in ("INGAME", "PREGAME"):
        if _STATE_SEEN["state"] == "INGAME":
            _MIDS_CACHE.clear()
        _ENDED.update(_MATCH_META)
        # About 200 ids a match, for days on end. Forgetting them only makes
        # the next progress line count a few games as unread.
        if len(_DETAIL_READ) > _DETAIL_READ_MAX:
            _DETAIL_READ.clear()
    if state in _IN_MATCH and _STATE_SEEN["state"] not in _IN_MATCH:
        _STATE_SEEN["matched"] = time.time()
    _STATE_SEEN["state"] = state
    # Room is held back for the players still to come: your team once a match
    # is found, then the enemies, who can load in half a minute after it when
    # everybody locks in fast. Once the match is on, everybody is here.
    riot_client.keep_for_first_reads(0 if state == "INGAME" else 5)


def _next_rung(have: int) -> int:
    want = _DEPTH["games"]
    return next((g for g in GAMES_READ if have < g < want), want)


_RANK_CACHE: dict[str, tuple[dict[str, Any], str]] = {}
_RR_CACHE: dict[str, tuple[Any, ...]] = {}

# Compressed, so the spatial data the Tendencies section reads stays without hundreds of parsed
# object trees in memory. Only the compressed payloads count toward the 16 MiB,
# and workers still decode the matches they are measuring.
_MATCH_DETAIL_CACHE = MatchCache(max_entries=200, max_bytes=16 * 1024 * 1024)
_DETAIL_READ: set[str] = set()
_DETAIL_READ_MAX = 20_000

_CAREER_CACHE: dict[str, tuple[dict[str, Any], tuple[str, ...]]] = {}

# Two pools, not one. The per-player workers call kd_hs, which fans out again
# over match details, and with one shared pool the outer workers can hold
# every thread while they wait on inner tasks that never get one. They sit at
# module level so a refresh does not start threads every tick, and they are
# never used as context managers, which would shut them down after one call.
_PLAYER_POOL = ThreadPoolExecutor(max_workers=8, thread_name_prefix="players")
_DETAIL_POOL = ThreadPoolExecutor(max_workers=6, thread_name_prefix="details")

_CACHE_WRITE_LOCK = threading.Lock()


def _cache_put(cache: dict[str, Any], cap: int, key: Any, value: Any) -> None:
    with _CACHE_WRITE_LOCK:
        for old in list(itertools.islice(cache, max(len(cache) - cap + 1, 0))):
            cache.pop(old, None)
        cache[key] = value


_QUEUE_CACHE: dict[str, Any] = {"at": 0.0, "data": None}
# Riot IDs found, by puuid. A lobby asked the name service for the same
# names every tick, and a name doesn't change while you look at it.
_NAME_CACHE: dict[str, str] = {}
_NAME_CACHE_MAX = 500


_log = console_logger("reveal")


def _is_throttled(resp: Any) -> bool:
    return isinstance(resp, dict) and resp.get("status") == 429


def _standard(md: Any, puuid: str) -> bool:
    if not isinstance(md, dict):
        return False
    info = md.get("matchInfo") or {}
    queue = str(info.get("queueID") or info.get("queueId") or "").lower()
    me = next((p for p in md.get("players") or [] if p.get("subject") == puuid), None)
    teams = {t.get("teamId"): t for t in md.get("teams") or []}
    won = (teams.get((me or {}).get("teamId")) or {}).get("won")
    return queue in STANDARD_QUEUES and won is not None


def _shots_by_player(md: dict[str, Any]) -> dict[Any, tuple[int, int]]:
    heads: Counter[Any] = Counter()
    hits: Counter[Any] = Counter()
    for rnd in md.get("roundResults", []):
        for stats in rnd.get("playerStats", []):
            who = stats.get("subject")
            for hit in stats.get("damage", []):
                heads[who] += hit.get("headshots", 0)
                hits[who] += hit.get("headshots", 0) + hit.get("bodyshots", 0)
                hits[who] += hit.get("legshots", 0)
    return {who: (heads[who], hits[who]) for who in hits}


def _hs_pct(shots: tuple[int, int]) -> int | None:
    heads, hits = shots
    return round(heads / hits * 100) if hits else None


def _kd(kills: int, deaths: int) -> float:
    return round(kills / deaths, 2) if deaths else float(kills)


def _per_round(score: int, rounds: int) -> int:
    return round(score / rounds) if rounds else 0


def _kills_by_gun(md: dict[str, Any], puuid: str) -> list[dict[str, Any]]:
    guns: Counter[str] = Counter()
    for rnd in md.get("roundResults", []):
        for stats in rnd.get("playerStats", []):
            if stats.get("subject") != puuid:
                continue
            for kill in stats.get("kills") or []:
                if item := (kill.get("finishingDamage") or {}).get("damageItem"):
                    guns[item] += 1
    return [
        {"name": valapi.weapon_name(item) or "Ability", "kills": n}
        for item, n in sorted(guns.items(), key=lambda kv: (-kv[1], str(kv[0])))
    ]


def _teammate(p: dict[str, Any], rounds: int, shots: dict[Any, tuple[int, int]]) -> dict[str, Any]:
    line = p.get("stats") or {}
    agent = resolve_agent(p.get("characterId") or "") or {}
    identity = p.get("PlayerIdentity") or p.get("playerIdentity") or {}
    return {
        "puuid": p.get("subject"),
        "agent": agent.get("name"),
        "agentColor": agent.get("color", "#8B978F"),
        "level": p.get("accountLevel") or identity.get("AccountLevel"),
        "kills": line.get("kills", 0),
        "deaths": line.get("deaths", 0),
        "assists": line.get("assists", 0),
        "acs": _per_round(line.get("score", 0), rounds),
        "headshots": shots.get(p.get("subject"), (0, 0))[0],
    }


_ROUND_FIELDS = {
    "adr": "adr",
    "kast": "kastPct",
    "econ": "econ",
    "firstBloods": "firstBloods",
    "firstDeaths": "firstDeaths",
    "topWeapon": "topWeapon",
    "clutches": "clutches",
    "clutchesLost": "clutchesLost",
    "plants": "plants",
    "defuses": "defuses",
    "plantsWon": "plantsWon",
    "defusesWon": "defusesWon",
    "shots": "shots",
}
_RESULT = {True: "Victory", False: "Defeat"}


def _tally(details: list[dict[str, Any]], puuid: str) -> dict[str, Any] | None:
    kills = deaths = shots = heads = games = 0
    agents: Counter[str] = Counter()
    form: list[str] = []
    maps: dict[str, list[int]] = {}
    for md in details:
        me = next((p for p in md.get("players", []) if p.get("subject") == puuid), None)
        if me is None:
            continue
        games += 1
        line = me.get("stats", {})
        kills += line.get("kills", 0)
        deaths += line.get("deaths", 0)
        got_heads, got_hits = _shots_by_player(md).get(puuid, (0, 0))
        heads += got_heads
        shots += got_hits
        if name := UUID_TO_NAME.get((me.get("characterId") or "").lower()):
            agents[name] += 1
        won = next(
            (t.get("won") for t in md.get("teams", []) if t.get("teamId") == me.get("teamId")),
            None,
        )
        if won is None:
            continue
        form.append("W" if won else "L")
        where = map_name_from_path((md.get("matchInfo") or {}).get("mapId", ""))
        played = maps.setdefault(where, [0, 0])
        played[0] += 1 if won else 0
        played[1] += 1
    if not games:
        return None
    return {
        "kd": _kd(kills, deaths),
        "hs": _hs_pct((heads, shots)),
        "agents": agents,
        "form": form,
        "maps": maps,
    }


def _rank_from_mmr(mmr: dict[str, Any], season: str, prev_season: str | None) -> dict[str, Any]:
    acts = ((mmr.get("QueueSkills") or {}).get("competitive") or {}).get(
        "SeasonalInfoBySeasonID"
    ) or {}
    now = acts.get(season) or {}
    tier = now.get("CompetitiveTier", 0) or 0
    rr = now.get("RankedRating", 0) or 0
    latest = mmr.get("LatestCompetitiveUpdate") or {}
    if not tier and latest.get("SeasonID") == season:
        tier = int(latest.get("TierAfterUpdate") or 0)
        rr = int(latest.get("RankedRatingAfterUpdate") or 0)
    peak, peak_season = int(tier), season
    for act, info in acts.items():
        moved = 3 if act in BEFORE_ASCENDANT else 0
        for won_at in info.get("WinsByTier") or {}:
            reached = int(won_at) + (moved if int(won_at) > 20 else 0)
            if reached > peak:
                peak, peak_season = reached, act
    wins = next(
        (
            w
            for w in (now.get("NumberOfWinsWithPlacements"), now.get("NumberOfWins"))
            if w is not None
        ),
        None,
    )
    games = now.get("NumberOfGames", 0) or 0
    if tier and not games:
        _log(f"a rank with no games, as {sorted(now)}")
    return {
        "tier": tier,
        "rr": rr,
        "lb": now.get("LeaderboardRank", 0) or 0,
        "peak": peak,
        "wr": round(int(wins) / games * 100) if games and wins is not None else None,
        "games": games,
        "prev": ((acts.get(prev_season) or {}).get("CompetitiveTier", 0) or 0)
        if prev_season
        else 0,
        "peak_season": peak_season,
        "ok": True,
    }


def _kd_owed(entry: dict[str, Any]) -> bool:
    if not entry.get("kd_done"):
        return True
    if entry.get("kd") is None:
        return False
    if not entry.get("kd_full"):
        return True
    return entry.get("deep", 5) != _DEPTH["games"] and _due(entry.get("deep_wait", 0.0))


def _reading_of(match_id: str, puuids: list[str]) -> Any:
    rows: list[tuple[str, int | None]] = []
    for puuid in puuids:
        entry = _CACHE.get(f"{match_id}:{puuid}") or {}
        if entry.get("kd") is not None:
            rows.append((puuid, entry.get("deep", 5 if entry.get("kd_full") else 3)))
        elif not entry.get("kd_done"):
            rows.append((puuid, None))
        # Otherwise Riot had nothing on them, and there is nothing to wait for.
    return _reading(rows)


def _meta_for(match_id: str) -> dict[str, Any]:
    if match_id not in _MATCH_META:
        _ENDED.update(_MATCH_META)
        _MATCH_META.clear()
        _MATCH_META[match_id] = _restore_match(match_id)
    _ENDED.discard(match_id)
    return _MATCH_META[match_id]


def _empty_board(state: str) -> dict[str, Any]:
    if state in ("INGAME", "PREGAME"):
        # In a match Riot's servers don't have the players of yet.
        return {
            "state": "MENUS",
            "stateLabel": STATES["MENUS"],
            "source": "local",
            "waiting": "match",
            "players": [],
            "teams": {},
        }
    return {
        "state": state,
        "stateLabel": STATES.get(state, state),
        "source": "local",
        "players": [],
        "teams": {},
    }


def _fallback_name(puuid: str) -> str:
    return f"Player-{(puuid or '????')[:4].upper()}"


def _unplaced(md: dict[str, Any]) -> dict[str, Any]:

    def kill(k: Any) -> Any:
        if not isinstance(k, dict):
            return k
        return {f: v for f, v in k.items() if f not in ("playerLocations", "victimLocation")}

    def stats(ps: Any) -> Any:
        if not isinstance(ps, dict) or not ps.get("kills"):
            return ps
        return {**ps, "kills": [kill(k) for k in ps["kills"]]}

    def played(rr: Any) -> Any:
        if not isinstance(rr, dict):
            return rr
        return {**rr, "playerStats": [stats(ps) for ps in rr.get("playerStats") or []]}

    if "roundResults" not in md:
        return md
    return {**md, "roundResults": [played(rr) for rr in md["roundResults"] or []]}


def _tier_from_games(puuid: str, season: str | None) -> int:
    # Their tier in their newest ranked game this act, from details already
    # in hand. It stands in while Riot's rank service won't answer.
    hit = _MIDS_CACHE.get(puuid)
    if not season or not hit or not hit[1]:
        return 0
    for mid in hit[0][:5]:
        md = _MATCH_DETAIL_CACHE.get(mid) or {}
        if (md.get("matchInfo") or {}).get("seasonId") != season:
            continue
        me = next((p for p in md.get("players") or [] if p.get("subject") == puuid), None)
        return int((me or {}).get("competitiveTier") or 0)
    return 0


def _history_parties(puuids: list[str], team_of: dict[str, Any]) -> dict[str, list[str]]:
    out: dict[str, list[str]] = {}
    for puuid in puuids:
        hit = _MIDS_CACHE.get(puuid)
        md = _MATCH_DETAIL_CACHE.get(hit[0][0]) if hit and hit[0] else None
        if not isinstance(md, dict):
            continue
        rows = {
            p.get("subject"): p.get("partyId")
            for p in md.get("players") or []
            if isinstance(p, dict)
        }
        pid = rows.get(puuid)
        mates = sorted(
            m for m in puuids if pid and rows.get(m) == pid and team_of.get(m) == team_of.get(puuid)
        )
        if pid and len(mates) > 1:
            out[str(pid)] = mates
    return out


def _remember(mid: str, md: dict[str, Any]) -> dict[str, Any]:
    for player in md.get("players") or []:
        sub = str(player.get("subject") or "") if isinstance(player, dict) else ""
        if sub and _standard(md, sub):
            _measure(mid, md, sub)
    lean = _unplaced(md)
    _MATCH_DETAIL_CACHE.put(mid, lean)
    return lean


def played(auth: Any, mid: str, *, retries: int = 0) -> Any:
    """Return a finished match's details, from the ones in hand before Riot's."""
    with _fetching(mid):
        md: Any = _MATCH_DETAIL_CACHE.get(mid) or _kept_detail(mid)
        if md is not None:
            return md
        md = auth.pd_get(f"/match-details/v1/matches/{mid}", retries=retries, urgent=True)
        if not isinstance(md, dict) or "players" not in md:
            return md
        md = _slim(md)
        me = getattr(auth, "puuid", None)
        if any(isinstance(p, dict) and p.get("subject") == me for p in md["players"]):
            _keep_detail(mid, md)
        _DETAIL_READ.add(mid)
        _remember(mid, md)
        return md


def _measure(mid: str, md: dict[str, Any], puuid: str) -> dict[str, Any] | None:
    key = f"{mid}:{puuid}"
    if key in _READS_CACHE:
        return _READS_CACHE[key]
    try:
        read = brief.match_reads(md, puuid)
    except Exception as e:
        _log(f"brief could not read {mid[:8]} for {puuid[:8]}: {type(e).__name__}: {e}")
        read = None
    _cache_put(_READS_CACHE, _READS_MAX, key, read)
    return read


def assemble_player(
    *,
    puuid: str,
    name: str,
    name_hidden: Any,
    team: Any,
    is_self: bool,
    agent_id: Any,
    rank_tier: Any,
    rr: Any,
    leaderboard: Any,
    peak_tier: Any,
    prev_tier: Any,
    win_rate: Any,
    games: Any,
    kd: Any,
    hs: Any,
    level: Any,
    level_hidden: Any,
    party: Any,
    skin: Any = None,
    peak_act: Any = None,
    rr_earned: Any = None,
    rr_actual: Any = None,
    player_card: Any = None,
    title: Any = None,
    weapons: Any = None,
    selection: Any = None,
    smurf: bool = False,
    smurf_reasons: Any = None,
    intel: Any = None,
) -> dict[str, Any]:
    """Return one player's row for the board, from everything gathered about them."""
    agent = resolve_agent(agent_id or "") or {}
    rank = rank_from_tier(rank_tier)
    peak = rank_from_tier(peak_tier)
    prev = rank_from_tier(prev_tier)
    intel = intel or {}
    return {
        "puuid": puuid,
        "name": name,
        "nameHidden": bool(name_hidden),
        "team": team,
        "isSelf": bool(is_self),
        "title": title,
        "playerCard": player_card,
        "agent": agent.get("name") or (agent_id and "Unknown") or None,
        "agentColor": agent.get("color", "#8B978F"),
        "role": agent.get("role"),
        "selection": selection,
        "rankTier": rank["tier"],
        "rank": rank["name"],
        "rr": rr,
        "rrEarned": rr_earned,
        "rrActual": rr_actual,
        "leaderboard": leaderboard or 0,
        "peakRankTier": peak["tier"],
        "peakRank": peak["name"],
        "peakAct": peak_act,
        "previousRank": prev["name"],
        "previousRankTier": prev["tier"],
        "winRate": win_rate,
        "games": games,
        "kd": kd,
        "hsPct": hs,
        "skin": skin,
        "weapons": weapons or [],
        "level": level,
        "levelHidden": bool(level_hidden),
        "party": party,
        "smurf": bool(smurf),
        "smurfReasons": smurf_reasons or [],
        "smurfEvidence": smurf_evidence(
            level=level,
            peak_tier=peak_tier,
            rank_tier=rank_tier,
            kd=kd,
            win_rate=win_rate,
            games=games,
            kd_matches=len(intel.get("form") or []),
            hs=hs,
            peak_act=peak_act,
        )
        if smurf_reasons
        else [],
        "topAgents": intel.get("topAgents") or [],
        "form": intel.get("form") or [],
        "streak": intel.get("streak"),
        "mapWins": intel.get("mapWins") or {},
        "habits": intel.get("habits"),
        "reads": intel.get("reads") or [],
        "readsReady": int((intel.get("habits") or {}).get("asked") or 0) >= _DEPTH["games"],
    }


class LiveMatch:
    """Who is in your match or your lobby, read from the client and Riot's servers."""

    def __init__(self, auth: Any) -> None:
        """Use `auth` for every call, fetching its tokens now so a closed client fails early."""
        self.auth = auth
        self.auth.headers()
        self.self_puuid = self.auth.puuid

    @staticmethod
    def decode_private(private: Any) -> dict[str, Any]:
        """Return a presence's private part decoded, or isValid False when it won't decode."""
        text = str(private or "")
        if not text or "{" in text:
            return {"isValid": False}
        try:
            got = json.loads(base64.b64decode(text))
        except ValueError:
            # This runs for every friend on every tick, and a presence from
            # another game need not be base64 JSON, so a miss is not logged.
            return {"isValid": False}
        return got if isinstance(got, dict) else {"isValid": False}

    def _own_private(self, presences: list[Any]) -> dict[str, Any] | None:
        own = next(
            (
                p
                for p in presences
                if p.get("puuid") == self.self_puuid and p.get("product") != "league_of_legends"
            ),
            None,
        )
        return None if own is None else self.decode_private(own.get("private"))

    def game_state(self, presences: list[Any]) -> str:
        """Return where you are: MENUS, PREGAME or INGAME, from your own presence."""
        own = self._own_private(presences)
        if own is None:
            return "MENUS"
        return own.get("matchPresenceData", own).get("sessionLoopState", "MENUS")

    def party_map(self, puuids: Any, presences: list[Any]) -> dict[str, Any]:
        """Return each party Riot shows among `puuids`, with its size and members."""
        parties: dict[str, list[Any]] = {}
        for p in presences:
            if p.get("puuid") not in puuids:
                continue
            priv = self.decode_private(p.get("private"))
            if not priv.get("isValid"):
                continue
            if "partyPresenceData" in priv:
                size = priv["partyPresenceData"].get("partySize", 0)
                pid = priv["partyPresenceData"].get("partyId", "")
            else:
                size = priv.get("partySize", 0)
                pid = priv.get("partyId", "")
            if size > 1 and pid:
                parties.setdefault(pid, []).append(p["puuid"])
        own, everyone = self.own_party(presences)
        if own and len(everyone) > 1:
            parties[own] = [m for m in puuids if m in everyone]
        return {pid: m for pid, m in parties.items() if len(m) > 1}

    def own_party(self, presences: list[Any]) -> tuple[str, list[str]]:
        """Your party's id and every member of it, from the party service."""
        own = self._own_private(presences) or {}
        data = own.get("partyPresenceData", own) if own.get("isValid") else {}
        party, size = data.get("partyId", ""), int(data.get("partySize") or 0)
        if not party:
            return "", []
        if (_OWN_PARTY["id"], _OWN_PARTY["size"]) == (party, size) and (
            0 <= time.time() - _OWN_PARTY["at"] < _OWN_PARTY_SECS
        ):
            return party, _OWN_PARTY["members"]
        try:
            answer = self.auth.glz_get(f"/parties/v1/parties/{party}")
        except Exception as e:
            _log(f"party lookup failed: {type(e).__name__}: {e}")
            return party, []
        rows = answer.get("Members") if isinstance(answer, dict) else None
        members = [m["Subject"] for m in rows or [] if isinstance(m, dict) and m.get("Subject")]
        if members:
            _OWN_PARTY.update(id=party, size=size, at=time.time(), members=members)
        return party, members

    def valorant_open(self, presences: list[Any]) -> bool:
        """Return whether VALORANT itself is running, by your own presence in it."""
        return any(
            p.get("puuid") == self.self_puuid and p.get("product") == "valorant" for p in presences
        )

    def party_members(self, presences: list[Any]) -> list[Any]:
        """Return the members of your party, from your friends' presences."""
        alone = [{"puuid": self.self_puuid, "level": 0, "incognito": False}]
        own = self._own_private(presences) or {}
        party = own.get("partyPresenceData", own).get("partyId", "") if own.get("isValid") else ""
        if not party:
            return alone
        levels: dict[str, int] = {}
        for p in presences:
            private = self.decode_private(p.get("private"))
            if private.get("isValid") and (
                private.get("partyPresenceData", private).get("partyId", "") == party
            ):
                levels[p["puuid"]] = private.get("playerPresenceData", private).get(
                    "accountLevel", 0
                )
        for member in self.own_party(presences)[1]:
            levels.setdefault(member, 0)
        return [{"puuid": p, "level": n, "incognito": False} for p, n in levels.items()] or alone

    def _names(self, asked: list[str], *, again: bool = False) -> dict[str, str]:
        try:
            rows = self.auth.pd_put("/name-service/v2/players", asked)
            if again and isinstance(rows, dict) and rows.get("errorCode"):
                rows = self.auth.pd_put("/name-service/v2/players", asked, refresh=True)
        except Exception as e:
            _log(f"name lookup failed for {len(asked)} players: {type(e).__name__}: {e}")
            return {}
        if isinstance(rows, dict) and rows.get("errorCode"):
            _log(f"name service answered {rows.get('errorCode')} for {len(asked)} players")
        found: dict[str, str] = {}
        for row in rows if isinstance(rows, list) else []:
            if not isinstance(row, dict):
                continue
            game, tag = row.get("GameName") or "", row.get("TagLine") or ""
            if row.get("Subject") and game.strip():
                found[row["Subject"]] = f"{game}#{tag}" if tag else game
        return found

    def reveal_names(self, puuids: list[str]) -> dict[str, Any]:
        """Return the Riot IDs Riot's name service gives for `puuids`, kept once known."""
        names: dict[str, str] = {p: _NAME_CACHE[p] for p in puuids if p in _NAME_CACHE}
        asked = [p for p in puuids if p not in names]
        if not asked:
            return names
        found = self._names(asked, again=True)
        missing = [p for p in asked if p not in found]
        if len(missing) <= 3:
            for puuid in missing:
                found.update(self._names([puuid]))
        names.update(found)
        for puuid in asked:
            if puuid in found:
                _cache_put(_NAME_CACHE, _NAME_CACHE_MAX, puuid, found[puuid])
        return names

    def reveal_via_account_api(self, puuid: str) -> str | None:
        """Return a Riot ID from the account API, when RIOT_API_KEY is set, kept a day."""
        kept = _ACCT_CACHE.get(puuid)
        if kept and time.time() < kept[1]:
            return kept[0]
        key = os.getenv("RIOT_API_KEY", "").strip()
        if not key:
            return None
        cluster = ROUTING.get(os.getenv("RIOT_REGION", "na").strip().lower(), "americas")
        url = f"https://{cluster}.api.riotgames.com/riot/account/v1/accounts/by-puuid/{puuid}"
        name: str | None = None
        # Kept for good, unless Riot was busy or out of reach, which says
        # nothing about this player.
        until = float("inf")
        try:
            got = requests.get(url, headers={"X-Riot-Token": key}, timeout=8)
            if got.ok:
                account = got.json()
                if account.get("gameName"):
                    name = "#".join(filter(None, (account["gameName"], account.get("tagLine"))))
            elif got.status_code in (401, 403):
                _log("account-v1 rejected the key (check RIOT_API_KEY)")
            elif got.status_code == 429 or got.status_code >= 500:
                _log(f"account-v1 answered HTTP {got.status_code} for {puuid[:8]}")
                until = time.time() + _RETRY_SECS
        except (requests.RequestException, ValueError, AttributeError) as e:
            _log(f"account-v1 lookup failed for {puuid[:8]}: {type(e).__name__}: {e}")
            until = time.time() + _RETRY_SECS
        _cache_put(_ACCT_CACHE, _KD_CACHE_MAX, puuid, (name, until))
        return name

    def resolve_identity(
        self, puuid: str, name_service: dict[str, Any], ident: dict[str, Any]
    ) -> tuple[str, int, bool]:
        """Return a player's name, level and whether the level is hidden."""
        name = name_service.get(puuid) or self.reveal_via_account_api(puuid)
        return (
            name or _fallback_name(puuid),
            ident.get("AccountLevel", 0) or 0,
            ident.get("HideAccountLevel", False),
        )

    def match_score(self, presences: list[Any]) -> dict[str, Any] | None:
        """Return the score of your match as your presence shows it, or None."""
        own = self._own_private(presences)
        if own is None:
            return None
        for where in (own.get("matchPresenceData", own), own):
            ally = where.get("partyOwnerMatchScoreAllyTeam")
            enemy = where.get("partyOwnerMatchScoreEnemyTeam")
            if ally is not None or enemy is not None:
                ally, enemy = int(ally or 0), int(enemy or 0)
                return {"ally": ally, "enemy": enemy, "round": ally + enemy + 1}
        return None

    def loadouts(self, state: str, match_id: str) -> dict[str, Any]:
        """Return everyone's equipped weapons and skins in the match or agent select."""
        where = "core-game" if state == "INGAME" else "pregame"
        weapons: dict[str, list[Any]] = {}
        try:
            answer = self.auth.glz_get(f"/{where}/v1/matches/{match_id}/loadouts")
            for entry in answer.get("Loadouts", []):
                inner = (entry.get("Loadout", entry) if state == "INGAME" else entry) or {}
                who = (entry.get("Subject") or inner.get("Subject") or "").lower()
                items = inner.get("Items") or (inner.get("Loadout") or {}).get("Items") or {}
                if who and items:
                    weapons[who] = valapi.loadout_weapons(items)
        except Exception as e:
            _log(f"loadouts failed for match {match_id[:8]}: {type(e).__name__}: {e}")
        return weapons

    def _current_players(self, state: str) -> tuple[list[dict[str, Any]], str, str, str] | None:
        if state not in ("INGAME", "PREGAME"):
            return None
        where = "core-game" if state == "INGAME" else "pregame"
        mid = self.auth.glz_get(f"/{where}/v1/players/{self.self_puuid}").get("MatchID")
        if not mid:
            return None
        match = self.auth.glz_get(f"/{where}/v1/matches/{mid}")
        if state == "INGAME":
            queue = (match.get("MatchmakingData") or {}).get("QueueID", "")
            return match.get("Players", []), mid, match.get("MapID", ""), queue
        ally = match.get("AllyTeam") or {}
        side = ally.get("TeamID", "Blue")
        players = [{**p, "TeamID": side} for p in ally.get("Players", [])]
        return players, mid, match.get("MapID", ""), match.get("QueueID", "")

    def _seasons(self) -> list[dict[str, Any]]:
        now = time.time()
        if _CONTENT_CACHE["seasons"] is not None and now - _CONTENT_CACHE["at"] < 3600:
            return _CONTENT_CACHE["seasons"]
        if now - _CONTENT_CACHE["failed_at"] < _RETRY_SECS:
            return _CONTENT_CACHE["seasons"] or []
        try:
            data = requests.get(
                f"https://shared.{self.auth.shard}.a.pvp.net/content-service/v3/content",
                headers=self.auth.headers(),
                timeout=8,
            ).json()
        except (requests.RequestException, ValueError) as e:
            _log(f"content service failed on shard {self.auth.shard}: {type(e).__name__}: {e}")
            data = None
        seasons = data.get("Seasons", []) if isinstance(data, dict) else []
        if seasons:
            _CONTENT_CACHE["seasons"] = seasons
            _CONTENT_CACHE["at"] = now
        else:
            _CONTENT_CACHE["failed_at"] = now
        return seasons or (_CONTENT_CACHE["seasons"] or [])

    def season_id(self) -> str | None:
        """Return the current act's id."""
        return next(
            (s["ID"] for s in self._seasons() if s.get("IsActive") and s.get("Type") == "act"),
            None,
        )

    def prev_season_id(self) -> str | None:
        """Return the act before the current one's id."""
        acts = [s for s in self._seasons() if s.get("Type") == "act"]
        now = next((a for a in acts if a.get("IsActive")), None)
        if now is None:
            return None
        return next((a["ID"] for a in acts if a.get("EndTime") == now.get("StartTime")), None)

    def _fresh_mids(self, puuid: str, *, background: bool = False) -> tuple[list[str], bool, bool]:
        kept = _MIDS_CACHE.get(puuid)
        if kept and _list_holds(kept[2]):
            return kept[0], kept[1], False
        mids: list[str] = []
        ranked = unsure = False
        for queue in ("competitive", "unrated", "swiftplay", ""):
            page = self.auth.pd_get(
                f"/match-history/v1/history/{puuid}?startIndex=0&endIndex=20"
                + (f"&queue={queue}" if queue else ""),
                retries=0 if background else 3,
                background=background,
            )
            unsure = unsure or _is_throttled(page)
            listed = page.get("History") if isinstance(page, dict) else None
            if not isinstance(listed, list):
                unsure = True
                break
            games = [
                e
                for e in listed
                if e.get("MatchID")
                and str(e.get("QueueID", "competitive")).lower() in STANDARD_QUEUES
            ]
            mids = [e["MatchID"] for e in games]
            for e in games:
                if e.get("GameStartTime"):
                    _cache_put(_STARTS, _STARTS_MAX, e["MatchID"], int(e["GameStartTime"]))
            if unsure or _recent(mids):
                ranked = bool(mids) and queue == "competitive"
                break
        if not unsure:
            _cache_put(_MIDS_CACHE, _KD_CACHE_MAX, puuid, (mids, ranked, time.time()))
        return mids, ranked, unsure

    def _on_hand(self, puuid: str) -> int:
        try:
            mids_all, _, unsure = self._fresh_mids(puuid)
        except Exception as e:
            _log(f"history lookup failed for {puuid[:8]}: {type(e).__name__}: {e}")
            return 0
        mids = _recent(mids_all)[: _DEPTH["games"]]
        if unsure or not mids:
            return 0
        held = 0
        for mid in mids:
            if mid not in _MATCH_DETAIL_CACHE and _kept_file(mid) is None:
                break
            held += 1
        if held == len(mids):
            return _DEPTH["games"]
        return max((g for g in GAMES_READ if g <= held), default=0)

    def rank_info(
        self, puuid: str, season: str | None, prev_season: str | None = None
    ) -> dict[str, Any]:
        """Return a player's rank, rating, peak and previous act, with the act labels."""

        def unknown() -> dict[str, Any]:
            # None for the win rate until Riot says, so a failed lookup reads
            # as unknown and not 0%.
            return {
                "tier": _tier_from_games(puuid, season),
                "rr": 0,
                "lb": 0,
                "peak": 0,
                "wr": None,
                "games": 0,
                "prev": 0,
                "peak_season": season,
                "ok": False,
            }

        try:
            kept = _RANK_CACHE.get(puuid)
            if kept:
                mids, ranked, unsure = self._fresh_mids(puuid)
                if unsure:
                    return kept[0]
                key = mids[0] if mids and ranked else "nocomp"
                if kept[1] is None:
                    _cache_put(_RANK_CACHE, _KD_CACHE_MAX, puuid, (kept[0], key))
                if kept[1] in (None, key):
                    return kept[0]
            else:
                listed = _MIDS_CACHE.get(puuid)
                key = None
                if listed and _list_holds(listed[2]):
                    key = listed[0][0] if listed[0] and listed[1] else "nocomp"
            # Without the act there is no rank to read, and one read as tier 0
            # would be kept as Unranked until their next ranked game.
            if not season or riot_client.held_secs("/mmr/") > 0:
                return kept[0] if kept else unknown()
            mmr = self.auth.pd_get(f"/mmr/v1/players/{puuid}")
            if not isinstance(mmr, dict) or "QueueSkills" not in mmr:
                return kept[0] if kept else unknown()
            rank = _rank_from_mmr(mmr, season, prev_season)
            _cache_put(_RANK_CACHE, _KD_CACHE_MAX, puuid, (rank, key))
        except Exception as e:
            _log(f"rank lookup failed for {puuid[:8]}: {type(e).__name__}: {e}")
            return unknown()
        return rank

    def act_episode(self, season_id: str | None) -> str | None:
        """Return an act's label, like E9 A3, by its season id."""
        if not season_id:
            return None
        if label := valapi.season_label(season_id):
            return label
        episode: dict[str, Any] | None = None
        for entry in self._seasons():
            if (entry.get("Type") or "").lower() == "episode":
                episode = entry
            if entry.get("ID", "").lower() == season_id.lower():
                break
        else:
            return None
        name = str(entry.get("Name") or "")
        act = valapi.act_number(name)
        if act is None:
            return name.title() or None
        ep = valapi.episode_label((episode or {}).get("Name"))
        return f"{ep} Act {act}" if ep else f"Act {act}"

    def level_from_history(self, puuid: str) -> int:
        """Return a player's account level from their match history, kept once found."""
        cached = _LEVEL_CACHE.get(puuid)
        if cached is not None:
            return cached
        level = 0
        try:
            hist = self.auth.pd_get(f"/match-history/v1/history/{puuid}?startIndex=0&endIndex=1")
            entries = (hist or {}).get("History", []) if isinstance(hist, dict) else []
            mid = entries[0].get("MatchID") if entries else None
            if mid:
                md = self.auth.pd_get(f"/match-details/v1/matches/{mid}")
                pl = next((x for x in (md.get("players") or []) if x.get("subject") == puuid), None)
                level = int((pl or {}).get("accountLevel", 0) or 0)
        except Exception as e:
            # A 0 from here looks like an account that really is level 0, so
            # the failure has to leave a trace.
            _log(f"level lookup failed for {puuid[:8]}: {type(e).__name__}: {e}")
        if level > 0:
            _cache_put(_LEVEL_CACHE, _KD_CACHE_MAX, puuid, level)
        return level

    def _detail(self, mid: str, *, keep: bool, month: bool, background: bool) -> Any:
        with _fetching(mid):
            if (hit := _MATCH_DETAIL_CACHE.get(mid)) is not None:
                return hit
            if (kept := _kept_detail(mid)) is not None:
                if month:
                    _keep_detail(mid, kept, month=True)
            else:
                got = self.auth.pd_get(
                    f"/match-details/v1/matches/{mid}",
                    retries=0 if background else 3,
                    background=background,
                )
                if _is_throttled(got) or not (isinstance(got, dict) and "players" in got):
                    return "throttled"
                kept = _slim(got)
                if keep or month:
                    _keep_detail(mid, kept, month=month)
            _DETAIL_READ.add(mid)
            return _remember(mid, kept)

    def kd_hs(
        self, puuid: str, count: int = 3, *, background: bool = False
    ) -> tuple[float | None, float | None, str, dict[str, Any] | None]:
        """Return K/D and headshot share over the last `count` matches."""
        count = max(0, min(count, GAMES_READ[-1]))
        try:
            listed, _, short = self._fresh_mids(puuid, background=background)
            mids = _recent(listed)[:count]
            if not mids:
                return None, None, ("throttled" if short else "empty"), None
            cached = _KD_CACHE.get(puuid)
            if cached and cached[2] == count and list(cached[1])[:count] == mids:
                return cached[0]
            month = puuid in encounter_log.regulars(self.self_puuid)
            keep = puuid in _PARTY
            fetched = list(
                _DETAIL_POOL.map(
                    lambda mid: self._detail(mid, keep=keep, month=month, background=background),
                    mids,
                )
            )
            short = short or any(md == "throttled" for md in fetched)
            games = [
                (mid, md) for mid, md in zip(mids, fetched, strict=True) if _standard(md, puuid)
            ]
            details = [md for _, md in games]
            total = _tally(details, puuid)
            if total is None:
                return None, None, ("throttled" if short else "empty"), None
            habits = habits_of(details, puuid)
            # How many games this read was asked for, 0 for one Riot cut short.
            habits["asked"] = 0 if short else count
            intel = {
                "habits": habits,
                "reads": [read for mid, md in games if (read := _measure(mid, md, puuid))],
                "topAgents": [
                    {"agent": a, "games": n, "role": (resolve_agent(a) or {}).get("role")}
                    for a, n in total["agents"].most_common(3)
                ],
                "form": total["form"],
                "streak": form_streak(total["form"]),
                "mapWins": total["maps"],
            }
            result = (total["kd"], total["hs"], "partial" if short else "ok", intel)
            if not short:
                _cache_put(_KD_CACHE, _KD_CACHE_MAX, puuid, (result, tuple(mids), count))
        except Exception as e:
            _log(f"kd lookup failed for {puuid[:8]}: {type(e).__name__}: {e}")
            return None, None, "error", None
        return result

    def spawn_kd_fill(self, match_id: str, puuids: list[str]) -> None:
        """Fill in K/D for `puuids` on a thread of its own, each player on one at a time."""
        with _KD_FILL_LOCK:
            busy = _KD_FILLING.setdefault(match_id, set())
            puuids = [p for p in puuids if p not in busy]
            if not puuids:
                return
            busy.update(puuids)

        def _fill_one(puuid: str) -> None:
            entry = _CACHE.get(f"{match_id}:{puuid}")
            if entry is None or entry.get("kd_done") or match_id in _ENDED:
                return
            entry["kd_tries"] = entry.get("kd_tries", 0) + 1
            held = self._on_hand(puuid)
            kd, hs, status, intel = self.kd_hs(puuid, count=max(held, 3))
            if kd is not None:
                entry["kd"], entry["hs"] = kd, hs
                entry["intel"] = intel
                entry["kd_done"] = True
                if held > 5:
                    entry["deep"] = held
                refresh.soon()
                return
            _log(f"kd-fill {puuid[:8]} status={status} tries={entry['kd_tries']}")
            if status == "empty" or (status != "throttled" and entry["kd_tries"] >= 6):
                entry["kd_done"] = True

        def _top_up(puuid: str) -> None:
            entry = _CACHE.get(f"{match_id}:{puuid}")
            if entry is None or entry.get("kd_full") or entry.get("kd") is None:
                return
            if match_id in _ENDED:
                return
            try:
                mids, is_comp, _ = self._fresh_mids(puuid)
                rr_key = mids[0] if (mids and is_comp) else "nocomp"
                hit = _RR_CACHE.get(puuid)
                if hit and hit[1] == rr_key:
                    entry["rr_earned"], entry["rr_actual"] = hit[0], hit[2]
                else:
                    cu = self.auth.pd_get(
                        f"/mmr/v1/players/{puuid}/competitiveupdates"
                        f"?startIndex=0&endIndex=1&queue=competitive",
                        retries=1,
                    )
                    m = cu.get("Matches", []) if isinstance(cu, dict) else []
                    if m:
                        entry["rr_earned"] = m[0].get("RankedRatingEarned")
                        entry["rr_actual"] = rr_moved(m[0])
                    if isinstance(cu, dict) and not _is_throttled(cu):
                        _cache_put(
                            _RR_CACHE,
                            _KD_CACHE_MAX,
                            puuid,
                            (entry.get("rr_earned"), rr_key, entry.get("rr_actual")),
                        )
            except Exception as e:
                _log(f"top up failed for {puuid[:8]}: {type(e).__name__}: {e}")
                return
            if entry.get("deep", 5) > 5:
                entry["kd_full"] = True
                return
            if match_id in _ENDED:
                return
            held = self._on_hand(puuid)
            kd, hs, status, intel = self.kd_hs(puuid, count=max(held, 5))
            before = (entry.get("kd"), entry.get("hs"), entry.get("intel"))
            if kd is not None:
                entry["kd"], entry["hs"] = kd, hs
                entry["intel"] = intel
                if held > 5 and status == "ok":
                    entry["deep"] = held
            entry["kd_full"] = True
            if (entry.get("kd"), entry.get("hs"), entry.get("intel")) != before:
                refresh.soon()

        def _deepen(puuid: str) -> bool:
            # One step toward the depth. True when Riot held it back, and the
            # pass waits for room before it goes again.
            entry = _CACHE.get(f"{match_id}:{puuid}")
            if match_id in _ENDED:
                return True
            if entry is None or not entry.get("kd_full") or entry.get("kd") is None:
                return False
            if entry.get("deep", 5) == _DEPTH["games"]:
                return False
            count = _next_rung(entry.get("deep", 5))
            kd, hs, status, intel = self.kd_hs(puuid, count=count, background=True)
            if status in ("throttled", "partial", "error"):
                entry["deep_wait"] = _deeper_at()
                return True
            if kd is not None and (kd, hs, intel) != (
                entry["kd"],
                entry.get("hs"),
                entry.get("intel"),
            ):
                entry["kd"], entry["hs"], entry["intel"] = kd, hs, intel
                refresh.soon()
            entry["deep"] = count
            return False

        def _owed() -> list[tuple[int, str]]:
            with _KD_FILL_LOCK:
                rows = sorted(_KD_FILLING.get(match_id, ()))
            owed = []
            for puuid in rows:
                entry = _CACHE.get(f"{match_id}:{puuid}")
                if entry and entry.get("kd_full") and entry.get("kd") is not None:
                    deep = entry.get("deep", 5)
                    if deep != _DEPTH["games"]:
                        owed.append((deep, puuid))
            return owed

        def _run() -> None:
            try:
                list(_PLAYER_POOL.map(_fill_one, puuids))
                list(_PLAYER_POOL.map(_top_up, puuids))
                # Once everybody has five, the row read from the fewest games
                # takes the next step back, one at a time across every fill
                # of the match. The rows fill out together without a burst
                # Riot would throttle, and the enemies, listed a minute after
                # your team, are not left at five while your team goes to
                # twenty. A step held back waits for room here rather than
                # for the board's next build, which is up to 15 seconds away
                # in a match.
                held: dict[str, int] = {}
                while match_id not in _ENDED:
                    with _DEEPEN_LOCK:
                        owed = [row for row in _owed() if held.get(row[1], 0) < _DEEP_TRIES]
                        if not owed:
                            return
                        puuid = min(owed)[1]
                        if not _deepen(puuid):
                            continue
                    held[puuid] = held.get(puuid, 0) + 1
                    if match_id in _ENDED:
                        return
                    time.sleep(max(0.0, _deeper_at() - time.time()))
            finally:
                with _KD_FILL_LOCK:
                    busy = _KD_FILLING.get(match_id, set())
                    busy.difference_update(puuids)
                    if not busy:
                        _KD_FILLING.pop(match_id, None)

        threading.Thread(target=_run, daemon=True, name=f"kd-fill-{match_id[:8]}").start()

    def build_scoreboard(self) -> dict[str, Any]:
        """Return the board: the lobby in menus, or every player in agent select or a match."""
        presences = riot_client.chat_presences(self.auth)
        state = self.game_state(presences)
        _note_state(state)
        if state == "MENUS":
            return self._menus_board(presences)
        current = self._current_players(state)
        if not current:
            return self._held_board() or _empty_board(state)
        listed, match_id, map_id, queue = current
        puuids = [p["Subject"] for p in listed]
        if state == "PREGAME":
            riot_client.keep_for_first_reads(max(5, len(puuids)))
        meta = _meta_for(match_id)
        names = self._match_names(meta, puuids)
        weapons = self._match_weapons(meta, state, match_id)
        parties = self._match_parties(listed, presences, meta)
        acts = (self.season_id(), self.prev_season_id())
        owed: list[str] = []
        found = {
            got[0]: got[1:]
            for got in _PLAYER_POOL.map(
                lambda p: self._player_entry(p, match_id, acts, names, owed), listed
            )
        }
        if owed:
            self.spawn_kd_fill(match_id, owed)
        players = [
            self._match_row(
                p,
                found[p["Subject"]],
                state=state,
                number=number,
                party=parties.get(p["Subject"]),
                weapons=weapons.get(p["Subject"].lower(), []),
            )
            for number, p in enumerate(listed, 1)
        ]
        score = self.match_score(presences) if state == "INGAME" else None
        if score and (queue or "").lower() in ("deathmatch", "hurm"):
            score["round"] = None
        board = finalize(
            players,
            state=state,
            source="local",
            self_team=next(
                (p["TeamID"] for p in listed if p["Subject"] == self.self_puuid), "Blue"
            ),
            map_name=map_name_from_path(map_id),
            queue=queue,
            match_id=match_id,
            score=score,
        )
        board["riotRequests"] = self.auth.req_count
        board["reading"] = _reading_of(match_id, puuids)
        _save_match(match_id, puuids)
        _LAST_BOARD.update(board=board, at=time.time())
        return board

    def _menus_board(self, presences: list[Any]) -> dict[str, Any]:
        _LAST_BOARD["board"] = None
        board = dict(self.build_lobby(presences))
        if not self.valorant_open(presences):
            # Signed in to the Riot client with VALORANT closed or still
            # starting. Your own row needs only the client's tokens, so it
            # shows now, and the party fills in once the game is up.
            return {**board, "stateLabel": "Signed In", "waiting": "game"}
        board["queue"] = self.queue_status()
        board["map"] = board["queue"].get("map")
        if not board.get("players"):
            # Signed in, with no VALORANT presence yet to read a lobby from.
            board["waiting"] = "game"
        return board

    def _match_names(self, meta: dict[str, Any], puuids: list[str]) -> dict[str, str]:
        names = meta.get("names") or {}
        missing = [p for p in puuids if p not in names]
        if missing and _due(meta.get("name_retry_at", 0.0)):
            names = meta["names"] = names | self.reveal_names(missing)
            if any(p not in names for p in puuids):
                meta["name_retry_at"] = time.time() + NAME_RETRY_SECS
        return names

    def _match_weapons(self, meta: dict[str, Any], state: str, match_id: str) -> dict[str, Any]:
        if meta.get("loadouts") and meta.get("loadouts_for") == state:
            return meta["loadouts"]
        got = self.loadouts(state, match_id)
        if got:
            meta["loadouts"], meta["loadouts_for"] = got, state
        return got or meta.get("loadouts") or {}

    def _match_parties(
        self, listed: list[dict[str, Any]], presences: list[Any], meta: dict[str, Any]
    ) -> dict[str, dict[str, Any]]:
        team_of = {p["Subject"]: p.get("TeamID") for p in listed}
        puuids = list(team_of)
        groups = self.party_map(puuids, presences)
        grouped = {m for members in groups.values() for m in members}
        meta["parties"] = {**meta.get("parties", {}), **_history_parties(puuids, team_of)}
        for party, members in meta["parties"].items():
            if grouped.isdisjoint(members):
                groups[party] = members
                grouped.update(members)
        if self.self_puuid:
            _PARTY.update(
                next((ms for ms in groups.values() if self.self_puuid in ms), [self.self_puuid])
            )
        mine = team_of.get(self.self_puuid, "Blue")
        on_side: Counter[Any] = Counter()
        found: dict[str, dict[str, Any]] = {}
        for number, (party, members) in enumerate(groups.items(), 1):
            side = team_of.get(members[0])
            colour = party_color(on_side[side], ours=side == mine)
            on_side[side] += 1
            for m in members:
                found[m] = {"id": party, "color": colour, "number": number, "size": len(members)}
        return found

    def _player_entry(
        self,
        p: dict[str, Any],
        match_id: str,
        acts: tuple[str | None, str | None],
        names: dict[str, str],
        owed: list[str],
    ) -> tuple[Any, ...]:
        puuid = p["Subject"]
        key = f"{match_id}:{puuid}"
        entry = _CACHE.get(key)
        if entry is None:
            rank = self.rank_info(puuid, *acts)
            entry = {
                "rk": rank,
                "prev": rank.get("prev", 0),
                "kd": None,
                "hs": None,
                "rr_earned": None,
                "kd_done": False,
            }
            if not rank.get("ok"):
                entry["rank_at"] = time.time()
            _cache_put(_CACHE, _KD_CACHE_MAX, key, entry)
            owed.append(puuid)
        else:
            # A rank Riot didn't give is asked for again after 10 seconds.
            asked = time.time() - entry.get("rank_at", 0.0)
            if not entry["rk"].get("ok") and not 0 <= asked <= 10.0:
                rank = self.rank_info(puuid, *acts)
                if rank.get("ok") or rank["tier"]:
                    entry["rk"], entry["prev"] = rank, rank.get("prev", 0)
                if not rank.get("ok"):
                    entry["rank_at"] = time.time()
            if _kd_owed(entry):
                owed.append(puuid)
        name, level, hidden = self.resolve_identity(puuid, names, p.get("PlayerIdentity") or {})
        if (level or 0) <= 0:
            level = self.level_from_history(puuid) or level
        return puuid, entry, name, level, hidden

    def _match_row(
        self,
        p: dict[str, Any],
        found: tuple[Any, ...],
        *,
        state: str,
        number: int,
        party: dict[str, Any] | None,
        weapons: list[Any],
    ) -> dict[str, Any]:
        puuid = p["Subject"]
        ident = p.get("PlayerIdentity") or {}
        entry, name, level, hidden = found
        unnamed = name == _fallback_name(puuid)
        if unnamed:
            agent = (resolve_agent(p.get("CharacterID") or "") or {}).get("name")
            name = agent if state != "PREGAME" and agent else f"Player {number}"
        rank = entry["rk"]
        smurf, why = compute_smurf(
            level=level,
            peak_tier=rank["peak"],
            rank_tier=rank["tier"],
            kd=entry["kd"],
            win_rate=rank["wr"],
            games=rank["games"],
            kd_matches=len((entry.get("intel") or {}).get("form") or []),
            hs=entry["hs"],
        )
        row = assemble_player(
            puuid=puuid,
            name=name,
            name_hidden=unnamed and bool(ident.get("Incognito", False)),
            team=p.get("TeamID", "Blue"),
            is_self=puuid == self.self_puuid,
            agent_id=p.get("CharacterID", ""),
            selection=p.get("CharacterSelectionState") if state == "PREGAME" else None,
            rank_tier=rank["tier"],
            rr=rank["rr"],
            leaderboard=rank["lb"],
            peak_tier=rank["peak"],
            prev_tier=entry["prev"],
            win_rate=rank["wr"],
            games=rank["games"],
            kd=entry["kd"],
            hs=entry["hs"],
            level=level,
            level_hidden=hidden,
            party=party,
            skin=next(
                (w["skin"] for w in weapons if w["weapon"] == "Vandal" and w.get("skin")), None
            ),
            weapons=weapons,
            peak_act=self.act_episode(rank.get("peak_season")),
            rr_earned=entry.get("rr_earned"),
            rr_actual=entry.get("rr_actual"),
            intel=entry.get("intel"),
            player_card=valapi.player_card(ident.get("PlayerCardID")),
            title=valapi.title_text(ident.get("PlayerTitleID")),
            smurf=smurf,
            smurf_reasons=why,
        )
        # Riot not answering yet is no reason to call them Unranked.
        if not rank.get("ok") and not rank["tier"]:
            row["rank"] = ""
        return row

    def _held_board(self) -> dict[str, Any] | None:
        board = _LAST_BOARD.get("board")
        fresh = 0 <= time.time() - _LAST_BOARD.get("at", 0.0) < _HOLD_SECS
        return board if board and fresh else None

    def queue_status(self) -> dict[str, Any]:
        """Return your party's queue state, kept for three seconds."""
        now = time.time()
        kept = _QUEUE_CACHE["data"]
        if kept is not None and now - _QUEUE_CACHE["at"] < 3.0:
            return kept
        try:
            snap = riot_client.party_snapshot(self.auth)
        except Exception as e:
            _log(f"party snapshot failed: {type(e).__name__}: {e}")
            snap = {"available": False}
        if snap.pop("throttled", None) and kept:
            return kept
        _QUEUE_CACHE.update(at=now, data=snap)
        return snap

    def build_lobby(self, presences: list[Any]) -> dict[str, Any]:
        """Return the lobby board: your party, with their ranks and stats."""
        members = self.party_members(presences)
        puuids = [m["puuid"] for m in members]

        key = tuple(sorted(puuids))
        now = time.time()
        if (
            _LOBBY_CACHE["board"] is not None
            and _LOBBY_CACHE["key"] == key
            and now - _LOBBY_CACHE["at"] < 20
        ):
            return _LOBBY_CACHE["board"]

        _PARTY.update(puuids)
        names = self.reveal_names(puuids)
        for puuid in puuids:
            kept = _LOBBY_KEPT.setdefault(puuid, {})
            if names.get(puuid):
                kept["name"] = names[puuid]
            elif kept.get("name"):
                names[puuid] = kept["name"]

        season = self.season_id()
        prev_season = self.prev_season_id()
        multi = len(members) > 1
        party = (
            {"id": "lobby", "color": party_color(0, ours=True), "number": 1, "size": len(members)}
            if multi
            else None
        )

        def fetch_member(m: dict[str, Any]) -> tuple[Any, ...] | None:
            puuid = m["puuid"]
            kept = _LOBBY_KEPT.setdefault(puuid, {})
            rk = self.rank_info(puuid, season, prev_season)
            if rk.get("ok") and season:
                kept["rank"] = rk
            else:
                rk = kept.get("rank", rk)
            have = kept.get("deep", 0)
            count = have or max(self._on_hand(puuid), 5)
            if have and _due(kept.get("deep_wait", 0.0)):
                count = _next_rung(have)
            # Only the first five wait their turn. The rest go when Riot's
            # minute has room for them.
            kd, hs, status, intel = self.kd_hs(puuid, count=count, background=bool(have))
            if status == "ok":
                kept["stats"] = (kd, hs, intel)
                kept["deep"] = count
            elif status != "empty" and "stats" in kept:
                kept["deep_wait"] = _deeper_at()
                kd, hs, intel = kept["stats"]

            level = m.get("level", 0) or 0
            if level <= 0:
                level = self.level_from_history(puuid)
            if level > 0:
                kept["level"] = level
            else:
                level = kept.get("level", 0)
            return m, rk, kd, hs, level, intel

        fetched = [f for f in _PLAYER_POOL.map(fetch_member, members) if f]

        players = []
        for m, rk, kd, hs, lvl, intel in fetched:
            puuid = m["puuid"]
            ident = {
                "AccountLevel": lvl,
                "HideAccountLevel": False,
                "Incognito": m.get("incognito", False),
            }
            name, level, level_hidden = self.resolve_identity(puuid, names, ident)
            smurf, smurf_reasons = compute_smurf(
                level=level,
                peak_tier=rk["peak"],
                rank_tier=rk["tier"],
                kd=kd,
                win_rate=rk["wr"],
                games=rk["games"],
                kd_matches=len((intel or {}).get("form") or []),
                hs=hs,
            )
            players.append(
                assemble_player(
                    puuid=puuid,
                    name=name,
                    name_hidden=False,
                    team="Blue",
                    is_self=(puuid == self.self_puuid),
                    agent_id="",
                    rank_tier=rk["tier"],
                    rr=rk["rr"],
                    leaderboard=rk["lb"],
                    peak_tier=rk["peak"],
                    prev_tier=rk.get("prev", 0),
                    win_rate=rk["wr"],
                    games=rk["games"],
                    kd=kd,
                    hs=hs,
                    intel=intel,
                    level=level,
                    level_hidden=level_hidden,
                    party=party,
                    peak_act=self.act_episode(rk.get("peak_season")),
                    smurf=smurf,
                    smurf_reasons=smurf_reasons,
                )
            )

        board = finalize(
            players,
            state="MENUS",
            source="local",
            self_team="Blue",
            map_name=None,
            queue="Lobby",
            match_id="lobby",
        )
        board["riotRequests"] = self.auth.req_count
        board["reading"] = _reading(
            [(m["puuid"], _LOBBY_KEPT.get(m["puuid"], {}).get("deep")) for m in members]
        )
        _LOBBY_CACHE.update(key=key, at=now, board=board)
        return board

    def player_career(self, puuid: str, count: int = 20) -> dict[str, Any]:
        """Return a player's career: their recent matches and what they add up to."""
        try:
            hist = self.auth.pd_get(
                f"/match-history/v1/history/{puuid}?startIndex=0&endIndex=20", urgent=True
            )
            entries = hist.get("History", []) or [] if isinstance(hist, dict) else []
            answered = isinstance(hist, dict) and isinstance(hist.get("History"), list)
        except requests.RequestException as e:
            _log(f"match history failed for {puuid[:8]}: {type(e).__name__}: {e}")
            entries, answered = [], False
        mids = career_mids(entries, count)

        hit = _CAREER_CACHE.get(puuid)
        if hit and mids and hit[1] == tuple(mids):
            return hit[0]

        def fetch_detail(mid: str) -> Any:
            try:
                md = _MATCH_DETAIL_CACHE.get(mid)
                if md is None and (kept := _kept_detail(mid)):
                    md = _remember(mid, kept)
                if md is None:
                    md = self.auth.pd_get(f"/match-details/v1/matches/{mid}", urgent=True)
                    if isinstance(md, dict) and "players" in md:
                        md = _remember(mid, _slim(md))
                        _DETAIL_READ.add(mid)
                return self.career_match(md, puuid, mid)
            except Exception as e:
                _log(f"career match {mid[:8]} failed: {type(e).__name__}: {e}")
                return None

        matches: list[dict[str, Any]] = []
        mate_puuids: set[str] = set()
        if mids:
            for row in _DETAIL_POOL.map(fetch_detail, mids):
                if row:
                    matches.append(row)
                    mate_puuids.update(m["puuid"] for m in row["teammates"])

        names = self.reveal_names(list(mate_puuids)) if mate_puuids else {}
        for row in matches:
            for mate in row["teammates"]:
                mate["name"] = names.get(mate["puuid"]) or _fallback_name(mate["puuid"])

        updates: dict[str, Any] = {}
        rated = False
        try:
            cu = self.auth.pd_get(
                f"/mmr/v1/players/{puuid}/competitiveupdates?startIndex=0&endIndex=20&queue=competitive",
                urgent=True,
            )
            rated = isinstance(cu, dict) and isinstance(cu.get("Matches"), list)
            for update in (cu or {}).get("Matches", []) or []:
                if update.get("MatchID"):
                    updates[update["MatchID"]] = update
        except (requests.RequestException, AttributeError, TypeError) as e:
            _log(f"rating history failed for {puuid[:8]}: {type(e).__name__}: {e}")
            updates = {}
        for row in matches:
            update = updates.get(str(row.get("matchId") or ""))
            if not update:
                continue
            tier = update.get("TierAfterUpdate")
            rank = rank_from_tier(tier or 0)
            row.update(
                {
                    "rrDelta": update.get("RankedRatingEarned"),
                    "tierAfter": tier,
                    "rrAfter": update.get("RankedRatingAfterUpdate"),
                    "rankAfter": rank.get("name"),
                }
            )

        out = {
            "source": "local",
            "puuid": puuid,
            "matches": matches,
            "rating": rating_run(list(updates.values())),
            **career_summary(matches),
        }
        # Only cache a complete answer. Half a career, because Riot throttled
        # three of the fetches, would otherwise stick until the next match.
        if rated and mids and len(matches) == len(mids):
            _cache_put(_CAREER_CACHE, _KD_CACHE_MAX, puuid, (out, tuple(mids)))
        # No games is the player's history, unless Riot never sent it or sent
        # none of the games in it, which says nothing about them.
        if not matches and (mids or not answered):
            out["unread"] = True
        return out

    def career_match(self, md: dict[str, Any], puuid: str, mid: str) -> dict[str, Any] | None:
        """Return one match as a career row for `puuid`, or None if they weren't in it."""
        players = md.get("players", []) or []
        me = next((p for p in players if p.get("subject") == puuid), None)
        if me is None:
            return None
        info = md.get("matchInfo", {}) or {}
        teams = {t.get("teamId"): t for t in md.get("teams", []) if t.get("teamId")}
        side = me.get("teamId")
        ours = teams.get(side, {})
        rounds = rounds_played(md)
        shots = _shots_by_player(md)
        line = me.get("stats", {}) or {}
        kills, deaths = line.get("kills", 0), line.get("deaths", 0)
        agent = resolve_agent(me.get("characterId") or "") or {}
        party = me.get("partyId")
        return {
            "matchId": mid or info.get("matchId", ""),
            "map": map_name_from_path(info.get("mapId", "")),
            "mode": _mode_label(info.get("queueID") or info.get("queueId") or ""),
            "startMillis": info.get("gameStartMillis", 0),
            "result": _RESULT.get(ours.get("won"), "Draw"),
            "team": side,
            "score": ours.get("roundsWon", 0),
            "agent": agent.get("name"),
            "buys": buy_habits(md, puuid),
            "gunKills": _kills_by_gun(md, puuid),
            "agentColor": agent.get("color", "#8B978F"),
            "kills": kills,
            "deaths": deaths,
            "assists": line.get("assists", 0),
            "kd": _kd(kills, deaths),
            "acs": _per_round(line.get("score", 0), rounds),
            "hsPct": _hs_pct(shots.get(puuid, (0, 0))),
            "partySize": max(1, sum(p.get("partyId") == party for p in players) if party else 1),
            "scores": {team: t.get("roundsWon", 0) for team, t in teams.items()},
            "teammates": [
                _teammate(p, rounds, shots)
                for p in players
                if p.get("teamId") == side and p.get("subject") != puuid
            ],
        }

    def match_detail(self, match_id: str, subject: str | None = None) -> dict[str, Any]:
        """Return a match's full scoreboard and rounds, as `subject` saw it."""
        md = played(self.auth, match_id)
        if not isinstance(md, dict) or "players" not in md:
            return {"error": "Match details unavailable."}
        info = md.get("matchInfo", {}) or {}
        teams = {t.get("teamId"): t for t in md.get("teams", []) if t.get("teamId")}
        listed = md.get("players", []) or []
        names = self.reveal_names([p.get("subject") for p in listed])
        acts = (self.season_id(), self.prev_season_id())
        ranks: dict[Any, dict[str, Any]] = dict(
            _PLAYER_POOL.map(
                lambda p: (
                    p.get("subject"),
                    self.rank_info(p["subject"], *acts) if p.get("subject") else {},
                ),
                listed,
            )
        )
        rounds = rounds_played(md)
        shots = _shots_by_player(md)
        more = round_stats(md, rounds)
        players = sorted(
            (
                _scoreboard_row(
                    p,
                    names=names,
                    rank=ranks.get(p.get("subject")) or {},
                    rounds=rounds,
                    shots=shots,
                    more=more,
                    subject=subject,
                )
                for p in listed
            ),
            key=lambda row: -row["acs"],
        )
        if players:
            players[0]["isMatchMvp"] = True
        side = next((p["team"] for p in players if p["isSubject"]), None)
        if best := next((p for p in players if p["team"] == side), None):
            best["isTeamMvp"] = True
        mine = next((p for p in listed if p.get("subject") == subject), None) if subject else None
        won = teams.get(mine.get("teamId"), {}).get("won") if mine else None
        team_stats = {}
        for team in teams:
            tiers = [p["rankTier"] for p in players if p["team"] == team and p["rankTier"] > 0]
            tier = round(sum(tiers) / len(tiers)) if tiers else 0
            team_stats[team] = {"avgRankTier": tier, "avgRank": rank_from_tier(tier)["name"]}
        return {
            "matchId": match_id,
            "map": map_name_from_path(info.get("mapId", "")),
            "mode": _mode_label(info.get("queueID") or info.get("queueId") or ""),
            "scores": {team: t.get("roundsWon", 0) for team, t in teams.items()},
            "result": None if won is None else _RESULT.get(won, "Draw"),
            "players": players,
            "teamStats": team_stats,
        }


def _scoreboard_row(
    p: dict[str, Any],
    *,
    names: dict[str, Any],
    rank: dict[str, Any],
    rounds: int,
    shots: dict[Any, tuple[int, int]],
    more: dict[Any, Any],
    subject: str | None,
) -> dict[str, Any]:
    # Riot's JSON, where a missing subject is None.
    who: Any = p.get("subject")
    line = p.get("stats", {}) or {}
    agent = resolve_agent(p.get("characterId") or "") or {}
    identity = p.get("PlayerIdentity") or p.get("playerIdentity") or {}
    now, peak = rank_from_tier(rank.get("tier") or 0), rank_from_tier(rank.get("peak") or 0)
    kills, deaths = line.get("kills", 0), line.get("deaths", 0)
    theirs = more.get(who) or {}
    saved = f"{p.get('gameName')}#{p.get('tagLine')}" if p.get("gameName") else None
    return {
        "puuid": who,
        "name": names.get(who) or saved or _fallback_name(who),
        "team": p.get("teamId"),
        "agent": agent.get("name", "Unknown"),
        "agentColor": agent.get("color", "#8B978F"),
        "kills": kills,
        "deaths": deaths,
        "assists": line.get("assists", 0),
        "kd": _kd(kills, deaths),
        "acs": _per_round(line.get("score", 0), rounds),
        "hsPct": _hs_pct(shots.get(who, (0, 0))),
        **{out: theirs.get(key) for out, key in _ROUND_FIELDS.items()},
        "multiKills": theirs.get("multiKills") or {},
        "weaponKills": theirs.get("weaponKills") or [],
        "rankTier": now["tier"],
        "rank": now["name"],
        "rr": rank.get("rr") or 0,
        "leaderboard": rank.get("lb") or 0,
        "peakRankTier": peak["tier"],
        "peakRank": peak["name"],
        "level": p.get("accountLevel") or identity.get("AccountLevel") or 0,
        "playerCard": valapi.player_card(
            identity.get("PlayerCardID") or p.get("playerCard") or p.get("playerCardId")
        ),
        "isSubject": who == subject,
    }


def _team_stats(team: list[Any]) -> dict[str, Any]:
    tiers = [p["rankTier"] for p in team if (p.get("rankTier") or 0) > 0]
    kds = [p["kd"] for p in team if p.get("kd") is not None]
    rates = [p["winRate"] for p in team if p.get("winRate") is not None]
    tier = statistics.fmean(tiers) if tiers else 0
    return {
        "avgRankTier": round(tier, 2),
        "avgRank": rank_from_tier(round(tier))["name"],
        "avgKd": round(statistics.fmean(kds), 2) if kds else None,
        "avgWinRate": round(statistics.fmean(rates)) if rates else None,
        "size": len(team),
    }


def _win_prob(ours: dict[str, Any], theirs: dict[str, Any]) -> int:
    odds = 50.0
    # A side with no rank read yet averages 0, which isn't Iron, so ranks only
    # count once both sides have some.
    if ours["avgRankTier"] and theirs["avgRankTier"]:
        odds += 5 * (ours["avgRankTier"] - theirs["avgRankTier"])
    if ours["avgKd"] is not None and theirs["avgKd"] is not None:
        odds += 20 * (ours["avgKd"] - theirs["avgKd"])
    return min(95, max(5, round(odds)))


# Riot never labels the side. What it hands over is the team colour, and the
# convention behind it is fixed: Red starts on attack, Blue starts on defence.
SIDE_BY_TEAM = {"Red": "Attack", "Blue": "Defense"}

MODES_WITHOUT_SIDES = {"deathmatch", "ggteam", "hurm"}

HALF_LENGTH = {"competitive": 12, "unrated": 12, "custom": 12}


def side_now(state: str, self_team: Any, queue: Any, round_number: Any) -> str | None:
    """Which side you are on right now, or None when that cannot be known."""
    mode = str(queue or "").lower()
    if mode in MODES_WITHOUT_SIDES:
        return None
    start = SIDE_BY_TEAM.get(str(self_team))
    if not start:
        return None
    if state == "PREGAME":
        return start
    if state != "INGAME":
        return None
    half = HALF_LENGTH.get(mode)
    rounds = round_number if isinstance(round_number, int) and round_number > 0 else None
    if not half or rounds is None or rounds > 2 * half:
        return None
    swapped = (rounds - 1) // half % 2 == 1
    other = "Defense" if start == "Attack" else "Attack"
    return other if swapped else start


def finalize(
    players: list[dict[str, Any]],
    *,
    state: str,
    source: str,
    self_team: Any,
    map_name: str | None,
    queue: Any,
    match_id: Any,
    score: Any = None,
) -> dict[str, Any]:
    """Return the finished board."""
    for p in players:
        here = (p.pop("mapWins", None) or {}).get(map_name) if map_name else None
        won, games = here or (0, 0)
        p["mapWinRate"] = {"winRate": round(100 * won / games), "games": games} if games else None
    players.sort(key=lambda x: (x["team"] != self_team, -x["rankTier"], -(x["level"] or 0)))
    teams = {
        team: [p for p in players if p["team"] == team]
        for team in dict.fromkeys(p["team"] for p in players)
    }
    stats = {team: _team_stats(members) for team, members in teams.items()}
    others = [team for team in stats if team != self_team]
    odds = None
    if state == "INGAME" and self_team in stats and len(others) == 1:
        odds = _win_prob(stats[self_team], stats[others[0]])
    locked = sum(p.get("selection") == "locked" for p in players)
    return {
        "state": state,
        "stateLabel": STATES.get(state, state),
        "source": source,
        "map": map_name,
        "mode": _mode_label(queue),
        "matchId": match_id,
        "selfTeam": self_team,
        "side": side_now(state, self_team, queue, (score or {}).get("round")),
        "players": players,
        "teams": teams,
        "teamStats": stats,
        "winProb": odds,
        "score": score,
        "lockProgress": {"locked": locked, "total": len(players)} if state == "PREGAME" else None,
    }


def _self_check() -> None:
    inside, most = [0], [0]

    def one_fetch(delay: float) -> None:
        time.sleep(delay)
        with _fetching("same"):
            inside[0] += 1
            most[0] = max(most[0], inside[0])
            time.sleep(0.03)
            inside[0] -= 1

    fetches = [threading.Thread(target=one_fetch, args=(n * 0.01,)) for n in range(8)]
    for fetch in fetches:
        fetch.start()
    for fetch in fetches:
        fetch.join()
    check(most[0] == 1 and not _FETCHING, (most, _FETCHING))
    check(_due(time.time() - 1) and not _due(time.time() + 60) and _due(time.time() + 3600))

    check(side_now("PREGAME", "Red", "competitive", None) == "Attack")
    check(side_now("PREGAME", "Blue", "competitive", None) == "Defense")
    check(side_now("PREGAME", "Red", "swiftplay", None) == "Attack")

    # Halves. Twelve rounds a side, so round 12 is still the first half and
    # round 13 is not, and the label has to follow the swap rather than sit on
    # the team colour for the whole match.
    check(side_now("INGAME", "Red", "competitive", 12) == "Attack")
    check(side_now("INGAME", "Red", "competitive", 13) == "Defense")
    check(side_now("INGAME", "Blue", "competitive", 13) == "Attack")
    check(side_now("INGAME", "Red", "competitive", 24) == "Defense")

    check(side_now("INGAME", "Red", "competitive", 25) is None)
    check(side_now("INGAME", "Red", "swiftplay", 3) is None)
    check(side_now("INGAME", "Red", "competitive", None) is None)
    check(side_now("PREGAME", "Blue", "deathmatch", None) is None)
    check(side_now("PREGAME", None, "competitive", None) is None)

    class _CountingAuth:
        puuid = "me"
        shard = "na"

        def __init__(self) -> None:
            self.calls: list[str] = []
            self.history = ["m1", "m2"]

        def headers(self) -> dict[str, str]:
            return {}

        def pd_get(self, path: str, **_: Any) -> Any:
            self.calls.append(path)
            if path.startswith("/match-history"):
                return {
                    "History": [
                        {"MatchID": m, "QueueID": "deathmatch" if m.startswith("dm") else "unrated"}
                        for m in self.history
                    ]
                }
            if path.startswith("/match-details"):
                mid = path.rsplit("/", 1)[-1]
                if mid.startswith("dm"):
                    return {
                        "matchInfo": {"matchId": mid, "queueID": "deathmatch"},
                        "players": [
                            {
                                "subject": "me",
                                "teamId": "me",
                                "stats": {"kills": 40, "deaths": 57, "assists": 0, "score": 0},
                            }
                        ],
                        "teams": [],
                        "roundResults": [],
                    }
                return {
                    "matchInfo": {"matchId": mid, "queueID": "competitive", "gameStartMillis": 1},
                    "players": [
                        {
                            "subject": "me",
                            "teamId": "Blue",
                            "characterId": "",
                            "stats": {"kills": 10, "deaths": 5, "assists": 2, "score": 4000},
                        }
                    ],
                    "teams": [{"teamId": "Blue", "won": True, "roundsWon": 13}],
                    "roundResults": [],
                }
            if "competitiveupdates" in path:
                return {"Matches": []}
            return {}

    _CAREER_CACHE.clear()
    _MATCH_DETAIL_CACHE.clear()
    auth = _CountingAuth()
    lm = LiveMatch(auth)
    first = lm.player_career("me", count=2)
    details = [c for c in auth.calls if c.startswith("/match-details")]
    check(len(details) == 2, auth.calls)
    second = lm.player_career("me", count=2)
    again = [c for c in auth.calls if c.startswith("/match-details")]
    check(len(again) == 2, f"a repeat refetched the details: {again}")
    check(second is first, "the career was rebuilt rather than served from the cache")

    # A new match at the top of the history is a different career, and the one
    # detail that is already in hand still does not go back to Riot.
    auth.calls.clear()
    auth.history = ["m0", "m1"]
    lm.player_career("me", count=2)
    fresh = [c for c in auth.calls if c.startswith("/match-details")]
    check(fresh == ["/match-details/v1/matches/m0"], fresh)

    class _FlakyAuth(_CountingAuth):
        def __init__(self) -> None:
            super().__init__()
            self.history_fails = False
            self.broken: set[str] = set()

        def pd_get(self, path: str, **kw: Any) -> Any:
            if path.startswith("/match-history") and self.history_fails:
                self.calls.append(path)
                return {"httpStatus": 500, "errorCode": "INTERNAL"}
            if path.rsplit("/", 1)[-1] in self.broken:
                self.calls.append(path)
                return {}
            return super().pd_get(path, **kw)

    _MIDS_CACHE.clear()
    _KD_CACHE.clear()
    _MATCH_DETAIL_CACHE.clear()
    flaky = _FlakyAuth()
    flaky.history = ["f1", "f2", "f3"]
    shaky = LiveMatch(flaky)
    flaky.history_fails = True
    check(shaky.kd_hs("me", count=3)[2] == "throttled")
    check("me" not in _MIDS_CACHE, _MIDS_CACHE)
    flaky.history_fails = False
    flaky.broken = {"f2"}
    read = shaky.kd_hs("me", count=3)
    check((read[0], read[2]) == (2.0, "partial"), read)
    flaky.broken = set()
    check(shaky.kd_hs("me", count=3)[2] == "ok")

    _MIDS_CACHE.clear()
    _KD_CACHE.clear()
    flaky.history = ["f1", "dm1", "f3"]
    read = shaky.kd_hs("me", count=3)
    check((read[0], read[2], len((read[3] or {})["form"])) == (2.0, "ok", 2), read)
    check(not any(c.endswith("/dm1") for c in flaky.calls), "a deathmatch's details were fetched")
    flaky.history = ["dm1"]
    _MIDS_CACHE.clear()
    check(shaky.kd_hs("me", count=3)[2] == "empty")
    _MIDS_CACHE.clear()
    _KD_CACHE.clear()

    # A presence that is not base64 JSON of an object reads as invalid rather
    # than raising: bad padding, bytes that are not UTF-8, not JSON, a list.
    good = base64.b64encode(b'{"isValid": true}').decode()
    check(LiveMatch.decode_private(good) == {"isValid": True})
    for raw in (b"\xff\xfe", b"nope", b"[1]"):
        bad = base64.b64encode(raw).decode()
        check(LiveMatch.decode_private(bad) == {"isValid": False}, raw)
    check(LiveMatch.decode_private("not base64!") == {"isValid": False})

    # From here on, mocks stand in for Riot's servers.
    import tempfile
    from pathlib import Path
    from unittest import mock

    with mock.patch.object(
        brief, "match_reads", side_effect=lambda md, _: {"matchId": md["matchInfo"]["matchId"]}
    ):
        for missing in ("dm-skipped", "unavailable"):
            _MIDS_CACHE.clear()
            _KD_CACHE.clear()
            _MATCH_DETAIL_CACHE.clear()
            _READS_CACHE.clear()
            flaky.history = ["probe1", missing, "probe3"]
            flaky.broken = {"unavailable"}
            brief_result = shaky.kd_hs("me", count=3)
            check(
                [r["matchId"] for r in (brief_result[3] or {})["reads"]] == ["probe1", "probe3"],
                brief_result,
            )
            check(f"{missing}:me" not in _READS_CACHE, _READS_CACHE)
            check(_READS_CACHE["probe3:me"] == {"matchId": "probe3"}, _READS_CACHE)
            if missing == "unavailable":
                flaky.broken.clear()
                brief_result = shaky.kd_hs("me", count=3)
                check(
                    [r["matchId"] for r in (brief_result[3] or {})["reads"]] == flaky.history,
                    brief_result,
                )
        _MIDS_CACHE.clear()
        _KD_CACHE.clear()
        _MATCH_DETAIL_CACHE.clear()
        _READS_CACHE.clear()
        flaky.history = [f"limit{i}" for i in range(30)]
        flaky.calls.clear()
        brief_result = shaky.kd_hs("me", count=999)
        check(len((brief_result[3] or {})["reads"]) == 20, brief_result)
        fetched = [c for c in flaky.calls if c.startswith("/match-details")]
        check(len(fetched) == 20, flaky.calls)
        shaky.kd_hs("me", count=999)
        check(fetched == [c for c in flaky.calls if c.startswith("/match-details")])
        _KD_CACHE.clear()
        _READS_CACHE.clear()
        check(shaky.kd_hs("me", count=999) == brief_result, "cached details changed the read")
        check(fetched == [c for c in flaky.calls if c.startswith("/match-details")])
    _MIDS_CACHE.clear()
    _KD_CACHE.clear()
    _MATCH_DETAIL_CACHE.clear()
    _READS_CACHE.clear()

    waiting, release = threading.Event(), threading.Event()
    asked: list[str] = []

    def slow(puuid: str, **_: Any) -> tuple[float, None, str, dict[str, Any]]:
        asked.append(puuid)
        if puuid == "ally":
            waiting.set()
            release.wait(10)
        return 1.0, None, "ok", {}

    _CACHE["both:ally"] = {"kd": None, "kd_done": False}
    _CACHE["both:enemy"] = {"kd": None, "kd_done": False}
    with (
        mock.patch.object(lm, "kd_hs", side_effect=slow),
        mock.patch.object(lm, "_on_hand", return_value=0),
    ):
        lm.spawn_kd_fill("both", ["ally"])
        check(waiting.wait(10), "the first fill never started")
        lm.spawn_kd_fill("both", ["ally", "enemy"])
        deadline = time.time() + 10
        while "enemy" not in asked and time.time() < deadline:
            time.sleep(0.01)
        check(asked[:2] == ["ally", "enemy"] and asked.count("ally") == 1, asked)
        release.set()
        for t in threading.enumerate():
            if t.name == "kd-fill-both":
                t.join()
    check(not _KD_FILLING, _KD_FILLING)
    check(_CACHE["both:enemy"]["kd"] == 1.0, _CACHE["both:enemy"])

    started, go_on = threading.Event(), threading.Event()
    steps: list[tuple[str, int]] = []

    def deeper(puuid: str, count: int, **_: Any) -> tuple[float, None, str, dict[str, Any]]:
        steps.append((puuid, count))
        if (puuid, count) == ("a", 10):
            started.set()
            go_on.wait(10)
        return 1.0, None, "ok", {}

    for row in ("a", "b"):
        _CACHE[f"even:{row}"] = {"kd": 1.0, "kd_done": True, "kd_full": True, "deep": 5}
    with mock.patch.object(lm, "kd_hs", side_effect=deeper):
        lm.spawn_kd_fill("even", ["a"])
        check(started.wait(10), "the first row never stepped back")
        lm.spawn_kd_fill("even", ["b"])
        go_on.set()
        for t in threading.enumerate():
            if t.name == "kd-fill-even":
                t.join()
    check(steps == [("a", 10), ("b", 10), ("a", 20), ("b", 20)], steps)
    check(_CACHE.pop("even:a")["deep"] == _CACHE.pop("even:b")["deep"] == 20)

    # A top up that loses its connection is logged and owed again, rather than
    # killing the fill thread with the player marked done.
    def fill(match_id: str) -> None:
        lm.spawn_kd_fill(match_id, ["me"])
        for t in threading.enumerate():
            if t.name == f"kd-fill-{match_id[:8]}":
                t.join()

    entry: dict[str, Any] = {"kd": 1.0, "hs": None, "rr_earned": None, "kd_done": True}
    _CACHE["topup:me"] = entry
    served = auth.pd_get

    def reset(path: str, **kw: Any) -> Any:
        if "competitiveupdates" in path:
            msg = "connection reset"
            raise requests.ConnectionError(msg)
        return served(path, **kw)

    with (
        mock.patch.object(auth, "pd_get", side_effect=reset),
        mock.patch.object(threading, "excepthook") as crashed,
    ):
        fill("topup")
    check(not crashed.called, crashed.call_args)
    check("kd_full" not in entry, entry)
    # A dodged match's fill asks Riot for nothing more.
    _ENDED.add("topup")
    with mock.patch.object(auth, "pd_get") as sent:
        fill("topup")
    check(not sent.called, sent.call_args)
    _ENDED.discard("topup")
    with mock.patch.object(refresh, "soon") as woke:
        fill("topup")
    check(entry["kd_full"] and entry["kd"] == 2.0, entry)
    check(woke.called, "a finished top up didn't wake the board loop")
    check(entry.get("deep") == 20, entry)
    set_depth(4)
    fill("topup")
    check(entry.get("deep") == 5, entry)
    set_depth(20)
    del entry["kd_full"], entry["deep"]
    counts: list[int] = []

    def in_hand(_puuid: str, count: int, **_: Any) -> tuple[float, None, str, dict[str, Any]]:
        counts.append(count)
        return 2.0, None, "ok", {}

    with (
        mock.patch.object(lm, "_on_hand", return_value=20),
        mock.patch.object(lm, "kd_hs", side_effect=in_hand),
    ):
        fill("topup")
    check(counts == [20] and entry["deep"] == 20 and entry["kd_full"], (counts, entry))
    check([_next_rung(have) for have in (5, 10, 20)] == [10, 20, 20])
    now_ms = int(time.time() * 1000)
    _STARTS.update(new=now_ms, old=now_ms - 40 * 86400 * 1000)
    check(_recent(["new", "old", "unknown"]) == ["new", "unknown"])
    set_depth(20, days=0)
    check(_recent(["new", "old"]) == ["new", "old"])
    set_depth(20)
    _CACHE["m:p"] = {"deep": 20}
    set_depth(20)
    check(_CACHE["m:p"]["deep"] == 20, _CACHE["m:p"])
    set_depth(20, days=7)
    check(_CACHE.pop("m:p")["deep"] == 0)
    set_depth(20)
    _MIDS_CACHE["kept"] = (["m1"], True, time.time())
    for state in ("PREGAME", "INGAME", "INGAME"):
        _note_state(state)
    check("kept" in _MIDS_CACHE)
    found = _STATE_SEEN["matched"]
    with mock.patch("time.time", return_value=found + 3000):
        check(_list_holds(found - 60) and not _list_holds(found - 600))
    _note_state("MENUS")
    check("kept" not in _MIDS_CACHE, _MIDS_CACHE)
    check(_list_holds(time.time() - 60) and not _list_holds(time.time() - 600), "menus")
    # The reading counts what is in hand: one row with 15 of its 20 games,
    # one with its history but no games, which shares two with the first,
    # and one not read at all. Nothing left to read says nothing.
    _MIDS_CACHE["a"] = ([f"g{i}" for i in range(20)], True, time.time())
    _MIDS_CACHE["b"] = ([f"g{i}" for i in range(18, 38)], True, time.time())
    for i in range(15):
        _MATCH_DETAIL_CACHE.put(f"g{i}", {})
    got = _reading([("a", 15), ("b", 5), ("c", None)])
    check(got is not None and got["progress"] == round(15 / 60, 3), got)
    check(got is not None and got["games"] == 20 and got["doneAt"] > time.time(), got)
    check(_reading([("a", 20)]) is not None)
    for i in range(15, 20):
        _MATCH_DETAIL_CACHE.put(f"g{i}", {})
    check(_reading([("a", 20)]) is None)
    for key in ("a", "b"):
        _MIDS_CACHE.pop(key)
    _MATCH_DETAIL_CACHE.clear()
    _MIDS_CACHE["x"] = (["last"], True, time.time())
    _MATCH_DETAIL_CACHE.put(
        "last",
        {
            "players": [
                {"subject": "x", "partyId": "d"},
                {"subject": "y", "partyId": "d"},
                {"subject": "z", "partyId": "e"},
            ]
        },
    )
    together = _history_parties(["x", "y", "z"], {"x": "Red", "y": "Red", "z": "Red"})
    check(together == {"d": ["x", "y"]}, together)
    check(_history_parties(["x", "y"], {"x": "Red", "y": "Blue"}) == {})
    _MIDS_CACHE.pop("x")
    _MATCH_DETAIL_CACHE.clear()

    def presence(puuid: str) -> dict[str, Any]:
        private = {"isValid": True, "partyPresenceData": {"partyId": "p1", "partySize": 5}}
        return {"puuid": puuid, "private": base64.b64encode(json.dumps(private).encode()).decode()}

    stack = ["me", "f1", "s1", "s2", "s3"]
    listed = {"Members": [{"Subject": s} for s in stack]}
    _OWN_PARTY.update(id="", at=0.0, members=[])
    with mock.patch.object(auth, "glz_get", create=True, return_value=listed) as glz:
        parties = lm.party_map([*stack, "enemy"], [presence("me"), presence("f1")])
        check(parties == {"p1": stack}, parties)
        lobby = {m["puuid"] for m in lm.party_members([presence("me"), presence("f1")])}
        check(lobby == set(stack), lobby)
        check(glz.call_count == 1, glz.call_args_list)
        grown = {"isValid": True, "partyPresenceData": {"partyId": "p1", "partySize": 4}}
        lm.own_party(
            [{"puuid": "me", "private": base64.b64encode(json.dumps(grown).encode()).decode()}]
        )
        check(glz.call_count == 2, glz.call_args_list)

    good_rank = {
        "tier": 12,
        "rr": 40,
        "lb": 0,
        "peak": 15,
        "wr": 50,
        "games": 20,
        "prev": 0,
        "peak_season": "s1",
        "ok": True,
    }
    good_intel = {"form": ["W", "L"], "habits": {"matches": 2, "rounds": 40}}
    lookups = {
        "rank_info": [good_rank, {**good_rank, "tier": 0, "ok": False}, good_rank],
        "kd_hs": [
            (1.4, 22, "ok", good_intel),
            (None, None, "error", None),
            (0.9, 10, "partial", {"form": ["W"], "habits": {"matches": 1}}),
        ],
        "reveal_names": [{"me": "Me#1"}, {}, {"me": "Me#1"}],
    }
    _LOBBY_KEPT.clear()
    seen = []
    with (
        mock.patch.object(auth, "req_count", 0, create=True),
        mock.patch.object(lm, "party_members", return_value=[{"puuid": "me", "level": 40}]),
        mock.patch.object(lm, "season_id", return_value="s1"),
        mock.patch.object(lm, "prev_season_id", return_value=None),
        mock.patch.object(lm, "act_episode", return_value=None),
        mock.patch.object(lm, "rank_info", side_effect=lookups["rank_info"]),
        mock.patch.object(lm, "kd_hs", side_effect=lookups["kd_hs"]),
        mock.patch.object(lm, "reveal_names", side_effect=lookups["reveal_names"]),
    ):
        for _ in range(3):
            _LOBBY_CACHE.update(board=None, key=None, at=0.0)
            me = lm.build_lobby([])["players"][0]
            seen.append((me["rankTier"], me["kd"], me["name"], me["form"]))
    check(seen == [(12, 1.4, "Me#1", ["W", "L"])] * 3, seen)

    # A rank the act's entry is missing comes from their latest ranked game,
    # and one Riot won't give yet stands in from a game in hand.
    ranker = object.__new__(LiveMatch)
    ranker.auth = mock.Mock()
    ranker.auth.pd_get.return_value = {
        "QueueSkills": {"competitive": {"SeasonalInfoBySeasonID": {}}},
        "LatestCompetitiveUpdate": {
            "SeasonID": "s1",
            "TierAfterUpdate": 15,
            "RankedRatingAfterUpdate": 40,
        },
    }
    _RANK_CACHE.clear()
    got = ranker.rank_info("r1", "s1")
    check((got["ok"], got["tier"], got["rr"]) == (True, 15, 40), got)
    _RANK_CACHE.clear()
    ranker.auth.pd_get.return_value = {"errorCode": "RATE_LIMITED", "status": 429}
    _MIDS_CACHE["r2"] = (["rg"], True, time.time())
    _MATCH_DETAIL_CACHE.put(
        "rg",
        {"matchInfo": {"seasonId": "s1"}, "players": [{"subject": "r2", "competitiveTier": 18}]},
    )
    got = ranker.rank_info("r2", "s1")
    check((got["ok"], got["tier"]) == (False, 18), got)
    check(ranker.rank_info("r2", "s0")["tier"] == 0, "a game from another act stood in")
    _MIDS_CACHE.pop("r2")
    _MATCH_DETAIL_CACHE.clear()

    _NAME_CACHE.clear()
    asker = object.__new__(LiveMatch)
    asker.auth = mock.Mock()
    asker.auth.pd_put.return_value = [{"Subject": "a", "GameName": "Ace", "TagLine": "1"}]
    check(asker.reveal_names(["a", "b"]) == {"a": "Ace#1"})
    asker.auth.pd_put.reset_mock()
    asker.auth.pd_put.return_value = []
    check(asker.reveal_names(["a", "b"]) == {"a": "Ace#1"})
    check([c.args[1] for c in asker.auth.pd_put.call_args_list] == [["b"], ["b"]])
    _NAME_CACHE.clear()

    # With the content service down, the act is asked for twice a tick and
    # only the first of those goes to Riot.
    with mock.patch.object(requests, "get", side_effect=requests.ConnectionError("down")) as get:
        check(lm.season_id() is None)
        check(lm.prev_season_id() is None)
    check(get.call_count == 1, get.call_args_list)

    # A timeout or a 429 from account-v1 is asked again a minute later, and
    # only a real answer keeps for the session.
    sova = mock.Mock(ok=True, status_code=200)
    sova.json.return_value = {"gameName": "Sova", "tagLine": "NA1"}
    answers = [requests.Timeout("slow"), mock.Mock(ok=False, status_code=429), sova]
    with (
        mock.patch.dict(os.environ, {"RIOT_API_KEY": "key"}),
        mock.patch.object(requests, "get", side_effect=answers) as get,
        mock.patch.object(time, "time", return_value=1000.0) as clock,
    ):
        for _ in answers[:2]:
            check(lm.reveal_via_account_api("them") is None)
            clock.return_value += _RETRY_SECS - 1
            check(lm.reveal_via_account_api("them") is None)
            clock.return_value += 1
        check(lm.reveal_via_account_api("them") == "Sova#NA1")
        clock.return_value += 86400
        check(lm.reveal_via_account_api("them") == "Sova#NA1")
    check(get.call_count == 3, get.call_args_list)

    # The name service is Riot's server and gets the access token, so its
    # certificate is checked. Only 127.0.0.1 may skip that.
    local = object.__new__(riot_client.LocalAuth)
    local.pd_url, local.kept_headers, local.req_count = "https://pd.na.a.pvp.net", {"A": "b"}, 0
    with mock.patch.object(requests.Session, "put") as put:
        put.return_value.json.return_value = []
        local.pd_put("/name-service/v2/players", ["me"])
    check(put.call_args.kwargs.get("verify", True) is True, put.call_args)

    with tempfile.TemporaryDirectory() as tmp:
        shooter_log = Path(tmp) / "ShooterGame.log"
        shooter_log.write_text("CI server version: \n", encoding="utf-8")
        down = requests.ConnectionError("down")
        with (
            mock.patch.object(riot_client, "_shooter_log_path", return_value=shooter_log),
            mock.patch.object(local, "local_get", side_effect=down),
            mock.patch.object(requests, "get", side_effect=down),
        ):
            check(local.client_version() == "release-09.00")
    with (
        mock.patch.object(local, "local_get", side_effect=down),
        mock.patch.object(requests, "get") as answer,
    ):
        answer.return_value.json.return_value = {"data": {"riotClientVersion": "release-10.00"}}
        check(local.client_version() == "release-10.00")

    with tempfile.TemporaryDirectory() as tmp, mock.patch(f"{__name__}._KEPT_DIR", Path(tmp)):
        mid = "12345678-1234-1234-1234-123456789abc"
        _KEEP["days"] = 14
        _keep_detail(mid, {"players": [], "matchInfo": {"gameStartMillis": time.time() * 1000}})
        check(_kept_detail(mid) == {"players": [], "matchInfo": mock.ANY}, "kept and read back")
        check(_kept_detail("../outside") is None, "only Riot's ids name files")
        old = time.time() - 15 * 86400
        os.utime(Path(tmp) / f"{mid}.json.gz", (old, old))
        check(_kept_detail(mid) is None, "expired cache must not be read before pruning")
        set_depth(20, days=30, keep=14)
        check(_kept_detail(mid) is None, "kept past 14 days")
        long, short = Path(tmp) / f"{mid}{_MONTH_SUFFIX}", Path(tmp) / f"{mid}.json.gz"
        game: dict[str, Any] = {"players": [], "matchInfo": {"gameStartMillis": old * 1000}}
        _keep_detail(mid, game)
        check(not short.exists(), "a party game kept at 15 days old")
        _keep_detail(mid, game, month=True)
        check(_kept_detail(mid) == game and _prune_kept() and long.exists(), "not kept a month")
        os.utime(long, (time.time(), time.time()))
        _keep_detail(mid, game)
        check(not short.exists(), "kept twice")
        short.write_bytes(long.read_bytes())
        long.unlink()
        _keep_detail(mid, game, month=True)
        check(long.exists() and not short.exists(), "the party's copy wasn't replaced")
        os.utime(long, (old - 16 * 86400, old - 16 * 86400))
        check(_kept_detail(mid) is None and _prune_kept() and not long.exists(), "past a month")
        set_depth(20, keep=0)
        _keep_detail(mid, game, month=True)
        check(not long.exists(), "kept with keeping turned off")
        set_depth(20)
        riot_asked: list[str] = []
        answers = [{"errorCode": "DOWN"}, game | {"players": [{"subject": "me"}]}]

        def riot(path: str, **_: Any) -> Any:
            riot_asked.append(path)
            return answers.pop(0)

        mine = mock.Mock(pd_get=riot, puuid="me")
        game["matchInfo"]["gameStartMillis"] = time.time() * 1000
        _MATCH_DETAIL_CACHE.clear()
        check(played(mine, mid) == {"errorCode": "DOWN"} and mid not in _MATCH_DETAIL_CACHE)
        first = played(mine, mid)
        check(first["players"] == [{"subject": "me"}] and played(mine, mid) == first, first)
        check(len(riot_asked) == 2 and short.exists(), (riot_asked, short.exists()))
        _MATCH_DETAIL_CACHE.clear()
        check(played(mine, mid) == first and len(riot_asked) == 2, "not read from disk")
        _MATCH_DETAIL_CACHE.clear()

    _kill = {
        "victim": "b",
        "roundTime": 9,
        "finishingDamage": {"damageItem": "v"},
        "victimLocation": {"x": 0, "y": 0},
        "playerLocations": [{"subject": "a", "location": {"x": 300, "y": 400}}],
    }
    _slimmed = _slim(
        {
            "players": [{"subject": "a", "stats": {"kills": 1}, "roundDamage": [{}]}],
            "kills": [_kill],
            "roundResults": [
                {
                    "winningTeam": "Red",
                    "playerEconomies": [{}],
                    "playerStats": [{"subject": "a", "kills": [_kill]}],
                }
            ],
        }
    )
    check(
        _slimmed
        == {
            "players": [{"subject": "a", "stats": {"kills": 1}}],
            "roundResults": [
                {"winningTeam": "Red", "playerStats": [{"subject": "a", "kills": [_kill]}]}
            ],
        },
        _slimmed,
    )

    _check_tally_and_rank()
    _check_kept_limits()
    _check_brief_positions()
    _check_saved_match()

    print(
        "live_match self-check OK (sides, career cached, party games kept, match picked up, "
        "failed lookups retried, name service over TLS)"
    )


def _check_tally_and_rank() -> None:

    def played(*, won: bool, kills: int, deaths: int, heads: int, body: int) -> dict[str, Any]:
        return {
            "matchInfo": {"mapId": "/Game/Maps/Ascent/Ascent"},
            "teams": [{"teamId": "Red", "won": won}, {"teamId": "Blue", "won": not won}],
            "players": [
                {
                    "subject": "me",
                    "teamId": "Red",
                    "characterId": "",
                    "stats": {"kills": kills, "deaths": deaths},
                },
                {"subject": "them", "teamId": "Blue", "stats": {"kills": 99, "deaths": 1}},
            ],
            "roundResults": [
                {
                    "playerStats": [
                        {"subject": "me", "damage": [{"headshots": heads, "bodyshots": body}]},
                        {"subject": "them", "damage": [{"headshots": 50}]},
                    ]
                }
            ],
        }

    total = present(
        _tally(
            [
                played(won=True, kills=20, deaths=10, heads=6, body=14),
                played(won=False, kills=10, deaths=10, heads=0, body=20),
            ],
            "me",
        )
    )
    check((total["kd"], total["hs"], total["form"]) == (1.5, 15, ["W", "L"]), total)
    check(list(total["maps"].values()) == [[1, 2]], total["maps"])
    check(_tally([played(won=True, kills=1, deaths=0, heads=0, body=0)], "nobody") is None)

    old_act = next(iter(BEFORE_ASCENDANT))
    mmr = {
        "QueueSkills": {
            "competitive": {
                "SeasonalInfoBySeasonID": {
                    "now": {
                        "CompetitiveTier": 15,
                        "RankedRating": 40,
                        "NumberOfWins": 6,
                        "NumberOfGames": 10,
                        "WinsByTier": {"15": 6},
                    },
                    "last": {"CompetitiveTier": 12},
                    old_act: {"WinsByTier": {"21": 1}},
                }
            }
        }
    }
    rank = _rank_from_mmr(mmr, "now", "last")
    want = {"tier": 15, "rr": 40, "peak": 24, "peak_season": old_act, "wr": 60, "prev": 12}
    check({k: rank[k] for k in want} == want, rank)
    latest = {"SeasonID": "new", "TierAfterUpdate": 9, "RankedRatingAfterUpdate": 77}
    rank = _rank_from_mmr({"QueueSkills": {}, "LatestCompetitiveUpdate": latest}, "new", None)
    check((rank["tier"], rank["rr"], rank["wr"]) == (9, 77, None), rank)


def _check_brief_positions() -> None:
    import tempfile
    from unittest import mock

    mid = "12345678-1234-1234-1234-123456789abc"
    md = {
        "matchInfo": {
            "matchId": mid,
            "queueID": "competitive",
            "gameStartMillis": time.time() * 1000,
        },
        "players": [{"subject": f"p{i}", "teamId": "Red" if i < 5 else "Blue"} for i in range(10)],
        "roundResults": [
            {
                "roundNum": 0,
                "winningTeam": "Red",
                "winningTeamRole": "Attacker",
                "playerStats": [
                    {
                        "subject": "p0",
                        "kills": [
                            {
                                "killer": "p0",
                                "victim": "p5",
                                "roundTime": 1000,
                                "finishingDamage": {"damageType": "Weapon"},
                                "victimLocation": {"x": 0, "y": 0},
                                "playerLocations": [
                                    {"subject": "p0", "location": {"x": 300, "y": 400}},
                                    {"subject": "p1", "location": {"x": 0, "y": 0}},
                                ],
                            }
                        ],
                    }
                ],
            }
        ],
    }
    places = brief.Places(
        {"callouts": [{"superRegionName": "Attacker Side", "location": {"x": 0, "y": 0}}]}
    )

    def read(detail: dict[str, Any]) -> dict[str, Any]:
        result = brief.match_reads(
            detail, "p0", places=places, weapon_name=lambda _: None, map_name=lambda _: "Fixture"
        )
        check(result is not None, "fixture must have a Tendencies read")
        return result or {}

    expected = read(md)
    check(expected["sums"]["kill_dist_mean"] == [500.0, 1.0], expected)
    check(expected["positional"]["attack"]["forward"] == 500.0, expected)
    check(expected["positional"]["attack"]["separation"] == 500.0, expected)
    slimmed = _slim(md)
    check(read(slimmed) == expected, "slimming changed Tendencies")
    cache = MatchCache(max_entries=1, max_bytes=1024 * 1024)
    cache.put(mid, slimmed)
    check(cache.get(mid) == slimmed, "memory cache changed match details")
    check(read(cache.get(mid) or {}) == expected, "the memory cache changed Tendencies")
    with tempfile.TemporaryDirectory() as tmp, mock.patch(f"{__name__}._KEPT_DIR", Path(tmp)):
        _keep_detail(mid, slimmed)
        check(read(_kept_detail(mid) or {}) == expected, "the disk cache changed Tendencies")
    _READS_CACHE.clear()
    whole = brief.match_reads

    def offline(detail: dict[str, Any], who: str) -> dict[str, Any] | None:
        return whole(
            detail, who, places=places, weapon_name=lambda _: None, map_name=lambda _: "Fixture"
        )

    with mock.patch.object(brief, "match_reads", side_effect=offline):
        lean = _remember(mid, {**slimmed, "teams": [{"teamId": "Red", "won": True}]})
    kills = [k for rr in lean["roundResults"] for ps in rr["playerStats"] for k in ps["kills"]]
    check(kills and not any("playerLocations" in k for k in kills), "cached without positions")
    check(_MATCH_DETAIL_CACHE.get(mid) == lean, "the lean copy is what is cached")
    _MATCH_DETAIL_CACHE.clear()
    check(_READS_CACHE[f"{mid}:p0"] == expected, "measured before the positions went")
    check(slimmed["roundResults"][0]["playerStats"][0]["kills"][0]["playerLocations"], "kept whole")
    _READS_CACHE.clear()


def _check_saved_match() -> None:
    import tempfile
    from unittest import mock

    with (
        tempfile.TemporaryDirectory() as tmp,
        mock.patch(f"{__name__}._SAVED_FILE", f"{tmp}/m.json"),
    ):
        entry = {"rk": {"tier": 20}, "kd": 1.2, "kd_done": True, "kd_full": True, "deep": 20}
        _CACHE["live:a"] = dict(entry)
        _MATCH_META["live"] = {
            "names": {"a": "A#1"},
            "parties": {"d": ["a", "b"]},
            "name_retry_at": 5,
        }
        _LEVEL_CACHE["a"] = 40
        _MIDS_CACHE["a"] = (["g1"], True, 7.0)
        _save_match("live", ["a", "b"])
        check(Path(f"{tmp}/m.json").exists(), "the match wasn't saved")
        stamp = read_json(f"{tmp}/m.json", {})["at"]
        _save_match("live", ["a", "b"])
        check(read_json(f"{tmp}/m.json", {})["at"] == stamp, "an unchanged match was written again")
        _CACHE.pop("live:a")
        _MATCH_META.pop("live")
        _LEVEL_CACHE.pop("a")
        _MIDS_CACHE.pop("a")
        check(_restore_match("other") == {}, "another match's rows came back")
        check("other:a" not in _CACHE)
        meta = _restore_match("live")
        check(meta == {"names": {"a": "A#1"}, "parties": {"d": ["a", "b"]}}, meta)
        check(_CACHE.get("live:a") == entry, _CACHE.get("live:a"))
        check(_LEVEL_CACHE.get("a") == 40 and _MIDS_CACHE.get("a") == (["g1"], True, 7.0))
        Path(f"{tmp}/m.json").write_text("{not json", encoding="utf-8")
        check(_restore_match("live") == {}, "a broken file is a fresh start")
        _CACHE.pop("live:a")
        _LEVEL_CACHE.pop("a")
        _MIDS_CACHE.pop("a")


def _check_kept_limits() -> None:
    import tempfile
    from unittest import mock

    def match(index: int) -> tuple[str, dict[str, Any]]:
        return f"{index:08x}-1234-1234-1234-123456789abc", {
            "players": [],
            "matchInfo": {"gameStartMillis": (time.time() - 100 + index) * 1000},
        }

    with (
        tempfile.TemporaryDirectory() as tmp,
        mock.patch(f"{__name__}._KEPT_DIR", Path(tmp)),
        mock.patch(f"{__name__}._KEPT_MAX_MATCHES", 2),
    ):
        games = [match(i) for i in range(8)]
        for mid, md in games[:3]:
            _keep_detail(mid, md)
        check(_kept_detail(games[0][0]) is None, "oldest match evicted at the count limit")
        check(_kept_detail(games[2][0]) == games[2][1], "newest match retained")
        sizes = [p.stat().st_size for p in Path(tmp).glob("*.json.gz")]
        with mock.patch(f"{__name__}._KEPT_MAX_BYTES", max(sizes)):
            check(_prune_kept(), "cache can be pruned to the byte limit")
            check(len(list(Path(tmp).glob("*.json.gz"))) == 1, "byte limit evicts a match")
        with mock.patch(f"{__name__}._KEPT_MAX_BYTES", 1):
            _keep_detail(*games[3])
            check(_kept_detail(games[3][0]) is None, "oversized match not persisted")
        # A locked file that cannot be evicted must not allow an over-budget write.
        with (
            mock.patch(f"{__name__}._KEPT_MAX_MATCHES", 1),
            mock.patch.object(Path, "unlink", side_effect=PermissionError("locked")),
        ):
            _keep_detail(*games[3])
        check(_kept_detail(games[3][0]) is None, "failed eviction prevents a new write")
        abandoned = Path(tmp) / f".{games[0][0]}.123.tmp"
        abandoned.write_bytes(b"interrupted write")
        with ThreadPoolExecutor(max_workers=6) as pool:
            list(pool.map(lambda game: _keep_detail(*game), games))
        check(len(list(Path(tmp).glob("*.json.gz"))) == 2, "concurrent writes respect the cap")
        check(not list(Path(tmp).glob("*.tmp")), "no temporary files remain")
        set_depth(20, keep=0)
        check(not list(Path(tmp).iterdir()), "disabling retention clears the cache")
        _keep_detail(*games[0])
        check(not list(Path(tmp).iterdir()), "disabled retention never writes")
        set_depth(20)


if __name__ == "__main__":
    _self_check()

# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""The live board: who is in your match, their ranks, history and parties.

Reads the VALORANT client's local API and Riot's servers, with caches and
throttles so a lobby costs as few calls as it can.
"""

from __future__ import annotations

import base64
import json
import os
import threading
import time
from concurrent.futures import ThreadPoolExecutor
from typing import Any

import refresh
import requests
import riot_client
import valapi
from agents import UUID_TO_NAME, resolve_agent
from career import career_mids, career_summary, form_streak, rating_run
from common import console_logger
from common.check import check
from rounds import buy_habits, habits_of, round_stats
from smurf import compute_smurf
from vconstants import GAMEMODES, ROUTING, STATES, map_name_from_path, party_color, rank_from_tier


def _mode_label(queue: str) -> str:
    if not queue:
        return "Custom"
    return GAMEMODES.get(queue.lower(), queue.replace("_", " ").title())


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

_LOBBY_CACHE: dict[str, Any] = {"key": None, "at": 0.0, "board": None}

# The last good answer for each account seen in the lobby. A lookup that
# fails, to a dropped connection or Riot throttling, says nothing about the
# player, so the row keeps what it had rather than going blank for a rebuild.
_LOBBY_KEPT: dict[str, dict[str, Any]] = {}

_LAST_BOARD: dict[str, Any] = {"board": None, "at": 0.0}
_HOLD_SECS = 90.0

# Your own party as the party service lists it. The board is built every
# second and a party cannot change mid-match, so it is asked for every so often.
_OWN_PARTY: dict[str, Any] = {"id": "", "at": 0.0, "members": []}
_OWN_PARTY_SECS = 30.0

# How long a failed lookup waits before it is asked again. The board asks on
# every tick, and a service that is down costs 8 seconds a request.
_RETRY_SECS = 60.0

# A Riot ID from account-v1 and the time it may be asked for again. A real
# answer keeps for the session.
_ACCT_CACHE: dict[str, tuple[str | None, float]] = {}

_CONTENT_CACHE: dict[str, Any] = {"seasons": None, "at": 0.0, "failed_at": 0.0}

_LEVEL_CACHE: dict[str, int] = {}

_KD_FILL_LOCK = threading.Lock()
_KD_FILLING: set[str] = set()

_KD_CACHE: dict[str, tuple[tuple[Any, ...], tuple[Any, ...], int]] = {}
_KD_CACHE_MAX = 300

_MIDS_CACHE: dict[str, tuple[list[str], bool, float]] = {}
_MIDS_TTL = 60.0

_RANK_CACHE: dict[str, tuple[dict[str, Any], str]] = {}
_RR_CACHE: dict[str, tuple[Any, ...]] = {}

_MATCH_DETAIL_CACHE: dict[str, dict[str, Any]] = {}
_MATCH_DETAIL_MAX = 200

# A finished career per player, with the match ids it was built from. Eight
# match details is the dearest thing the panel asks for, and the answer cannot
# change until they play another match, so a repeat costs only the history
# lookup. There is no expiry because a new match changes the ids.
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
        while len(cache) >= cap:
            cache.pop(next(iter(cache)), None)
        cache[key] = value


_QUEUE_CACHE: dict[str, Any] = {"at": 0.0, "data": None}
# Riot IDs found, by puuid. A lobby asked the name service for the same
# names every tick, and a name doesn't change while you look at it.
_NAME_CACHE: dict[str, str] = {}
_NAME_CACHE_MAX = 500


_log = console_logger("reveal")


def _is_throttled(resp: Any) -> bool:
    return isinstance(resp, dict) and resp.get("status") == 429


def _fallback_name(puuid: str) -> str:
    return f"Player-{(puuid or '????')[:4].upper()}"


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
        "topAgents": intel.get("topAgents") or [],
        "form": intel.get("form") or [],
        "streak": intel.get("streak"),
        "mapWins": intel.get("mapWins") or {},
        "habits": intel.get("habits"),
    }


class LiveMatch:
    """Who is in your match or your lobby, read from the client and Riot's servers."""

    def __init__(self, auth: Any) -> None:
        """Use `auth` for every call, fetching its tokens now so a closed client fails early."""
        self.auth = auth
        self.auth.headers()
        self.self_puuid = self.auth.puuid

    def _presences(self) -> list[Any]:
        return riot_client.chat_presences(self.auth)

    @staticmethod
    def decode_private(private: Any) -> dict[str, Any]:
        """Return a presence's private part decoded, or isValid False when it won't decode."""
        if not private or "{" in str(private):
            return {"isValid": False}
        try:
            decoded = json.loads(base64.b64decode(str(private)).decode("utf-8"))
        except ValueError:
            # This runs for every friend on every tick, and a presence from
            # another game need not be base64 JSON, so a miss is not logged.
            return {"isValid": False}
        return decoded if isinstance(decoded, dict) else {"isValid": False}

    def game_state(self, presences: list[Any]) -> str:
        """Return where you are: MENUS, PREGAME or INGAME, from your own presence."""
        for p in presences:
            if p.get("puuid") != self.self_puuid:
                continue
            if p.get("product") == "league_of_legends":
                continue
            priv = self.decode_private(p.get("private"))
            if "matchPresenceData" in priv:
                return priv["matchPresenceData"].get("sessionLoopState", "MENUS")
            return priv.get("sessionLoopState", "MENUS")
        return "MENUS"

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
        # Presences only cover friends, so a stack with anyone else in it
        # would come out smaller than it is.
        own, everyone = self.own_party(presences)
        if own and len(everyone) > 1:
            parties[own] = [m for m in puuids if m in everyone]
        return {pid: m for pid, m in parties.items() if len(m) > 1}

    def own_party(self, presences: list[Any]) -> tuple[str, list[str]]:
        """Your party's id and every member of it, from the party service."""
        own = ""
        for p in presences:
            if p.get("puuid") == self.self_puuid:
                priv = self.decode_private(p.get("private"))
                if priv.get("isValid"):
                    own = priv.get("partyPresenceData", priv).get("partyId", "")
                break
        if not own:
            return "", []
        if _OWN_PARTY["id"] == own and time.time() - _OWN_PARTY["at"] < _OWN_PARTY_SECS:
            return own, _OWN_PARTY["members"]
        try:
            party = self.auth.glz_get(f"/parties/v1/parties/{own}")
        except Exception as e:
            # Presences still give the friends in it, so this costs little.
            _log(f"party lookup failed: {type(e).__name__}: {e}")
            return own, []
        rows = party.get("Members") if isinstance(party, dict) else None
        members = [m["Subject"] for m in rows or [] if isinstance(m, dict) and m.get("Subject")]
        if members:
            _OWN_PARTY.update(id=own, at=time.time(), members=members)
        return own, members

    def valorant_open(self, presences: list[Any]) -> bool:
        """Return whether VALORANT itself is running, by your own presence in it."""
        return any(
            p.get("puuid") == self.self_puuid and p.get("product") == "valorant" for p in presences
        )

    def party_members(self, presences: list[Any]) -> list[Any]:
        """Return the members of your party, from your friends' presences."""

        def _fields(priv: dict[str, Any]) -> tuple[str, int]:
            data = priv.get("partyPresenceData", priv)
            pid = data.get("partyId", "")
            player = priv.get("playerPresenceData", priv)
            return pid, player.get("accountLevel", 0)

        my_party = None
        for p in presences:
            if p.get("puuid") == self.self_puuid:
                priv = self.decode_private(p.get("private"))
                if priv.get("isValid"):
                    my_party = _fields(priv)[0]
                break
        if not my_party:
            return [{"puuid": self.self_puuid, "level": 0, "incognito": False}]

        members = []
        for p in presences:
            priv = self.decode_private(p.get("private"))
            if not priv.get("isValid"):
                continue
            pid, level = _fields(priv)
            if pid == my_party:
                members.append({"puuid": p["puuid"], "level": level, "incognito": False})
        # Anybody who isn't a friend has no presence, and the party service
        # still lists them. Their level comes from their history later.
        seen = {m["puuid"] for m in members}
        members += [
            {"puuid": m, "level": 0, "incognito": False}
            for m in self.own_party(presences)[1]
            if m not in seen
        ]
        return members or [{"puuid": self.self_puuid, "level": 0, "incognito": False}]

    def reveal_names(self, puuids: list[str]) -> dict[str, Any]:
        """Return the Riot IDs Riot's name service gives for `puuids`, kept once known."""
        names: dict[str, str] = {p: _NAME_CACHE[p] for p in puuids if p in _NAME_CACHE}
        asked = [p for p in puuids if p not in names]
        if not asked:
            return names

        def _ingest(rows: Any) -> None:
            if not isinstance(rows, list):
                return
            for entry in rows:
                if not isinstance(entry, dict):
                    continue
                subj = entry.get("Subject")
                game, tag = entry.get("GameName") or "", entry.get("TagLine") or ""
                if subj and game.strip():
                    names[subj] = f"{game}#{tag}" if tag else game

        # Names are optional. A network error or an odd row from the name
        # service must not take the board down, so both guards are broad.
        try:
            res = self.auth.pd_put("/name-service/v2/players", asked)
            if isinstance(res, dict) and res.get("errorCode"):
                res = self.auth.pd_put("/name-service/v2/players", asked, refresh=True)
            _ingest(res)
        except Exception as e:
            _log(f"name lookup failed for {len(asked)} players: {type(e).__name__}: {e}")

        missing = [p for p in asked if p not in names]
        if missing and len(missing) <= 3:
            for puuid in missing:
                try:
                    _ingest(self.auth.pd_put("/name-service/v2/players", [puuid]))
                except Exception as e:
                    _log(f"name lookup failed for {puuid[:8]}: {type(e).__name__}: {e}")
        for subject in asked:
            if subject in names:
                _cache_put(_NAME_CACHE, _NAME_CACHE_MAX, subject, names[subject])
        return names

    def reveal_via_account_api(self, puuid: str) -> str | None:
        """Return a Riot ID from the account API, when RIOT_API_KEY is set, kept a day."""
        hit = _ACCT_CACHE.get(puuid)
        if hit and time.time() < hit[1]:
            return hit[0]
        key = os.getenv("RIOT_API_KEY", "").strip()
        if not key:
            return None
        cluster = ROUTING.get(os.getenv("RIOT_REGION", "na").strip().lower(), "americas")
        name = None
        retry_at = float("inf")
        try:
            r = requests.get(
                f"https://{cluster}.api.riotgames.com/riot/account/v1/accounts/by-puuid/{puuid}",
                headers={"X-Riot-Token": key},
                timeout=8,
            )
            if r.ok:
                j = r.json()
                gn, tl = j.get("gameName"), j.get("tagLine")
                if gn:
                    name = f"{gn}#{tl}" if tl else gn
            elif r.status_code in (401, 403):
                _log("account-v1 rejected the key (check RIOT_API_KEY)")
            elif r.status_code == 429 or r.status_code >= 500:
                # Riot being busy says nothing about this player.
                _log(f"account-v1 answered HTTP {r.status_code} for {puuid[:8]}")
                retry_at = time.time() + _RETRY_SECS
        except (requests.RequestException, ValueError, AttributeError) as e:
            # ValueError is a body that is not JSON, AttributeError JSON that is not an object.
            _log(f"account-v1 lookup failed for {puuid[:8]}: {type(e).__name__}: {e}")
            retry_at = time.time() + _RETRY_SECS
        _cache_put(_ACCT_CACHE, _KD_CACHE_MAX, puuid, (name, retry_at))
        return name

    def resolve_identity(
        self, puuid: str, name_service: dict[str, Any], ident: dict[str, Any]
    ) -> tuple[str, int, bool]:
        """Return a player's name, level and whether the level is hidden."""
        name = name_service.get(puuid) or self.reveal_via_account_api(puuid)
        level = ident.get("AccountLevel", 0) or 0
        level_hidden = ident.get("HideAccountLevel", False)
        return name or _fallback_name(puuid), level, level_hidden

    def match_score(self, presences: list[Any]) -> dict[str, Any] | None:
        """Return the score of your match as your presence shows it, or None."""
        for p in presences:
            if p.get("puuid") != self.self_puuid:
                continue
            priv = self.decode_private(p.get("private"))
            data = priv.get("matchPresenceData", priv)
            ally = data.get("partyOwnerMatchScoreAllyTeam")
            enemy = data.get("partyOwnerMatchScoreEnemyTeam")
            if ally is None and enemy is None:
                ally = priv.get("partyOwnerMatchScoreAllyTeam")
                enemy = priv.get("partyOwnerMatchScoreEnemyTeam")
            if ally is None and enemy is None:
                return None
            ally, enemy = int(ally or 0), int(enemy or 0)
            return {"ally": ally, "enemy": enemy, "round": ally + enemy + 1}
        return None

    def loadouts(self, state: str, match_id: str) -> dict[str, Any]:
        """Return everyone's equipped weapons and skins in the match or agent select."""
        path = (
            f"/core-game/v1/matches/{match_id}/loadouts"
            if state == "INGAME"
            else f"/pregame/v1/matches/{match_id}/loadouts"
        )
        out: dict[str, list[Any]] = {}
        try:
            ld = self.auth.glz_get(path)
            for entry in ld.get("Loadouts", []):
                subj = (entry.get("Subject") or "").lower()
                loadout = entry.get("Loadout", entry) if state == "INGAME" else entry
                items = (loadout or {}).get("Items", {}) or {}

                if not items and isinstance(loadout, dict):
                    items = (loadout.get("Loadout") or {}).get("Items", {}) or {}
                if subj and items:
                    out[subj] = valapi.loadout_weapons(items)
        except Exception as e:
            # Skins are decoration. An odd loadout shape must not cost the board.
            _log(f"loadouts failed for match {match_id[:8]}: {type(e).__name__}: {e}")
        return out

    def _current_players(self, state: str) -> tuple[list[dict[str, Any]], str, str, str] | None:
        if state == "INGAME":
            cg = self.auth.glz_get(f"/core-game/v1/players/{self.self_puuid}")
            mid = cg.get("MatchID")
            if not mid:
                return None
            match = self.auth.glz_get(f"/core-game/v1/matches/{mid}")
            players = match.get("Players", [])
            queue = (match.get("MatchmakingData") or {}).get("QueueID", "")
            return players, mid, match.get("MapID", ""), queue
        if state == "PREGAME":
            pg = self.auth.glz_get(f"/pregame/v1/players/{self.self_puuid}")
            mid = pg.get("MatchID")
            if not mid:
                return None
            match = self.auth.glz_get(f"/pregame/v1/matches/{mid}")
            ally = match.get("AllyTeam") or {}
            team = ally.get("TeamID", "Blue")
            players = [{**p, "TeamID": team} for p in ally.get("Players", [])]
            return players, mid, match.get("MapID", ""), match.get("QueueID", "")
        return None

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
        for s in self._seasons():
            if s.get("IsActive") and s.get("Type") == "act":
                return s["ID"]
        return None

    def prev_season_id(self) -> str | None:
        """Return the act before the current one's id."""
        seasons = self._seasons()
        current = next((s for s in seasons if s.get("IsActive") and s.get("Type") == "act"), None)
        if not current:
            return None
        for s in seasons:
            if s.get("Type") == "act" and s.get("EndTime") == current.get("StartTime"):
                return s["ID"]
        return None

    def _fresh_mids(self, puuid: str) -> tuple[list[str], bool, bool]:
        """Their newest match ids, whether competitive, and whether unsure.

        Unsure is throttled or failed, which says nothing about the player
        and is never kept.
        """
        hit = _MIDS_CACHE.get(puuid)
        if hit and time.time() - hit[2] < _MIDS_TTL:
            return hit[0], hit[1], False
        mids: list[str] = []
        is_comp = False
        throttled = False
        for queue in ("competitive", "unrated", "swiftplay", ""):
            q = f"&queue={queue}" if queue else ""
            hist = self.auth.pd_get(
                f"/match-history/v1/history/{puuid}?startIndex=0&endIndex=10{q}", retries=3
            )
            throttled = throttled or _is_throttled(hist)
            # An error or an empty body is not an empty history. Read as one,
            # it blanked a player's stats and reads for a minute at a time.
            if not isinstance(hist, dict) or not isinstance(hist.get("History"), list):
                throttled = True
                break
            entries = hist["History"]
            mids = [e["MatchID"] for e in entries if e.get("MatchID")]
            if mids or throttled:
                is_comp = bool(mids) and queue == "competitive"
                break
        if not throttled:
            _cache_put(_MIDS_CACHE, _KD_CACHE_MAX, puuid, (mids, is_comp, time.time()))
        return mids, is_comp, throttled

    def rank_info(
        self, puuid: str, season: str | None, prev_season: str | None = None
    ) -> dict[str, Any]:
        """Return a player's rank, rating, peak and previous act, with the act labels."""
        out = {
            "tier": 0,
            "rr": 0,
            "lb": 0,
            "peak": 0,
            "wr": 0,
            "games": 0,
            "prev": 0,
            "peak_season": season,
            "ok": False,
        }
        try:
            hit = _RANK_CACHE.get(puuid)
            if hit:
                mids, is_comp, unsure = self._fresh_mids(puuid)
                # A history that failed says nothing about a new match, so the
                # rank already in hand stands.
                if unsure:
                    return hit[0]
                rank_key = mids[0] if (mids and is_comp) else "nocomp"
                if hit[1] is None:
                    _cache_put(_RANK_CACHE, _KD_CACHE_MAX, puuid, (hit[0], rank_key))
                    return hit[0]
                if hit[1] == rank_key:
                    return hit[0]
            else:
                mhit = _MIDS_CACHE.get(puuid)
                if mhit and time.time() - mhit[2] < _MIDS_TTL:
                    rank_key = mhit[0][0] if (mhit[0] and mhit[1]) else "nocomp"
                else:
                    rank_key = None
            if riot_client.held_secs("/mmr/") > 0:
                return out
            r = self.auth.pd_get(f"/mmr/v1/players/{puuid}")
            if not isinstance(r, dict) or "QueueSkills" not in r:
                return out
            out["ok"] = True
            si = (
                ((r.get("QueueSkills") or {}).get("competitive") or {}).get(
                    "SeasonalInfoBySeasonID"
                )
            ) or {}
            cur = si.get(season, {}) if season else {}
            out["tier"] = cur.get("CompetitiveTier", 0) or 0
            out["rr"] = cur.get("RankedRating", 0) or 0
            out["lb"] = cur.get("LeaderboardRank", 0) or 0

            if prev_season:
                out["prev"] = (si.get(prev_season, {}) or {}).get("CompetitiveTier", 0) or 0
            peak = int(out["tier"] or 0)
            for s, info in si.items():
                for t in info.get("WinsByTier") or {}:
                    ti = int(t)
                    if s in BEFORE_ASCENDANT and ti > 20:
                        ti += 3
                    if ti > peak:
                        peak = ti
                        out["peak_season"] = s
            out["peak"] = peak
            wins = cur.get("NumberOfWinsWithPlacements", 0) or 0
            games = cur.get("NumberOfGames", 0) or 0
            out["games"] = games
            out["wr"] = round(wins / games * 100) if games else 0
            _cache_put(_RANK_CACHE, _KD_CACHE_MAX, puuid, (out, rank_key))
        except Exception as e:
            # This runs per player in the pool, and an odd MMR answer has to
            # cost that player their rank, not the whole board.
            _log(f"rank lookup failed for {puuid[:8]}: {type(e).__name__}: {e}")
        return out

    def act_episode(self, season_id: str | None) -> str | None:
        """Return an act's label, like E9 A3, by its season id."""
        if not season_id:
            return None
        label = valapi.season_label(season_id)
        if label:
            return label

        seasons = self._seasons()
        act = ep = None
        for s in seasons:
            if (s.get("Type") or "").lower() == "episode":
                ep = s
            if s.get("ID", "").lower() == season_id.lower():
                act = s
                break
        if not act:
            return None
        num = valapi.act_number(str(act.get("Name") or ""))
        ep_label = valapi.episode_label((ep or {}).get("Name"))
        if ep_label and num is not None:
            return f"{ep_label} Act {num}"
        if num is not None:
            return f"Act {num}"
        return (act.get("Name") or "").title() or None

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

    def kd_hs(
        self, puuid: str, count: int = 3
    ) -> tuple[float | None, float | None, Any, str, dict[str, Any] | None]:
        """Return K/D and headshot share over the last `count` matches.

        Then the RR earned, which is None here and filled in later from competitive
        updates, how the read went (ok, partial, empty, throttled or error), and the
        rest: top agents, form, streak and wins per map.
        """
        # The third value, the RR earned, is always None here. _top_up fills it
        # in from competitiveupdates.
        try:
            mids_all, _, throttled = self._fresh_mids(puuid)
            mids = mids_all[:count]
            if not mids:
                return None, None, None, ("throttled" if throttled else "empty"), None
            cached = _KD_CACHE.get(puuid)
            if cached and cached[2] >= count and list(cached[1])[:count] == mids:
                return cached[0]

            def fetch_detail(mid: str) -> Any:
                hit = _MATCH_DETAIL_CACHE.get(mid)
                if hit is not None:
                    return hit
                md = self.auth.pd_get(f"/match-details/v1/matches/{mid}", retries=3)
                if _is_throttled(md):
                    return "throttled"
                if isinstance(md, dict) and "players" in md:
                    _cache_put(_MATCH_DETAIL_CACHE, _MATCH_DETAIL_MAX, mid, md)
                    return md
                # A detail that failed to load, counted like a throttled one.
                return "throttled"

            kills = deaths = hits = heads = used = 0
            agent_counts: dict[str, int] = {}
            form: list[str] = []
            map_wins: dict[str, list[Any]] = {}

            details = list(_DETAIL_POOL.map(fetch_detail, mids))
            for md in details:
                if md == "throttled":
                    throttled = True
                    continue
                if not md:
                    continue
                for rr in md.get("roundResults", []):
                    for ps in rr.get("playerStats", []):
                        if ps.get("subject") == puuid:
                            for dmg in ps.get("damage", []):
                                hits += (
                                    dmg.get("legshots", 0)
                                    + dmg.get("bodyshots", 0)
                                    + dmg.get("headshots", 0)
                                )
                                heads += dmg.get("headshots", 0)
                for pl in md.get("players", []):
                    if pl.get("subject") == puuid:
                        st = pl.get("stats", {})
                        kills += st.get("kills", 0)
                        deaths += st.get("deaths", 0)
                        used += 1

                        aname = UUID_TO_NAME.get((pl.get("characterId") or "").lower())
                        if aname:
                            agent_counts[aname] = agent_counts.get(aname, 0) + 1
                        teams = {t.get("teamId"): t for t in md.get("teams", [])}
                        won = (teams.get(pl.get("teamId")) or {}).get("won")
                        if won is not None:
                            form.append("W" if won else "L")
                            mapn = map_name_from_path(
                                (md.get("matchInfo", {}) or {}).get("mapId", "")
                            )
                            mw = map_wins.setdefault(mapn, [0, 0])
                            mw[1] += 1
                            if won:
                                mw[0] += 1
                        break
            if used == 0:
                return None, None, None, ("throttled" if throttled else "empty"), None
            kd = round(kills / deaths, 2) if deaths else float(kills)
            hs = round(heads / hits * 100) if hits else None
            intel = {
                "habits": habits_of([md for md in details if isinstance(md, dict)], puuid),
                "topAgents": [
                    {"agent": a, "games": n}
                    for a, n in sorted(agent_counts.items(), key=lambda x: -x[1])[:3]
                ],
                "form": form,
                "streak": form_streak(form),
                "mapWins": map_wins,
            }
            # Fewer matches than asked for is a thinner read, and tags that
            # need all of them would come and go. The lobby keeps the last
            # whole read over a partial one.
            whole = used == len(mids)
            result = (kd, hs, None, "ok" if whole else "partial", intel)
            if not throttled and used == len(mids):
                _cache_put(_KD_CACHE, _KD_CACHE_MAX, puuid, (result, tuple(mids), count))
        except Exception as e:
            # This runs per player in the pool, so an odd match detail costs
            # that player their K/D and nothing more.
            _log(f"kd lookup failed for {puuid[:8]}: {type(e).__name__}: {e}")
            return None, None, None, "error", None
        else:
            return result

    def spawn_kd_fill(self, match_id: str, puuids: list[str]) -> None:
        """Fill in K/D for `puuids` on a thread of its own, once per match."""
        with _KD_FILL_LOCK:
            if match_id in _KD_FILLING:
                return
            _KD_FILLING.add(match_id)

        def _fill_one(puuid: str) -> None:
            entry = _CACHE.get(f"{match_id}:{puuid}")
            if entry is None or entry.get("kd_done"):
                return
            entry["kd_tries"] = entry.get("kd_tries", 0) + 1
            kd, hs, _, status, intel = self.kd_hs(puuid, count=3)
            if kd is not None:
                entry["kd"], entry["hs"] = kd, hs
                entry["intel"] = intel
                entry["kd_done"] = True
                # On screen now, not at the board loop's next tick.
                refresh.soon()
                return
            _log(f"kd-fill {puuid[:8]} status={status} tries={entry['kd_tries']}")
            # No history is final. Throttling says nothing about the player, so
            # it never ends the fill. Anything else gets six tries.
            if status == "empty" or (status != "throttled" and entry["kd_tries"] >= 6):
                entry["kd_done"] = True

        def _top_up(puuid: str) -> None:
            entry = _CACHE.get(f"{match_id}:{puuid}")
            if entry is None or entry.get("kd_full") or entry.get("kd") is None:
                return
            try:
                mids, is_comp, _ = self._fresh_mids(puuid)
                rr_key = mids[0] if (mids and is_comp) else "nocomp"
                hit = _RR_CACHE.get(puuid)
                if hit and hit[1] == rr_key:
                    entry["rr_earned"] = hit[0]
                else:
                    cu = self.auth.pd_get(
                        f"/mmr/v1/players/{puuid}/competitiveupdates"
                        f"?startIndex=0&endIndex=1&queue=competitive",
                        retries=1,
                    )
                    m = cu.get("Matches", []) if isinstance(cu, dict) else []
                    if m:
                        entry["rr_earned"] = m[0].get("RankedRatingEarned")
                    if isinstance(cu, dict) and not _is_throttled(cu):
                        _cache_put(
                            _RR_CACHE, _KD_CACHE_MAX, puuid, (entry.get("rr_earned"), rr_key)
                        )
            except Exception as e:
                # kd_full stays unset, so the next board sends them back here.
                _log(f"top up failed for {puuid[:8]}: {type(e).__name__}: {e}")
                return
            kd, hs, _, _, intel = self.kd_hs(puuid, count=5)
            before = (entry.get("kd"), entry.get("hs"), entry.get("intel"))
            if kd is not None:
                entry["kd"], entry["hs"] = kd, hs
                entry["intel"] = intel
            entry["kd_full"] = True
            # Only a top up that changed something is worth a new board.
            if (entry.get("kd"), entry.get("hs"), entry.get("intel")) != before:
                refresh.soon()

        def _run() -> None:
            try:
                list(_PLAYER_POOL.map(_fill_one, puuids))
                list(_PLAYER_POOL.map(_top_up, puuids))
            finally:
                with _KD_FILL_LOCK:
                    _KD_FILLING.discard(match_id)

        threading.Thread(target=_run, daemon=True, name=f"kd-fill-{match_id[:8]}").start()

    def build_scoreboard(self) -> dict[str, Any]:
        """Return the board: the lobby in menus, or every player in agent select or a match."""
        presences = self._presences()
        state = self.game_state(presences)

        if state == "MENUS":
            _LAST_BOARD["board"] = None
            if not self.valorant_open(presences):
                # Signed in to the Riot client with VALORANT closed or still
                # starting, where the lobby would be you alone.
                return {
                    "state": "MENUS",
                    "stateLabel": STATES["MENUS"],
                    "source": "local",
                    "waiting": "game",
                    "players": [],
                    "teams": {},
                }

            board = dict(self.build_lobby(presences))
            board["queue"] = self.queue_status()
            board["map"] = board["queue"].get("map")
            if not board.get("players"):
                # Signed in, with no VALORANT presence yet to read a lobby from.
                board["waiting"] = "game"
            return board
        if state not in ("INGAME", "PREGAME"):
            held = self._held_board()
            return held or {
                "state": state,
                "stateLabel": STATES.get(state, state),
                "source": "local",
                "players": [],
                "teams": {},
            }

        current = self._current_players(state)
        if not current:
            held = self._held_board()
            return held or {
                "state": "MENUS",
                "stateLabel": STATES["MENUS"],
                "source": "local",
                # In a match Riot's servers don't have the players of yet.
                "waiting": "match",
                "players": [],
                "teams": {},
            }

        raw_players, match_id, map_id, queue = current
        puuids = [p["Subject"] for p in raw_players]

        if match_id not in _MATCH_META:
            _MATCH_META.clear()
            _MATCH_META[match_id] = {}
        meta = _MATCH_META[match_id]

        names = meta.get("names") or {}
        missing_names = [p for p in puuids if p not in names]
        if missing_names and meta.get("name_tries", 0) < 8:
            meta["name_tries"] = meta.get("name_tries", 0) + 1
            names = {**names, **self.reveal_names(missing_names)}
            meta["names"] = names
        if not meta.get("loadouts"):
            ld = self.loadouts(state, match_id)
            if ld:
                meta["loadouts"] = ld
            weapons_by_puuid = ld
        else:
            weapons_by_puuid = meta["loadouts"]

        pmap = self.party_map(puuids, presences)
        party_lookup = {}
        parties_out = []
        team_of = {p["Subject"]: p.get("TeamID") for p in raw_players}
        mine = team_of.get(self.self_puuid, "Blue")
        seen_on: dict[Any, int] = {}
        for idx, (pid, members) in enumerate(pmap.items()):
            side = team_of.get(members[0])
            color = party_color(seen_on.get(side, 0), ours=side == mine)
            seen_on[side] = seen_on.get(side, 0) + 1
            parties_out.append(
                {
                    "id": pid,
                    "color": color,
                    "number": idx + 1,
                    "size": len(members),
                    "members": members,
                }
            )
            for m in members:
                party_lookup[m] = {"id": pid, "color": color, "number": idx + 1}

        season = self.season_id()
        prev_season = self.prev_season_id()
        self_team = next(
            (p["TeamID"] for p in raw_players if p["Subject"] == self.self_puuid), "Blue"
        )

        uncached_kd: list[str] = []

        def fetch_player(p: dict[str, Any]) -> tuple[Any, ...] | None:
            puuid = p["Subject"]
            ident = p.get("PlayerIdentity", {}) or {}
            cache_key = f"{match_id}:{puuid}"
            cached = _CACHE.get(cache_key)
            if cached is None:
                rk = self.rank_info(puuid, season, prev_season)

                cached = {
                    "rk": rk,
                    "prev": rk.get("prev", 0),
                    "kd": None,
                    "hs": None,
                    "rr_earned": None,
                    "kd_done": False,
                }
                if not rk.get("ok"):
                    cached["rank_at"] = time.time()
                _cache_put(_CACHE, _KD_CACHE_MAX, cache_key, cached)
                uncached_kd.append(puuid)
            else:
                if not cached["rk"].get("ok") and time.time() - cached.get("rank_at", 0.0) > 20.0:
                    rk = self.rank_info(puuid, season, prev_season)
                    if rk.get("ok"):
                        cached["rk"], cached["prev"] = rk, rk.get("prev", 0)
                    else:
                        cached["rank_at"] = time.time()
                # The fill still owes them a K/D, or the top up that comes after it.
                owed = not cached.get("kd_done") or (
                    cached.get("kd") is not None and not cached.get("kd_full")
                )
                if owed:
                    uncached_kd.append(puuid)
            name, level, level_hidden = self.resolve_identity(puuid, names, ident)

            if (level or 0) <= 0:
                recovered = self.level_from_history(puuid)
                if recovered > 0:
                    level = recovered
            return puuid, cached, name, level, level_hidden

        resolved = {r[0]: r[1:] for r in _PLAYER_POOL.map(fetch_player, raw_players) if r}

        if uncached_kd:
            self.spawn_kd_fill(match_id, uncached_kd)

        players: list[dict[str, Any]] = []
        for p in raw_players:
            puuid = p["Subject"]
            ident = p.get("PlayerIdentity", {}) or {}
            cached, name, level, level_hidden = resolved[puuid]
            if name == _fallback_name(puuid):
                agent_meta = resolve_agent(p.get("CharacterID", "") or "") or {}
                if state != "PREGAME" and agent_meta.get("name"):
                    name = agent_meta["name"]
                else:
                    name = f"Player {len(players) + 1}"
            rk = cached["rk"]
            weapons = weapons_by_puuid.get(puuid.lower(), [])
            vandal = next(
                (w["skin"] for w in weapons if w["weapon"] == "Vandal" and w.get("skin")), None
            )
            smurf, smurf_reasons = compute_smurf(
                level=level,
                peak_tier=rk["peak"],
                rank_tier=rk["tier"],
                kd=cached["kd"],
                win_rate=rk["wr"],
                games=rk["games"],
                kd_matches=len((cached.get("intel") or {}).get("form") or []),
                hs=cached["hs"],
            )
            players.append(
                assemble_player(
                    puuid=puuid,
                    name=name,
                    name_hidden=ident.get("Incognito", False),
                    team=p.get("TeamID", "Blue"),
                    is_self=(puuid == self.self_puuid),
                    agent_id=p.get("CharacterID", ""),
                    selection=p.get("CharacterSelectionState") if state == "PREGAME" else None,
                    rank_tier=rk["tier"],
                    rr=rk["rr"],
                    leaderboard=rk["lb"],
                    peak_tier=rk["peak"],
                    prev_tier=cached["prev"],
                    win_rate=rk["wr"],
                    games=rk["games"],
                    kd=cached["kd"],
                    hs=cached["hs"],
                    level=level,
                    level_hidden=level_hidden,
                    party=party_lookup.get(puuid),
                    skin=vandal,
                    weapons=weapons,
                    peak_act=self.act_episode(rk.get("peak_season")),
                    rr_earned=cached.get("rr_earned"),
                    intel=cached.get("intel"),
                    player_card=valapi.player_card(ident.get("PlayerCardID")),
                    title=valapi.title_text(ident.get("PlayerTitleID")),
                    smurf=smurf,
                    smurf_reasons=smurf_reasons,
                )
            )

        map_name = map_name_from_path(map_id)
        score = self.match_score(presences) if state == "INGAME" else None
        if score and (queue or "").lower() in ("deathmatch", "hurm"):
            score["round"] = None
        board = finalize(
            players,
            state=state,
            source="local",
            self_team=self_team,
            map_name=map_name,
            queue=queue,
            match_id=match_id,
            score=score,
        )
        board["riotRequests"] = self.auth.req_count

        _LAST_BOARD["board"] = board
        _LAST_BOARD["at"] = time.time()
        return board

    def _held_board(self) -> dict[str, Any] | None:
        b = _LAST_BOARD.get("board")
        if b and (time.time() - _LAST_BOARD.get("at", 0.0)) < _HOLD_SECS:
            return b
        return None

    def queue_status(self) -> dict[str, Any]:
        """Return your party's queue state, kept for three seconds."""
        now = time.time()
        if _QUEUE_CACHE["data"] is not None and now - _QUEUE_CACHE["at"] < 3.0:
            return _QUEUE_CACHE["data"]
        try:
            snap = riot_client.party_snapshot(self.auth)
        except Exception as e:
            # The queue panel is extra. An odd party answer must not cost the lobby.
            _log(f"party snapshot failed: {type(e).__name__}: {e}")
            snap = {"available": False}
        if snap.get("throttled") and _QUEUE_CACHE["data"]:
            return _QUEUE_CACHE["data"]
        snap.pop("throttled", None)
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
            # Without the season, an answer that came back reads as unranked.
            rk = self.rank_info(puuid, season, prev_season)
            if rk.get("ok") and season:
                kept["rank"] = rk
            else:
                rk = kept.get("rank", rk)
            kd, hs, _, status, intel = self.kd_hs(puuid, count=5)
            if status == "ok":
                kept["stats"] = (kd, hs, intel)
            elif status != "empty" and "stats" in kept:
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
        _LOBBY_CACHE.update(key=key, at=now, board=board)
        return board

    def player_career(self, puuid: str, count: int = 8) -> dict[str, Any]:
        """Return a player's career: their recent matches and what they add up to."""
        try:
            hist = self.auth.pd_get(f"/match-history/v1/history/{puuid}?startIndex=0&endIndex=20")
            entries = hist.get("History", []) or [] if isinstance(hist, dict) else []
        except requests.RequestException as e:
            _log(f"match history failed for {puuid[:8]}: {type(e).__name__}: {e}")
            entries = []
        mids = career_mids(entries, count)

        hit = _CAREER_CACHE.get(puuid)
        if hit and mids and hit[1] == tuple(mids):
            return hit[0]

        def fetch_detail(mid: str) -> Any:
            try:
                # The board already pulled the newest few of these to work out
                # a K/D, and they are the same documents. Reading them from
                # there is the difference between a career that opens and one
                # you wait for.
                md = _MATCH_DETAIL_CACHE.get(mid)
                if md is None:
                    md = self.auth.pd_get(f"/match-details/v1/matches/{mid}")
                    if isinstance(md, dict) and "players" in md:
                        _cache_put(_MATCH_DETAIL_CACHE, _MATCH_DETAIL_MAX, mid, md)
                return self.career_match(md, puuid, mid)
            except Exception as e:
                # One odd match detail costs that match, not the whole career.
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

        # Asked for every career, not only one with a ranked match in it. The
        # rating line is their last twenty ranked games, while the matches
        # above are the last few of anything, so somebody who mixes in unrated
        # would get a line two points long.
        updates: dict[str, Any] = {}
        try:
            cu = self.auth.pd_get(
                f"/mmr/v1/players/{puuid}/competitiveupdates?startIndex=0&endIndex=20&queue=competitive"
            )
            for update in (cu or {}).get("Matches", []) or []:
                if update.get("MatchID"):
                    updates[update["MatchID"]] = update
        except (requests.RequestException, AttributeError, TypeError) as e:
            # AttributeError and TypeError are an answer that is not a list of match objects.
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
        if mids and len(matches) == len(mids):
            _cache_put(_CAREER_CACHE, _KD_CACHE_MAX, puuid, (out, tuple(mids)))
        return out

    def career_match(self, md: dict[str, Any], puuid: str, mid: str = "") -> dict[str, Any] | None:
        """Return one match as a career row for `puuid`, or None if they weren't in it."""
        info = md.get("matchInfo", {}) or {}
        players = md.get("players", []) or []
        subj = next((p for p in players if p.get("subject") == puuid), None)
        if not subj:
            return None

        st = subj.get("stats", {}) or {}
        team_id = subj.get("teamId")
        teams = {t.get("teamId"): t for t in md.get("teams", []) if t.get("teamId")}
        mine = teams.get(team_id, {})
        won = mine.get("won")
        rounds = max((t.get("roundsWon", 0) for t in teams.values()), default=0) + min(
            (t.get("roundsWon", 0) for t in teams.values()), default=0
        )

        hits_by_player: dict[str, int] = {}
        heads_by_player: dict[str, int] = {}
        guns: dict[str, int] = {}
        for rr in md.get("roundResults", []):
            for ps in rr.get("playerStats", []):
                player_id = ps.get("subject")
                if not player_id:
                    continue
                if player_id == puuid:
                    for kill in ps.get("kills") or []:
                        item = (kill.get("finishingDamage") or {}).get("damageItem") or ""
                        if item:
                            guns[item] = guns.get(item, 0) + 1
                for dmg in ps.get("damage", []):
                    hits_by_player[player_id] = (
                        hits_by_player.get(player_id, 0)
                        + dmg.get("legshots", 0)
                        + dmg.get("bodyshots", 0)
                        + dmg.get("headshots", 0)
                    )
                    heads_by_player[player_id] = heads_by_player.get(player_id, 0) + dmg.get(
                        "headshots", 0
                    )

        kills, deaths = st.get("kills", 0), st.get("deaths", 0)
        hits = hits_by_player.get(puuid, 0)
        heads = heads_by_player.get(puuid, 0)
        agent = resolve_agent(subj.get("characterId") or "") or {}
        teammates = []
        for player in players:
            player_id = player.get("subject")
            if player.get("teamId") != team_id or player_id == puuid:
                continue
            player_stats = player.get("stats", {}) or {}
            teammate_agent = resolve_agent(player.get("characterId") or "") or {}
            teammates.append(
                {
                    "puuid": player_id,
                    "agent": teammate_agent.get("name"),
                    "agentColor": teammate_agent.get("color", "#8B978F"),
                    "level": (
                        player.get("accountLevel")
                        or (
                            (
                                player.get("PlayerIdentity") or player.get("playerIdentity") or {}
                            ).get("AccountLevel")
                        )
                    ),
                    "kills": player_stats.get("kills", 0),
                    "deaths": player_stats.get("deaths", 0),
                    "assists": player_stats.get("assists", 0),
                    "acs": round(player_stats.get("score", 0) / rounds) if rounds else 0,
                    "headshots": heads_by_player.get(player_id, 0),
                }
            )
        party_id = subj.get("partyId")
        party_size = sum(p.get("partyId") == party_id for p in players) if party_id else 1
        queue = info.get("queueID") or info.get("queueId") or ""
        map_name = map_name_from_path(info.get("mapId", ""))
        return {
            "matchId": mid or info.get("matchId", ""),
            "map": map_name,
            "mode": _mode_label(queue),
            "startMillis": info.get("gameStartMillis", 0),
            "result": "Victory" if won is True else "Defeat" if won is False else "Draw",
            "team": team_id,
            "score": mine.get("roundsWon", 0),
            # None rather than "Unknown" for a character nobody picks, like the
            # stand-in every player gets in an ability draft.
            "agent": agent.get("name"),
            "buys": buy_habits(md, puuid),
            # What they actually got kills with in this match, best first.
            "gunKills": [
                {"name": valapi.weapon_name(item) or "Ability", "kills": n}
                for item, n in sorted(guns.items(), key=lambda kv: (-kv[1], str(kv[0])))
            ],
            "agentColor": agent.get("color", "#8B978F"),
            "kills": kills,
            "deaths": deaths,
            "assists": st.get("assists", 0),
            "kd": round(kills / deaths, 2) if deaths else float(kills),
            "acs": round(st.get("score", 0) / rounds) if rounds else 0,
            "hsPct": round(heads / hits * 100) if hits else None,
            "partySize": max(1, party_size),
            "scores": {tid: team.get("roundsWon", 0) for tid, team in teams.items()},
            "teammates": teammates,
        }

    def match_detail(self, match_id: str, subject: str | None = None) -> dict[str, Any]:
        """Return a match's full scoreboard and rounds, as `subject` saw it."""
        md = self.auth.pd_get(f"/match-details/v1/matches/{match_id}")
        if not isinstance(md, dict) or "players" not in md:
            return {"error": "Match details unavailable."}
        info = md.get("matchInfo", {}) or {}
        teams = {t.get("teamId"): t for t in md.get("teams", []) if t.get("teamId")}
        rounds = (
            sum(t.get("roundsWon", 0) for t in teams.values())
            or len(md.get("roundResults", []))
            or 1
        )

        hits: dict[str, Any] = {}
        heads: dict[str, Any] = {}
        for rr in md.get("roundResults", []):
            for ps in rr.get("playerStats", []):
                s = ps.get("subject")
                for dmg in ps.get("damage", []):
                    hits[s] = (
                        hits.get(s, 0)
                        + dmg.get("legshots", 0)
                        + dmg.get("bodyshots", 0)
                        + dmg.get("headshots", 0)
                    )
                    heads[s] = heads.get(s, 0) + dmg.get("headshots", 0)

        detail = round_stats(md, rounds)

        raw = md.get("players", []) or []
        names = self.reveal_names([p.get("subject") for p in raw])
        season = self.season_id()
        prev_season = self.prev_season_id()

        def fetch_rank(player: dict[str, Any]) -> tuple[Any, dict[str, Any]]:
            puuid = player.get("subject")
            return puuid, self.rank_info(puuid, season, prev_season) if puuid else {}

        ranks: dict[Any, dict[str, Any]] = dict(_PLAYER_POOL.map(fetch_rank, raw))
        players = []
        for p in raw:
            sub = p.get("subject")
            st = p.get("stats", {}) or {}
            agent = resolve_agent(p.get("characterId") or "") or {}
            identity = p.get("PlayerIdentity") or p.get("playerIdentity") or {}
            rank = ranks.get(sub) or {}
            rank_meta = rank_from_tier(rank.get("tier") or 0)
            peak_meta = rank_from_tier(rank.get("peak") or 0)
            k, d, a = st.get("kills", 0), st.get("deaths", 0), st.get("assists", 0)
            th = hits.get(sub, 0)
            stored = f"{p.get('gameName')}#{p.get('tagLine')}" if p.get("gameName") else None
            players.append(
                {
                    "puuid": sub,
                    "name": names.get(sub) or stored or _fallback_name(sub),
                    "team": p.get("teamId"),
                    "agent": agent.get("name", "Unknown"),
                    "agentColor": agent.get("color", "#8B978F"),
                    "kills": k,
                    "deaths": d,
                    "assists": a,
                    "kd": round(k / d, 2) if d else float(k),
                    "acs": round(st.get("score", 0) / rounds) if rounds else 0,
                    "hsPct": round(heads.get(sub, 0) / th * 100) if th else None,
                    "adr": (detail.get(sub) or {}).get("adr"),
                    "kast": (detail.get(sub) or {}).get("kastPct"),
                    "econ": (detail.get(sub) or {}).get("econ"),
                    "firstBloods": (detail.get(sub) or {}).get("firstBloods"),
                    "firstDeaths": (detail.get(sub) or {}).get("firstDeaths"),
                    "multiKills": (detail.get(sub) or {}).get("multiKills") or {},
                    "topWeapon": (detail.get(sub) or {}).get("topWeapon"),
                    "weaponKills": (detail.get(sub) or {}).get("weaponKills") or [],
                    "clutches": (detail.get(sub) or {}).get("clutches"),
                    "clutchesLost": (detail.get(sub) or {}).get("clutchesLost"),
                    "plants": (detail.get(sub) or {}).get("plants"),
                    "defuses": (detail.get(sub) or {}).get("defuses"),
                    "plantsWon": (detail.get(sub) or {}).get("plantsWon"),
                    "defusesWon": (detail.get(sub) or {}).get("defusesWon"),
                    "shots": (detail.get(sub) or {}).get("shots"),
                    "rankTier": rank_meta["tier"],
                    "rank": rank_meta["name"],
                    "rr": rank.get("rr") or 0,
                    "leaderboard": rank.get("lb") or 0,
                    "peakRankTier": peak_meta["tier"],
                    "peakRank": peak_meta["name"],
                    "level": p.get("accountLevel") or identity.get("AccountLevel") or 0,
                    "playerCard": valapi.player_card(
                        identity.get("PlayerCardID") or p.get("playerCard") or p.get("playerCardId")
                    ),
                    "isSubject": sub == subject,
                }
            )
        players.sort(key=lambda x: -x["acs"])

        won = None
        if subject:
            sp = next((p for p in raw if p.get("subject") == subject), None)
            if sp:
                won = teams.get(sp.get("teamId"), {}).get("won")
        subject_team = next((p.get("team") for p in players if p.get("isSubject")), None)
        if players:
            players[0]["isMatchMvp"] = True
        team_mvp = next((p for p in players if p.get("team") == subject_team), None)
        if team_mvp:
            team_mvp["isTeamMvp"] = True
        team_stats = {}
        for team_id in teams:
            team_players = [p for p in players if p.get("team") == team_id]
            rated = [t for p in team_players if (t := p.get("rankTier") or 0) > 0]
            avg_tier = round(sum(rated) / len(rated)) if rated else 0
            avg_rank = rank_from_tier(avg_tier)
            team_stats[team_id] = {
                "avgRankTier": avg_tier,
                "avgRank": avg_rank["name"],
            }
        map_name = map_name_from_path(info.get("mapId", ""))
        return {
            "matchId": match_id,
            "map": map_name,
            "mode": _mode_label(info.get("queueID") or info.get("queueId") or ""),
            "scores": {tid: t.get("roundsWon", 0) for tid, t in teams.items()},
            "result": (
                "Victory"
                if won is True
                else "Defeat"
                if won is False
                else ("Draw" if won is not None else None)
            ),
            "players": players,
            "teamStats": team_stats,
        }


def _team_stats(team_players: list[Any]) -> dict[str, Any]:
    ranked = [p["rankTier"] for p in team_players if (p.get("rankTier") or 0) > 0]
    kds = [p["kd"] for p in team_players if p.get("kd") is not None]
    wrs = [p["winRate"] for p in team_players if p.get("winRate") is not None]
    avg_tier = sum(ranked) / len(ranked) if ranked else 0
    rank_meta = rank_from_tier(round(avg_tier)) if ranked else rank_from_tier(0)
    return {
        "avgRankTier": round(avg_tier, 2),
        "avgRank": rank_meta["name"],
        "avgKd": round(sum(kds) / len(kds), 2) if kds else None,
        "avgWinRate": round(sum(wrs) / len(wrs)) if wrs else None,
        "size": len(team_players),
    }


def _win_prob(self_stats: dict[str, Any], enemy_stats: dict[str, Any]) -> int:
    prob = 50.0
    prob += (self_stats["avgRankTier"] - enemy_stats["avgRankTier"]) * 5
    self_kd = self_stats["avgKd"]
    enemy_kd = enemy_stats["avgKd"]
    if self_kd is not None and enemy_kd is not None:
        prob += (self_kd - enemy_kd) * 20
    return max(5, min(95, round(prob)))


# Riot never labels the side. What it hands over is the team colour, and the
# convention behind it is fixed: Red starts on attack, Blue starts on defence.
SIDE_BY_TEAM = {"Red": "Attack", "Blue": "Defense"}

# Modes where nobody attacks or defends anything. Saying "Defense" over a
# deathmatch is worse than saying nothing.
MODES_WITHOUT_SIDES = {"deathmatch", "ggteam", "hurm"}

# Rounds before the sides swap, for the modes whose format this has been
# checked against. A mode that is missing here gets no side once the match is
# under way: the starting side is still true in agent select, but which side
# you are on in round nine is a guess without knowing where half time falls.
HALF_LENGTH = {"competitive": 12, "unrated": 12, "custom": 12}


def side_now(state: str, self_team: Any, queue: Any, round_number: Any) -> str | None:
    """Which side you are on right now, or None when that cannot be known."""
    mode = str(queue or "").lower()
    if mode in MODES_WITHOUT_SIDES:
        return None
    start = SIDE_BY_TEAM.get(str(self_team))
    if not start:
        return None
    # Agent select matters most, because the side is what you pick an agent
    # for, and no round has been played yet, so the starting side is the side.
    if state == "PREGAME":
        return start
    if state != "INGAME":
        return None
    half = HALF_LENGTH.get(mode)
    rounds = round_number if isinstance(round_number, int) and round_number > 0 else None
    # Overtime swaps on rules of its own, so past regulation this does not guess.
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
    """Return the finished board.

    Your team first and each team by rank, with each team's stats, your odds of
    winning and which side you are on.
    """
    for p in players:
        mw = p.pop("mapWins", None) or {}
        w, g = (mw.get(map_name) or [0, 0]) if map_name else (0, 0)
        p["mapWinRate"] = {"winRate": round(100 * w / g), "games": g} if g else None
    players.sort(key=lambda x: (x["team"] != self_team, -x["rankTier"], -(x["level"] or 0)))
    teams: dict[str, list[Any]] = {}
    for p in players:
        teams.setdefault(p["team"], []).append(p)

    team_stats = {tid: _team_stats(tp) for tid, tp in teams.items()}

    win_prob = None
    if state == "INGAME" and len(team_stats) == 2 and self_team in team_stats:
        enemy_team = next(t for t in team_stats if t != self_team)
        win_prob = _win_prob(team_stats[self_team], team_stats[enemy_team])

    locked = sum(1 for p in players if p.get("selection") == "locked")

    side = side_now(state, self_team, queue, (score or {}).get("round"))
    return {
        "state": state,
        "stateLabel": STATES.get(state, state),
        "source": source,
        "map": map_name,
        "mode": _mode_label(queue),
        "matchId": match_id,
        "selfTeam": self_team,
        "side": side,
        "players": players,
        "teams": teams,
        "teamStats": team_stats,
        "winProb": win_prob,
        "score": score,
        "lockProgress": {"locked": locked, "total": len(players)} if state == "PREGAME" else None,
    }


def _self_check() -> None:
    # VALORANT is open when your own presence is in it, not just the client.
    lm = LiveMatch.__new__(LiveMatch)
    lm.self_puuid = "me"
    check(lm.valorant_open([{"puuid": "me", "product": "valorant"}]))
    check(not lm.valorant_open([{"puuid": "me", "product": "keystone"}]))
    check(not lm.valorant_open([{"puuid": "friend", "product": "valorant"}]))

    # Sides. Agent select is the case that matters: it is what you pick an
    # agent for, and no round has been played, so the starting side is the
    # side. Red starts on attack, Blue starts on defence.
    check(side_now("PREGAME", "Red", "competitive", None) == "Attack")
    check(side_now("PREGAME", "Blue", "competitive", None) == "Defense")
    # A mode this has never been checked against still knows where it starts.
    check(side_now("PREGAME", "Red", "swiftplay", None) == "Attack")

    # Halves. Twelve rounds a side, so round 12 is still the first half and
    # round 13 is not, and the label has to follow the swap rather than sit on
    # the team colour for the whole match.
    check(side_now("INGAME", "Red", "competitive", 1) == "Attack")
    check(side_now("INGAME", "Red", "competitive", 12) == "Attack")
    check(side_now("INGAME", "Red", "competitive", 13) == "Defense")
    check(side_now("INGAME", "Blue", "competitive", 13) == "Attack")
    check(side_now("INGAME", "Red", "competitive", 24) == "Defense")

    # Where it must not guess: overtime swaps on its own rules, a mode with an
    # unchecked format has no known half time, nobody defends in a deathmatch,
    # and with no team colour there is no side at all.
    check(side_now("INGAME", "Red", "competitive", 25) is None)
    check(side_now("INGAME", "Red", "swiftplay", 3) is None)
    check(side_now("INGAME", "Red", "competitive", None) is None)
    check(side_now("PREGAME", "Blue", "deathmatch", None) is None)
    check(side_now("PREGAME", None, "competitive", None) is None)
    check(side_now("MENUS", "Red", "competitive", None) is None)

    # A career is eight match details, and the same eight every time until they
    # play another match. Twice through has to cost one history lookup and no
    # details at all, or opening the guns section is a wait every time.
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
                return {"History": [{"MatchID": m} for m in self.history]}
            if path.startswith("/match-details"):
                mid = path.rsplit("/", 1)[-1]
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
    # Identity, not equality: the detail cache alone would make a rebuild look
    # free here, and the point of this cache is not paying to parse eight
    # matches again either.
    check(second is first, "the career was rebuilt rather than served from the cache")

    # A new match at the top of the history is a different career, and the one
    # detail that is already in hand still does not go back to Riot.
    auth.calls.clear()
    auth.history = ["m0", "m1"]
    lm.player_career("me", count=2)
    fresh = [c for c in auth.calls if c.startswith("/match-details")]
    check(fresh == ["/match-details/v1/matches/m0"], fresh)

    # A history that answers with an error says nothing about the player.
    # It must not read as no matches, and must not be kept. A read where one
    # detail failed is partial, and the next whole one is ok.
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
    check(shaky.kd_hs("me", count=3)[3] == "throttled")
    check("me" not in _MIDS_CACHE, _MIDS_CACHE)
    flaky.history_fails = False
    flaky.broken = {"f2"}
    read = shaky.kd_hs("me", count=3)
    check((read[0], read[3]) == (2.0, "partial"), read)
    flaky.broken = set()
    check(shaky.kd_hs("me", count=3)[3] == "ok")

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
    # A top up that lands wakes the board loop rather than waiting for it.
    with mock.patch.object(refresh, "soon") as woke:
        fill("topup")
    check(entry["kd_full"] and entry["kd"] == 2.0, entry)
    check(woke.called, "a finished top up didn't wake the board loop")

    # A five stack with one friend in it: presences show two of you, and the
    # party service shows all five, in the match and in the lobby.
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
    # Asked once and kept, since the board is built every second.
    check(glz.call_count == 1, glz.call_args_list)

    # Alone in the lobby, a rebuild whose lookups all fail keeps the rank,
    # the stats and the name the last good one had.
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
            (1.4, 22, None, "ok", good_intel),
            (None, None, None, "error", None),
            # Two of five details came back, so the reads are thinner.
            (0.9, 10, None, "partial", {"form": ["W"], "habits": {"matches": 1}}),
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

    # A name found once isn't asked for again, and one never found still is.
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
    with mock.patch.object(requests, "put") as put:
        put.return_value.json.return_value = []
        local.pd_put("/name-service/v2/players", ["me"])
    check(put.call_args.kwargs.get("verify", True) is True, put.call_args)

    # A version line with nothing after it is no version, and must not stick.
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
    # A later lookup that works then gives its own answer, not the default.
    with (
        mock.patch.object(local, "local_get", side_effect=down),
        mock.patch.object(requests, "get") as answer,
    ):
        answer.return_value.json.return_value = {"data": {"riotClientVersion": "release-10.00"}}
        check(local.client_version() == "release-10.00")

    print(
        "live_match self-check OK (sides, career cached, "
        "failed lookups retried, name service over TLS)"
    )


if __name__ == "__main__":
    _self_check()

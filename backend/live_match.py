from __future__ import annotations

import base64
import json
import os
import threading
import time
from concurrent.futures import ThreadPoolExecutor
from typing import Any

import requests
import riot_client
import valapi
from agents import UUID_TO_NAME, resolve_agent
from common import console_logger
from vconstants import GAMEMODES, ROUTING, STATES, map_name_from_path, party_color, rank_from_tier


def _mode_label(queue: str) -> str:
    if not queue:
        return "Custom"
    return GAMEMODES.get(queue.lower(), queue.replace("_", " ").title())


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

_LAST_BOARD: dict[str, Any] = {"board": None, "at": 0.0}
_HOLD_SECS = 90.0

_ACCT_CACHE: dict[str, str | None] = {}

_CONTENT_CACHE: dict[str, Any] = {"seasons": None, "at": 0.0}

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

# Two pools, deliberately not one. The per-player fan-outs call kd_hs, which
# fans out again over match details; with a single shared pool the outer
# workers would occupy every thread while waiting on inner tasks that can
# never be scheduled -- a nested-pool deadlock. Separate pools make that
# impossible. Module-level so a refresh no longer pays thread creation on
# every tick, and never used as context managers, which would shut them down
# permanently after the first call.
_PLAYER_POOL = ThreadPoolExecutor(max_workers=8, thread_name_prefix="players")
_DETAIL_POOL = ThreadPoolExecutor(max_workers=6, thread_name_prefix="details")

_CACHE_WRITE_LOCK = threading.Lock()


def _cache_put(cache: dict[str, Any], cap: int, key: Any, value: Any) -> None:
    with _CACHE_WRITE_LOCK:
        while len(cache) >= cap:
            cache.pop(next(iter(cache)), None)
        cache[key] = value


_QUEUE_CACHE: dict[str, Any] = {"at": 0.0, "data": None}


_log = console_logger("reveal")


def _is_throttled(resp: Any) -> bool:
    return isinstance(resp, dict) and resp.get("status") == 429


def _fallback_name(puuid: str) -> str:
    return f"Player-{(puuid or '????')[:4].upper()}"


def smurf_signals(
    *,
    level: int | None,
    peak_tier: int | None,
    rank_tier: int | None,
    kd: float | None,
    win_rate: float | None,
    games: int | None,
) -> list[str]:
    reasons: list[str] = []
    lvl = level or 0
    if lvl <= 0:
        return reasons
    if lvl < 60 and (peak_tier or 0) >= 20:
        reasons.append(f"Lvl {lvl}, peak {rank_from_tier(peak_tier)['name']}")
    if kd is not None and kd >= 1.35 and lvl < 80:
        reasons.append(f"K/D {kd} at lvl {lvl}")
    if win_rate is not None and win_rate >= 62 and (games or 0) >= 15 and lvl < 100:
        reasons.append(f"{win_rate}% WR")
    return reasons


def form_streak(form: list[Any]) -> dict[str, Any] | None:
    if not form:
        return None
    t, n = form[0], 1
    for r in form[1:]:
        if r != t:
            break
        n += 1
    return {"type": t, "count": n}


def compute_smurf(
    *,
    level: int | None,
    peak_tier: int | None,
    rank_tier: int | None,
    kd: float | None,
    win_rate: float | None,
    games: int | None,
) -> tuple[bool, list[str]]:
    reasons = smurf_signals(
        level=level, peak_tier=peak_tier, rank_tier=rank_tier, kd=kd, win_rate=win_rate, games=games
    )
    if not reasons:
        return False, []
    flagged = ((level or 0) < 60 and len(reasons) >= 1) or len(reasons) >= 2
    return flagged, reasons


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
        "agentId": agent.get("uuid"),
        "agentPortrait": agent.get("portrait"),
        "agentArt": agent.get("fullPortrait"),
        "agentColor": agent.get("color", "#8B978F"),
        "role": agent.get("role"),
        "selection": selection,
        "rankTier": rank["tier"],
        "rank": rank["name"],
        "rankColor": rank["color"],
        "rankGroup": rank["group"],
        "rankIcon": valapi.rank_icon(rank["tier"]),
        "rr": rr,
        "rrEarned": rr_earned,
        "leaderboard": leaderboard or 0,
        "peakRankTier": peak["tier"],
        "peakRank": peak["name"],
        "peakColor": peak["color"],
        "peakIcon": valapi.rank_icon(peak["tier"]),
        "peakAct": peak_act,
        "previousRank": prev["name"],
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
    }


# A trade window of 3 seconds is the figure the public trackers settle on. It
# is a convention, not something Riot publishes, so it lives here as a named
# constant rather than a magic number three call sites deep.
_TRADE_WINDOW_MS = 3000


def _round_stats(md: dict[str, Any], rounds: int) -> dict[str, dict[str, Any]]:
    """Per-player round detail: damage, KAST, opening duels, economy, multikills.

    Every one of these comes out of the roundResults block that match-details
    already returns, so none of it costs another request to Riot. The parsing is
    defensive throughout: rounds get abandoned, players disconnect, and Riot has
    shipped rounds with no playerStats at all.
    """
    out: dict[str, dict[str, Any]] = {}
    # Which side each account played, for the clutch check below.
    teams: dict[str, str] = {}
    for player in md.get("players") or []:
        if isinstance(player, dict) and player.get("subject") and player.get("teamId"):
            teams[str(player["subject"])] = str(player["teamId"])

    def slot(puuid: str) -> dict[str, Any]:
        if puuid not in out:
            out[puuid] = {
                "damage": 0,
                "spent": 0,
                "kast": 0,
                "firstBloods": 0,
                "firstDeaths": 0,
                "headshots": 0,
                "bodyshots": 0,
                "legshots": 0,
                "plants": 0,
                "defuses": 0,
                "clutches": 0,
                "clutchesLost": 0,
                "multiKills": {2: 0, 3: 0, 4: 0, 5: 0},
                "weapons": {},
            }
        return out[puuid]

    for rr in md.get("roundResults") or []:
        stats = rr.get("playerStats") or []
        if not stats:
            continue

        # Everything the round needs, gathered in one pass.
        kills: list[dict[str, Any]] = []
        clutched: set[str] = set()
        alive: set[str] = set()
        assisted: set[str] = set()
        killed_by: dict[str, str] = {}
        killed_at: dict[str, int] = {}
        per_player_kills: dict[str, int] = {}

        for ps in stats:
            sub = ps.get("subject")
            if not sub:
                continue
            alive.add(sub)
            entry = slot(sub)

            for dmg in ps.get("damage") or []:
                entry["damage"] += int(dmg.get("damage") or 0)
                # Riot counts where every bullet landed and nothing read it.
                entry["headshots"] += int(dmg.get("headshots") or 0)
                entry["bodyshots"] += int(dmg.get("bodyshots") or 0)
                entry["legshots"] += int(dmg.get("legshots") or 0)

            econ = ps.get("economy") or {}
            entry["spent"] += int(econ.get("spent") or 0)

            for kill in ps.get("kills") or []:
                when = int(kill.get("timeSinceRoundStartMillis") or 0)
                victim = kill.get("victim")
                kills.append({"killer": sub, "victim": victim, "at": when})
                per_player_kills[sub] = per_player_kills.get(sub, 0) + 1
                if victim:
                    killed_by[victim] = sub
                    killed_at[victim] = when
                for helper in kill.get("assistants") or []:
                    if helper:
                        assisted.add(helper)
                item = (kill.get("finishingDamage") or {}).get("damageItem") or ""
                if item:
                    entry["weapons"][item] = entry["weapons"].get(item, 0) + 1

        # Who touched the spike. Both are named on the round itself.
        planter = rr.get("bombPlanter")
        if planter:
            slot(planter)["plants"] += 1
        defuser = rr.get("bombDefuser")
        if defuser:
            slot(defuser)["defuses"] += 1

        # Clutches. The last player standing on their side, with at least one
        # opponent still alive, and then the round goes their way. This needs
        # the sides, which the round does not carry, so they come from the match.
        winner = rr.get("winningTeam")
        if teams:
            order = sorted(kills, key=lambda k: k["at"])
            standing: dict[str, set[str]] = {side: set() for side in set(teams.values())}
            for puuid, side in teams.items():
                standing[side].add(puuid)
            for kill in order:
                victim = str(kill.get("victim") or "")
                fell = teams.get(victim)
                if fell:
                    standing[fell].discard(victim)
                for side_name, members in standing.items():
                    if len(members) != 1:
                        continue
                    alone = next(iter(members))
                    others = sum(len(m) for name, m in standing.items() if name != side_name and m)
                    if others >= 1 and alone not in clutched:
                        clutched.add(alone)
                        key = "clutches" if winner == side_name else "clutchesLost"
                        slot(alone)[key] += 1

        # The opening duel of the round.
        if kills:
            first = min(kills, key=lambda k: k["at"])
            if first.get("killer"):
                slot(first["killer"])["firstBloods"] += 1
            if first.get("victim"):
                slot(first["victim"])["firstDeaths"] += 1

        for sub, count in per_player_kills.items():
            if count >= 2:
                slot(sub)["multiKills"][min(count, 5)] += 1

        died = set(killed_by)
        for sub in alive:
            traded = False
            killer = killed_by.get(sub)
            if killer is not None:
                # You were traded if whoever killed you died soon afterwards.
                avenged = killed_at.get(killer)
                if avenged is not None and 0 <= avenged - killed_at.get(sub, 0) <= _TRADE_WINDOW_MS:
                    traded = True
            if per_player_kills.get(sub) or sub in assisted or sub not in died or traded:
                slot(sub)["kast"] += 1

    span = max(1, rounds)
    for entry in out.values():
        entry["adr"] = round(entry["damage"] / span)
        entry["kastPct"] = round(entry["kast"] / span * 100)
        # Riot's own econ rating is damage per 1000 credits spent.
        entry["econ"] = round(entry["damage"] / entry["spent"] * 1000) if entry["spent"] else None
        shots = entry["headshots"] + entry["bodyshots"] + entry["legshots"]
        entry["hsPct"] = round(100 * entry["headshots"] / shots) if shots else None
        entry["shots"] = shots
        ranked_weapons = sorted(entry["weapons"].items(), key=lambda kv: (-kv[1], str(kv[0])))
        # The whole loadout, not just the favourite. It was being thrown away.
        entry["weaponKills"] = [
            {"name": valapi.weapon_name(item) or "Ability", "kills": count}
            for item, count in ranked_weapons[:6]
        ]
        entry["topWeapon"] = entry["weaponKills"][0] if entry["weaponKills"] else None
        entry["multiKills"] = {str(k): v for k, v in entry["multiKills"].items() if v}
        entry.pop("weapons", None)
        entry.pop("kast", None)
    return out


class LiveMatch:
    def __init__(self, auth: Any) -> None:
        self.auth = auth
        self.auth.headers()
        self.self_puuid = self.auth.puuid
        self._content = None

    def _presences(self) -> list[Any]:
        return riot_client.chat_presences(self.auth)

    @staticmethod
    def _decode_private(private: Any) -> dict[str, Any]:
        if not private or "{" in str(private):
            return {"isValid": False}
        try:
            decoded = json.loads(base64.b64decode(str(private)).decode("utf-8"))
            return decoded if isinstance(decoded, dict) else {"isValid": False}
        except Exception:
            return {"isValid": False}

    def game_state(self, presences: list[Any]) -> str:
        for p in presences:
            if p.get("puuid") != self.self_puuid:
                continue
            if p.get("product") == "league_of_legends":
                continue
            priv = self._decode_private(p.get("private"))
            if "matchPresenceData" in priv:
                return priv["matchPresenceData"].get("sessionLoopState", "MENUS")
            return priv.get("sessionLoopState", "MENUS")
        return "MENUS"

    def party_map(self, puuids: Any, presences: list[Any]) -> dict[str, Any]:
        parties: dict[str, list[Any]] = {}
        for p in presences:
            if p.get("puuid") not in puuids:
                continue
            priv = self._decode_private(p.get("private"))
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
        return {pid: m for pid, m in parties.items() if len(m) > 1}

    def party_members(self, presences: list[Any]) -> list[Any]:
        def _fields(priv: dict[str, Any]) -> tuple[str, int]:
            data = priv.get("partyPresenceData", priv)
            pid = data.get("partyId", "")
            player = priv.get("playerPresenceData", priv)
            return pid, player.get("accountLevel", 0)

        my_party = None
        for p in presences:
            if p.get("puuid") == self.self_puuid:
                priv = self._decode_private(p.get("private"))
                if priv.get("isValid"):
                    my_party = _fields(priv)[0]
                break
        if not my_party:
            return [{"puuid": self.self_puuid, "level": 0, "incognito": False}]

        members = []
        for p in presences:
            priv = self._decode_private(p.get("private"))
            if not priv.get("isValid"):
                continue
            pid, level = _fields(priv)
            if pid == my_party:
                members.append({"puuid": p["puuid"], "level": level, "incognito": False})
        return members or [{"puuid": self.self_puuid, "level": 0, "incognito": False}]

    def reveal_names(self, puuids: list[str]) -> dict[str, Any]:
        names: dict[str, str] = {}
        if not puuids:
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

        try:
            res = self.auth.pd_put("/name-service/v2/players", puuids)
            if isinstance(res, dict) and res.get("errorCode"):
                res = self.auth.pd_put("/name-service/v2/players", puuids, refresh=True)
            _ingest(res)
        except Exception:
            pass

        missing = [p for p in puuids if p not in names]
        if missing and len(missing) <= 3:
            for puuid in missing:
                try:
                    _ingest(self.auth.pd_put("/name-service/v2/players", [puuid]))
                except Exception:
                    pass
        return names

    def reveal_via_account_api(self, puuid: str) -> str | None:
        if puuid in _ACCT_CACHE:
            return _ACCT_CACHE[puuid]
        key = os.getenv("RIOT_API_KEY", "").strip()
        if not key:
            return None
        cluster = ROUTING.get(os.getenv("RIOT_REGION", "na").strip().lower(), "americas")
        name = None
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
        except Exception as e:
            _log(f"account-v1 lookup error: {e}")
        _ACCT_CACHE[puuid] = name
        return name

    def resolve_identity(
        self, puuid: str, name_service: dict[str, Any], ident: dict[str, Any]
    ) -> tuple[str, int, bool]:
        name = name_service.get(puuid) or self.reveal_via_account_api(puuid)
        level = ident.get("AccountLevel", 0) or 0
        level_hidden = ident.get("HideAccountLevel", False)
        return name or _fallback_name(puuid), level, level_hidden

    def match_score(self, presences: list[Any]) -> dict[str, Any] | None:
        for p in presences:
            if p.get("puuid") != self.self_puuid:
                continue
            priv = self._decode_private(p.get("private"))
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
        except Exception:
            pass
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
            players = []
            for p in ally.get("Players", []):
                p = dict(p)
                p["TeamID"] = ally.get("TeamID", "Blue")
                players.append(p)
            return players, mid, match.get("MapID", ""), match.get("QueueID", "")
        return None

    def _seasons(self) -> list[dict[str, Any]]:

        now = time.time()
        if _CONTENT_CACHE["seasons"] is not None and now - _CONTENT_CACHE["at"] < 3600:
            return _CONTENT_CACHE["seasons"]
        try:
            data = requests.get(
                f"https://shared.{self.auth.shard}.a.pvp.net/content-service/v3/content",
                headers=self.auth.headers(),
                timeout=8,
            ).json()
            seasons = data.get("Seasons", []) if isinstance(data, dict) else []
            if seasons:
                _CONTENT_CACHE["seasons"] = seasons
                _CONTENT_CACHE["at"] = now
            return seasons or (_CONTENT_CACHE["seasons"] or [])
        except Exception:
            return _CONTENT_CACHE["seasons"] or []

    def season_id(self) -> str | None:
        for s in self._seasons():
            if s.get("IsActive") and s.get("Type") == "act":
                return s["ID"]
        return None

    def prev_season_id(self) -> str | None:
        seasons = self._seasons()
        current = next((s for s in seasons if s.get("IsActive") and s.get("Type") == "act"), None)
        if not current:
            return None
        for s in seasons:
            if s.get("Type") == "act" and s.get("EndTime") == current.get("StartTime"):
                return s["ID"]
        return None

    def _fresh_mids(self, puuid: str) -> tuple[list[str], bool, bool]:
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
            entries = (hist or {}).get("History", []) if isinstance(hist, dict) else []
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
                mids, is_comp, _ = self._fresh_mids(puuid)
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
        except Exception:
            pass
        return out

    def act_episode(self, season_id: str | None) -> str | None:
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
        num = valapi._act_number(str(act.get("Name") or ""))
        ep_label = valapi._episode_label((ep or {}).get("Name"))
        if ep_label and num is not None:
            return f"{ep_label} Act {num}"
        if num is not None:
            return f"Act {num}"
        return (act.get("Name") or "").title() or None

    def level_from_history(self, puuid: str) -> int:
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
            # This used to return a bare 0, which is indistinguishable from an
            # account that really is level 0. Still degrade, but leave a trace.
            _log(f"level lookup failed for {puuid[:8]}: {type(e).__name__}: {e}")
        if level > 0:
            # Every sibling cache goes through _cache_put for the cap and the
            # write lock; this one grew without bound and wrote unsynchronised
            # from the fetch_player/fetch_member pool workers.
            _cache_put(_LEVEL_CACHE, _KD_CACHE_MAX, puuid, level)
        return level

    def kd_hs(
        self, puuid: str, count: int = 3
    ) -> tuple[float | None, float | None, Any, str, dict[str, Any] | None]:
        try:
            rr_earned = None

            mids_all, _, throttled = self._fresh_mids(puuid)
            mids = mids_all[:count]
            if not mids:
                return None, None, rr_earned, ("throttled" if throttled else "empty"), None
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
                return None

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
                return None, None, rr_earned, ("throttled" if throttled else "empty"), None
            kd = round(kills / deaths, 2) if deaths else float(kills)
            hs = round(heads / hits * 100) if hits else None
            intel = {
                "topAgents": [
                    {"agent": a, "games": n}
                    for a, n in sorted(agent_counts.items(), key=lambda x: -x[1])[:3]
                ],
                "form": form,
                "streak": form_streak(form),
                "mapWins": map_wins,
            }
            result = (kd, hs, rr_earned, "ok", intel)
            if not throttled and used == len(mids):
                _cache_put(_KD_CACHE, _KD_CACHE_MAX, puuid, (result, tuple(mids), count))
            return result
        except Exception:
            return None, None, None, "error", None

    def _spawn_kd_fill(
        self,
        match_id: str,
        puuids: list[str],
        season: str | None,
        prev_season: str | None,
    ) -> None:
        with _KD_FILL_LOCK:
            if match_id in _KD_FILLING:
                return
            _KD_FILLING.add(match_id)

        def _run() -> None:
            try:

                def _fill_one(puuid: str) -> None:
                    cache_key = f"{match_id}:{puuid}"
                    entry = _CACHE.get(cache_key)
                    if entry is None or entry.get("kd_done"):
                        return
                    entry["kd_tries"] = entry.get("kd_tries", 0) + 1
                    kd, hs, _, status, intel = self.kd_hs(puuid, count=3)
                    if kd is None:
                        _log(f"kd-fill {puuid[:8]} status={status} tries={entry['kd_tries']}")
                    if kd is not None:
                        entry["kd"], entry["hs"] = kd, hs
                        entry["intel"] = intel
                        entry["kd_done"] = True
                    elif status == "empty":
                        entry["kd_done"] = True
                    elif status == "throttled":
                        pass
                    elif entry["kd_tries"] >= 6:
                        entry["kd_done"] = True

                def _top_up(puuid: str) -> None:
                    entry = _CACHE.get(f"{match_id}:{puuid}")
                    if entry is None or entry.get("kd_full") or entry.get("kd") is None:
                        return
                    entry["kd_full"] = True
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
                    kd, hs, _, _, intel = self.kd_hs(puuid, count=5)
                    if kd is not None:
                        entry["kd"], entry["hs"] = kd, hs
                        entry["intel"] = intel

                list(_PLAYER_POOL.map(_fill_one, puuids))
                list(_PLAYER_POOL.map(_top_up, puuids))
            finally:
                with _KD_FILL_LOCK:
                    _KD_FILLING.discard(match_id)

        threading.Thread(target=_run, daemon=True, name=f"kd-fill-{match_id[:8]}").start()

    def build_scoreboard(self, include_stats: bool = True) -> dict[str, Any]:
        presences = self._presences()
        state = self.game_state(presences)

        if state == "MENUS":
            _LAST_BOARD["board"] = None

            board = dict(self.build_lobby(presences, include_stats=include_stats))
            board["queue"] = self.queue_status()
            return board
        if state not in ("INGAME", "PREGAME"):
            held = self._held_board()
            return held or {
                "state": state,
                "stateLabel": STATES.get(state, state),
                "source": "local",
                "players": [],
                "teams": {},
                "parties": [],
            }

        current = self._current_players(state)
        if not current:
            held = self._held_board()
            return held or {
                "state": "MENUS",
                "stateLabel": STATES["MENUS"],
                "source": "local",
                "players": [],
                "teams": {},
                "parties": [],
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
        for idx, (pid, members) in enumerate(pmap.items()):
            color = party_color(idx)
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
                _CACHE[cache_key] = cached
                if include_stats:
                    uncached_kd.append(puuid)
            else:
                if not cached["rk"].get("ok") and time.time() - cached.get("rank_at", 0.0) > 20.0:
                    rk = self.rank_info(puuid, season, prev_season)
                    if rk.get("ok"):
                        cached["rk"], cached["prev"] = rk, rk.get("prev", 0)
                    else:
                        cached["rank_at"] = time.time()
                if include_stats and not cached.get("kd_done"):
                    uncached_kd.append(puuid)
            name, level, level_hidden = self.resolve_identity(puuid, names, ident)

            if (level or 0) <= 0:
                recovered = self.level_from_history(puuid)
                if recovered > 0:
                    level = recovered
            return puuid, cached, name, level, level_hidden

        resolved = {r[0]: r[1:] for r in _PLAYER_POOL.map(fetch_player, raw_players) if r}

        if uncached_kd:
            self._spawn_kd_fill(match_id, uncached_kd, season, prev_season)

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
            parties=parties_out,
            map_splash=valapi.map_splash(map_name),
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
        now = time.time()
        if _QUEUE_CACHE["data"] is not None and now - _QUEUE_CACHE["at"] < 3.0:
            return _QUEUE_CACHE["data"]
        from riot_client import party_snapshot

        try:
            snap = party_snapshot(self.auth)
        except Exception:
            snap = {"available": False}
        if snap.get("throttled") and _QUEUE_CACHE["data"]:
            return _QUEUE_CACHE["data"]
        snap.pop("throttled", None)
        _QUEUE_CACHE.update(at=now, data=snap)
        return snap

    def diagnose_reveal(self, max_players: int = 2, max_matches: int = 8) -> dict[str, Any]:
        presences = self._presences()
        state = self.game_state(presences)
        current = self._current_players(state)
        if not current:
            return {"state": state, "error": "Not in a pre-game/in-game match."}
        raw_players, _, _, _ = current
        puuids = [p["Subject"] for p in raw_players]
        names = self.reveal_names(puuids)

        targets = [
            p["Subject"]
            for p in raw_players
            if (p.get("PlayerIdentity", {}) or {}).get("Incognito")
            and p["Subject"] != self.self_puuid
        ]

        report = []
        for puuid in targets[:max_players]:
            entry = {"puuid": puuid[:8], "nameService": names.get(puuid), "matches": []}
            try:
                hist = self.auth.pd_get(
                    f"/match-history/v1/history/{puuid}?startIndex=0&endIndex={max_matches}"
                )
                for m in (hist.get("History") or [])[:max_matches]:
                    mid = m.get("MatchID")
                    if not mid:
                        continue
                    md = self.auth.pd_get(f"/match-details/v1/matches/{mid}")
                    pl = next(
                        (x for x in (md.get("players") or []) if x.get("subject") == puuid), None
                    )
                    gn = (pl or {}).get("gameName") or ""
                    entry["matches"].append(
                        {
                            "queue": m.get("QueueID") or "?",
                            "namePresent": bool(gn.strip()),
                            "name": (
                                f"{gn}#{(pl or {}).get('tagLine', '')}" if gn.strip() else None
                            ),
                            "level": (pl or {}).get("accountLevel"),
                        }
                    )
            except Exception as e:
                entry["error"] = str(e)
            entry["nameEverPresent"] = any(x["namePresent"] for x in entry["matches"])
            report.append(entry)

        if not targets:
            verdict = "no Incognito players in this match to test"
        elif any(e["nameEverPresent"] for e in report):
            verdict = "baked per-match, deeper history search CAN reveal names"
        else:
            verdict = (
                "dynamic on current status, match history canNOT reveal names; "
                "account-v1 (RIOT_API_KEY) is the only path"
            )
        return {
            "state": state,
            "incognitoCount": len(targets),
            "verdict": verdict,
            "report": report,
        }

    def build_lobby(self, presences: list[Any], include_stats: bool = False) -> dict[str, Any]:
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

        season = self.season_id()
        prev_season = self.prev_season_id()
        multi = len(members) > 1
        party = (
            {"id": "lobby", "color": party_color(0), "number": 1, "size": len(members)}
            if multi
            else None
        )

        def fetch_member(m: dict[str, Any]) -> tuple[Any, ...] | None:
            puuid = m["puuid"]
            rk = self.rank_info(puuid, season, prev_season)
            kd = hs = intel = None
            if include_stats:
                kd, hs, _, _, intel = self.kd_hs(puuid, count=5)

            level = m.get("level", 0) or 0
            if level <= 0:
                level = self.level_from_history(puuid)
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

        parties_out = [{**party, "members": puuids}] if party else []
        board = finalize(
            players,
            state="MENUS",
            source="local",
            self_team="Blue",
            map_name=None,
            queue="Lobby",
            match_id="lobby",
            parties=parties_out,
        )
        board["riotRequests"] = self.auth.req_count
        _LOBBY_CACHE.update(key=key, at=now, board=board)
        return board

    def player_career(self, puuid: str, count: int = 8) -> dict[str, Any]:
        try:
            hist = self.auth.pd_get(
                f"/match-history/v1/history/{puuid}?startIndex=0&endIndex={count}"
            )
            entries = hist.get("History", []) or [] if isinstance(hist, dict) else []
        except Exception:
            entries = []
        mids = [h["MatchID"] for h in entries if h.get("MatchID")]

        def fetch_detail(mid: str) -> Any:
            try:
                return self._career_match(
                    self.auth.pd_get(f"/match-details/v1/matches/{mid}"), puuid, mid
                )
            except Exception:
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

        updates = {}
        if any((row.get("mode") or "").lower() == "competitive" for row in matches):
            try:
                cu = self.auth.pd_get(
                    f"/mmr/v1/players/{puuid}/competitiveupdates"
                    f"?startIndex=0&endIndex={min(20, max(10, count))}&queue=competitive"
                )
                for update in (cu or {}).get("Matches", []) or []:
                    if update.get("MatchID"):
                        updates[update["MatchID"]] = update
            except Exception:
                updates = {}
        for row in matches:
            update = updates.get(row.get("matchId"))
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
                    "rankColor": rank.get("color"),
                    "rankIcon": valapi.rank_icon(tier or 0) if tier else None,
                }
            )

        return {"source": "local", "puuid": puuid, "matches": matches, **_career_summary(matches)}

    def _career_match(self, md: dict[str, Any], puuid: str, mid: str = "") -> dict[str, Any] | None:
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
        for rr in md.get("roundResults", []):
            for ps in rr.get("playerStats", []):
                player_id = ps.get("subject")
                if not player_id:
                    continue
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
            teammate_hits = hits_by_player.get(player_id, 0)
            teammates.append(
                {
                    "puuid": player_id,
                    "agent": teammate_agent.get("name", "Unknown"),
                    "agentPortrait": teammate_agent.get("portrait"),
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
                    "shotsHit": teammate_hits,
                    "headshots": heads_by_player.get(player_id, 0),
                }
            )
        party_id = subj.get("partyId")
        party_size = sum(p.get("partyId") == party_id for p in players) if party_id else 1
        queue = info.get("queueID") or info.get("queueId") or ""
        map_name = map_name_from_path(info.get("mapId", ""))
        opponent_score = next(
            (team.get("roundsWon", 0) for tid, team in teams.items() if tid != team_id), None
        )
        return {
            "matchId": mid or info.get("matchId", ""),
            "map": map_name,
            "mapSplash": valapi.map_splash(map_name),
            "mode": _mode_label(queue),
            "startMillis": info.get("gameStartMillis", 0),
            "result": "Victory" if won is True else "Defeat" if won is False else "Draw",
            "team": team_id,
            "score": mine.get("roundsWon", 0),
            "opponentScore": opponent_score,
            "agent": agent.get("name", "Unknown"),
            "agentPortrait": agent.get("portrait"),
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

        detail = _round_stats(md, rounds)

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
                    "agentPortrait": agent.get("portrait"),
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
                    # The rest of what the rounds already told us. Every one of
                    # these was computed and then left behind here.
                    "weaponKills": (detail.get(sub) or {}).get("weaponKills") or [],
                    "clutches": (detail.get(sub) or {}).get("clutches"),
                    "clutchesLost": (detail.get(sub) or {}).get("clutchesLost"),
                    "plants": (detail.get(sub) or {}).get("plants"),
                    "defuses": (detail.get(sub) or {}).get("defuses"),
                    "shots": (detail.get(sub) or {}).get("shots"),
                    "rankTier": rank_meta["tier"],
                    "rank": rank_meta["name"],
                    "rankColor": rank_meta["color"],
                    "rankIcon": valapi.rank_icon(rank_meta["tier"]),
                    "rr": rank.get("rr") or 0,
                    "leaderboard": rank.get("lb") or 0,
                    "peakRankTier": peak_meta["tier"],
                    "peakRank": peak_meta["name"],
                    "peakColor": peak_meta["color"],
                    "peakIcon": valapi.rank_icon(peak_meta["tier"]),
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
                "avgRankColor": avg_rank["color"],
                "rankIcon": valapi.rank_icon(avg_tier) if avg_tier else None,
            }
        map_name = map_name_from_path(info.get("mapId", ""))
        return {
            "matchId": match_id,
            "map": map_name,
            "mapSplash": valapi.map_splash(map_name),
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


def _career_summary(matches: list[Any]) -> dict[str, Any]:
    n = len(matches)
    if not n:
        return {
            "averages": {
                "games": 0,
                "wins": 0,
                "winRate": 0,
                "kd": 0,
                "kills": 0,
                "deaths": 0,
                "assists": 0,
                "hsPct": 0,
            },
            "coPlayers": [],
            "agentPool": [],
            "mapStats": [],
        }
    wins = sum(1 for m in matches if m["result"] == "Victory")
    k = sum(m["kills"] for m in matches)
    d = sum(m["deaths"] for m in matches)
    a = sum(m["assists"] for m in matches)
    hs = [m["hsPct"] for m in matches if m.get("hsPct") is not None]

    seen: dict[str, dict[str, Any]] = {}
    for m in matches:
        for mate in m["teammates"]:
            pid = mate.get("puuid")
            if not pid:
                continue
            e = seen.setdefault(
                pid, {"puuid": pid, "name": mate.get("name"), "sharedMatches": 0, "agents": set()}
            )
            e["sharedMatches"] += 1
            e["name"] = mate.get("name") or e["name"]
            if mate.get("agent"):
                e["agents"].add(mate["agent"])
    co_players = sorted(
        (
            {
                "puuid": e["puuid"],
                "name": e["name"],
                "sharedMatches": e["sharedMatches"],
                "agents": sorted(e["agents"]),
                "isParty": e["sharedMatches"] >= 2,
            }
            for e in seen.values()
        ),
        key=lambda x: x["sharedMatches"],
        reverse=True,
    )[:6]

    def _tally(key: str) -> dict[str, Any]:
        out: dict[str, list[Any]] = {}
        for m in matches:
            name = m.get(key)
            if not name or name == "Unknown":
                continue
            t = out.setdefault(name, [0, 0])
            t[1] += 1
            if m["result"] == "Victory":
                t[0] += 1
        return out

    agent_pool = [
        {
            "agent": ag,
            "games": g,
            "winRate": round(100 * w / g),
            "portrait": (resolve_agent(ag) or {}).get("portrait"),
            "color": (resolve_agent(ag) or {}).get("color", "#8B978F"),
        }
        for ag, (w, g) in sorted(_tally("agent").items(), key=lambda x: -x[1][1])
    ][:5]
    map_stats = [
        {"map": mp, "games": g, "wins": w, "winRate": round(100 * w / g)}
        for mp, (w, g) in sorted(_tally("map").items(), key=lambda x: -x[1][1])
    ]

    return {
        "averages": {
            "games": n,
            "wins": wins,
            "winRate": round(100 * wins / n),
            "kills": round(k / n, 1),
            "deaths": round(d / n, 1),
            "assists": round(a / n, 1),
            "kd": round(k / d, 2) if d else float(k),
            "hsPct": round(sum(hs) / len(hs)) if hs else None,
        },
        "coPlayers": co_players,
        "agentPool": agent_pool,
        "mapStats": map_stats,
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
        "rankColor": rank_meta["color"],
        "rankIcon": valapi.rank_icon(round(avg_tier)) if ranked else None,
        "avgKd": round(sum(kds) / len(kds), 2) if kds else None,
        "avgWinRate": round(sum(wrs) / len(wrs)) if wrs else None,
        "smurfCount": sum(1 for p in team_players if p.get("smurf")),
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
# deathmatch is worse than saying nothing, and the old code said it.
MODES_WITHOUT_SIDES = {"deathmatch", "ggteam", "hurm"}

# Rounds before the sides swap, for the modes whose format this has been
# checked against. A mode that is missing here gets no side once the match is
# under way: the starting side is still true in agent select, but which side
# you are on in round nine is a guess without knowing where half time falls.
HALF_LENGTH = {"competitive": 12, "unrated": 12, "custom": 12}


def side_now(state: str, self_team: Any, queue: Any, round_number: Any) -> str | None:
    """Which side you are on right now, or None when that cannot be known.

    Agent select is the easy case and the one that matters most, because the
    side is what you pick an agent for: no round has been played, so the
    starting side is the side. Once the match is running the answer depends on
    how many rounds have gone by, and after regulation it depends on overtime
    rules that differ again, which is where this stops claiming to know.
    """
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
    parties: Any,
    map_splash: Any = None,
    score: Any = None,
) -> dict[str, Any]:

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
        "mapSplash": map_splash,
        "mode": _mode_label(queue),
        "matchId": match_id,
        "selfTeam": self_team,
        "side": side,
        "players": players,
        "teams": teams,
        "teamStats": team_stats,
        "winProb": win_prob,
        "parties": parties,
        "score": score,
        "lockProgress": {"locked": locked, "total": len(players)} if state == "PREGAME" else None,
    }


def _self_check() -> None:
    # Two rounds, worked out by hand, covering every branch of _round_stats:
    # an opening duel, an assist, a survivor, a trade inside the window and a
    # multikill.
    #
    #   round 1: A kills B at 5.0s, C assists.
    #     A  kill            -> KAST, first blood
    #     B  died, untraded  -> no KAST, first death
    #     C  assist          -> KAST
    #     D  survived        -> KAST
    #   round 2: B kills A at 2.0s, C kills B at 3.0s, C kills D at 4.0s.
    #     A  killer B died 1.0s later, inside the 3s window -> traded, KAST
    #     B  kill            -> KAST, first blood
    #     C  two kills       -> KAST, one 2k
    #     D  died, killer C never died -> no KAST
    match = {
        "roundResults": [
            {
                "playerStats": [
                    {
                        "subject": "A",
                        "damage": [{"damage": 150, "headshots": 3, "bodyshots": 2}],
                        "economy": {"spent": 3900},
                        "kills": [
                            {
                                "victim": "B",
                                "timeSinceRoundStartMillis": 5000,
                                "assistants": ["C"],
                                "finishingDamage": {"damageItem": "vandal-id"},
                            },
                        ],
                    },
                    {"subject": "B", "damage": [{"damage": 40}], "economy": {"spent": 2900}},
                    {"subject": "C", "damage": [{"damage": 10}], "economy": {"spent": 800}},
                    {"subject": "D", "damage": [], "economy": {"spent": 0}},
                ],
                "winningTeam": "Blue",
                "bombPlanter": "A",
            },
            {
                "playerStats": [
                    {
                        "subject": "A",
                        "damage": [{"damage": 100, "headshots": 1, "bodyshots": 4}],
                        "economy": {"spent": 2900},
                    },
                    {
                        "subject": "B",
                        "damage": [{"damage": 120}],
                        "economy": {"spent": 3900},
                        "kills": [
                            {
                                "victim": "A",
                                "timeSinceRoundStartMillis": 2000,
                                "finishingDamage": {"damageItem": "vandal-id"},
                            },
                        ],
                    },
                    {
                        "subject": "C",
                        "damage": [{"damage": 260}],
                        "economy": {"spent": 3900},
                        "kills": [
                            {
                                "victim": "B",
                                "timeSinceRoundStartMillis": 3000,
                                "finishingDamage": {"damageItem": "vandal-id"},
                            },
                            {
                                "victim": "D",
                                "timeSinceRoundStartMillis": 4000,
                                "finishingDamage": {"damageItem": "vandal-id"},
                            },
                        ],
                    },
                    {"subject": "D", "damage": [{"damage": 30}], "economy": {"spent": 2400}},
                ],
                "winningTeam": "Blue",
                "bombDefuser": "D",
            },
        ],
        # A and C hold one side, B and D the other. The rounds cannot say who
        # clutched without this, because a round does not carry the sides.
        "players": [
            {"subject": "A", "teamId": "Blue"},
            {"subject": "C", "teamId": "Blue"},
            {"subject": "B", "teamId": "Red"},
            {"subject": "D", "teamId": "Red"},
        ],
    }

    stats = _round_stats(match, 2)

    assert stats["A"]["firstBloods"] == 1, stats["A"]
    assert stats["A"]["firstDeaths"] == 1, stats["A"]
    assert stats["A"]["kastPct"] == 100, f"A should be traded in round 2: {stats['A']}"
    assert stats["A"]["adr"] == 125, stats["A"]
    assert stats["A"]["econ"] == round(250 / 6800 * 1000), stats["A"]

    # Where the bullets landed. Riot counts every one and nothing read them:
    # A hit 4 heads out of 10 shots across the two rounds.
    assert stats["A"]["hsPct"] == 40, stats["A"]
    assert stats["A"]["shots"] == 10, stats["A"]
    assert stats["D"]["hsPct"] is None, stats["D"]

    # The spike. Both ends are named on the round itself.
    assert stats["A"]["plants"] == 1, stats["A"]
    assert stats["D"]["defuses"] == 1, stats["D"]

    # Clutches. In round 2 C is left alone against B and D and wins it. D is
    # last alive in both rounds and loses both, which is not a clutch and is
    # worth counting separately rather than not at all.
    assert stats["C"]["clutches"] == 1, stats["C"]
    assert stats["C"]["clutchesLost"] == 0, stats["C"]
    assert stats["D"]["clutches"] == 0, stats["D"]
    assert stats["D"]["clutchesLost"] == 2, stats["D"]

    # The whole loadout, not just the favourite.
    assert stats["C"]["weaponKills"][0]["kills"] == 2, stats["C"]
    assert stats["C"]["topWeapon"]["kills"] == 2, stats["C"]

    assert stats["B"]["firstBloods"] == 1, stats["B"]
    assert stats["B"]["firstDeaths"] == 1, stats["B"]
    assert stats["B"]["kastPct"] == 50, f"B has no round-1 contribution: {stats['B']}"

    assert stats["C"]["kastPct"] == 100, stats["C"]
    assert stats["C"]["multiKills"].get("2") == 1, stats["C"]
    assert stats["C"]["firstBloods"] == 0, stats["C"]

    assert stats["D"]["kastPct"] == 50, f"D survived round 1 only: {stats['D']}"

    # A match Riot returned nothing useful for must not throw.
    assert _round_stats({}, 0) == {}
    assert _round_stats({"roundResults": [{}]}, 1) == {}
    empty = _round_stats({"roundResults": [{"playerStats": [{"subject": "X"}]}]}, 1)
    assert empty["X"]["adr"] == 0, empty
    assert empty["X"]["econ"] is None, empty

    # Sides. Agent select is the case that matters: it is what you pick an
    # agent for, and no round has been played, so the starting side is the
    # side. Red starts on attack, Blue starts on defence.
    assert side_now("PREGAME", "Red", "competitive", None) == "Attack"
    assert side_now("PREGAME", "Blue", "competitive", None) == "Defense"
    # A mode this has never been checked against still knows where it starts.
    assert side_now("PREGAME", "Red", "swiftplay", None) == "Attack"

    # Halves. Twelve rounds a side, so round 12 is still the first half and
    # round 13 is not, and the label has to follow the swap rather than sit on
    # the team colour for the whole match.
    assert side_now("INGAME", "Red", "competitive", 1) == "Attack"
    assert side_now("INGAME", "Red", "competitive", 12) == "Attack"
    assert side_now("INGAME", "Red", "competitive", 13) == "Defense"
    assert side_now("INGAME", "Blue", "competitive", 13) == "Attack"
    assert side_now("INGAME", "Red", "competitive", 24) == "Defense"

    # Where it must not guess: overtime swaps on its own rules, a mode with an
    # unchecked format has no known half time, nobody defends in a deathmatch,
    # and a missing team coloured everyone Defense before.
    assert side_now("INGAME", "Red", "competitive", 25) is None
    assert side_now("INGAME", "Red", "swiftplay", 3) is None
    assert side_now("INGAME", "Red", "competitive", None) is None
    assert side_now("PREGAME", "Blue", "deathmatch", None) is None
    assert side_now("PREGAME", None, "competitive", None) is None
    assert side_now("MENUS", "Red", "competitive", None) is None

    print("live_match self-check OK (round stats: KAST, trades, opening duels, econ; sides)")


if __name__ == "__main__":
    _self_check()

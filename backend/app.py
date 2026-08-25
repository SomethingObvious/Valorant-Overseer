from __future__ import annotations

import json
import os
import secrets
import sys
import threading
import time
from pathlib import Path
from typing import TYPE_CHECKING, Any

from flask import Flask, jsonify, request

if TYPE_CHECKING:
    from flask.typing import ResponseReturnValue
from flask_cors import CORS

try:
    from dotenv import load_dotenv

    load_dotenv()
except Exception:
    pass

import discord_presence
import encounter_log
import history
import inventory
import live_match
import match_meta
import overseerlog
import party_detector
import pick_advisor
import sample_match
import session_tracker
from agents import AGENTS, resolve_agent
from common import write_atomic
from riot_client import REGIONS, ClientNotReadyError, LocalAuth, RiotClient
from vconstants import APP_VERSION, STATES, rank_from_tier

app = Flask(__name__)

# The local API is not public, and until now it behaved as though it were.
#
# `CORS(app)` with no arguments reflects whatever Origin it is given and allows
# every method, and there was no authentication at all. Any page open in any
# browser tab could therefore read /api/live (your Riot ID, your PUUID, your
# rank, and the same for the nine other people in your lobby), /api/encounters
# and /api/inventory -- and could POST /api/dodge with {"dryRun": false} to
# quit your live ranked match. It bound 127.0.0.1, which stops the network but
# does nothing about the browser already running on this machine.
#
# Two changes. Cross-origin is allowed only for a dashboard you configured
# yourself, and nothing at all if you configured none; and every route except
# the readiness probe needs a token that only this process and the files it
# writes know.
_FRONTEND_ORIGIN = os.getenv("FRONTEND_URL", "").strip().rstrip("/")
if _FRONTEND_ORIGIN:
    CORS(app, origins=[_FRONTEND_ORIGIN], supports_credentials=True)

# A fresh secret per launch. It goes in .overseer/bridge.json beside the
# WebSocket token, so the terminal scoreboard and a dashboard you host can
# both read it and nothing else can.
API_TOKEN = secrets.token_urlsafe(32)

# /api/health carries no personal data and is what run.py, diagnose.ps1 and the
# bridge's own readiness probe wait on before the token file exists.
_OPEN_ROUTES = frozenset({"/", "/api/health"})


@app.before_request
def _require_token() -> ResponseReturnValue | None:
    if request.path in _OPEN_ROUTES:
        return None
    sent = request.headers.get("X-Overseer-Token") or request.args.get("s") or ""
    # compare_digest, not ==, so a wrong token cannot be found one character at
    # a time by timing the reply.
    if secrets.compare_digest(sent, API_TOKEN):
        return None
    return jsonify({"error": "unauthorized"}), 401


_COMMAND_ROUTER = None

for _h in overseerlog.get_logger("backend").handlers:
    app.logger.addHandler(_h)

client = RiotClient()

_CACHE: dict[str, tuple[float, dict[str, Any]]] = {}
_CACHE_TTL = float(os.getenv("PLAYER_CACHE_TTL", "60"))
_ENCOUNTER_BACKFILL_AT: dict[str, float] = {}
_ENCOUNTER_BACKFILL_LOCK = threading.Lock()

_SETTINGS_PATH = str(Path(str(Path(__file__).parent)) / "data" / "settings.json")
_SETTINGS_LOCK = threading.Lock()

_SETTINGS_KEYS = {"region", "autoRefresh"}


def _load_settings() -> dict[str, Any]:
    try:
        with Path(_SETTINGS_PATH).open(encoding="utf-8") as fh:
            data = json.load(fh)
        return data if isinstance(data, dict) else {}
    except Exception:
        return {}


def _save_settings(data: dict[str, Any]) -> None:
    Path(str(Path(_SETTINGS_PATH).parent)).mkdir(parents=True, exist_ok=True)
    tmp = f"{_SETTINGS_PATH}.tmp"
    with Path(tmp).open("w", encoding="utf-8") as fh:
        json.dump(data, fh, indent=2)
    Path(tmp).replace(_SETTINGS_PATH)


def _summarize(matches: list[dict[str, Any]]) -> dict[str, Any]:
    n = len(matches)
    if n == 0:
        return {
            "matchesAnalyzed": 0,
            "kills": 0,
            "deaths": 0,
            "assists": 0,
            "kd": 0,
            "kda": 0,
            "hsPct": 0,
            "winRate": 0,
            "wins": 0,
            "losses": 0,
        }
    k = sum(m["stats"]["kills"] for m in matches)
    d = sum(m["stats"]["deaths"] for m in matches)
    a = sum(m["stats"]["assists"] for m in matches)
    hs = [m["stats"].get("hsPct", 0) for m in matches if m["stats"].get("hsPct")]
    wins = sum(1 for m in matches if m["result"] == "Victory")
    losses = sum(1 for m in matches if m["result"] == "Defeat")
    return {
        "matchesAnalyzed": n,
        "kills": round(k / n, 1),
        "deaths": round(d / n, 1),
        "assists": round(a / n, 1),
        "kd": round(k / d, 2) if d else float(k),
        "kda": round((k + a) / d, 2) if d else float(k + a),
        "hsPct": round(sum(hs) / len(hs), 1) if hs else 0,
        "winRate": round(100 * wins / n, 1),
        "wins": wins,
        "losses": losses,
    }


def _decorate_match(m: dict[str, Any], suggestion: dict[str, Any]) -> dict[str, Any]:
    meta = resolve_agent(m.get("agent")) or {}
    st = m["stats"]
    kda = (
        round((st["kills"] + st["assists"]) / st["deaths"], 2)
        if st["deaths"]
        else float(st["kills"] + st["assists"])
    )
    out = dict(m)
    out.pop("teammates", None)
    out["agentMeta"] = {
        "name": meta.get("name", m.get("agent")),
        "role": meta.get("role", "Flex"),
        "color": meta.get("color", "#FF4655"),
        "portrait": meta.get("portrait"),
    }
    out["stats"] = {**st, "kda": kda}

    out["pickSuggestion"] = {
        "agent": suggestion.get("agent"),
        "times": suggestion.get("times", 0),
    }
    return out


def build_player_payload(puuid: str) -> dict[str, Any]:
    raw = client.get_player_overview(puuid)
    matches = raw.get("matches", [])

    party = party_detector.analyze(matches, top_n=5)
    suggestion = pick_advisor.recommend(matches)
    rank = rank_from_tier(raw.get("rankTier"))
    peak = rank_from_tier(raw.get("peakTier"))

    decorated = [_decorate_match(m, suggestion) for m in party["matches"]]

    for dm, pm in zip(decorated, party["matches"], strict=True):
        dm["partyMembers"] = pm.get("partyMembers", [])

    return {
        "puuid": puuid,
        "riotId": raw.get("riotId", "Player"),
        "currentRank": rank["name"],
        "rankTier": rank["tier"],
        "rankGroup": rank["group"],
        "rankColor": rank["color"],
        "rr": raw.get("rr", 0),
        "peakRank": peak["name"],
        "peakColor": peak["color"],
        "source": raw.get("source", "demo"),
        "sourceDetail": raw.get("sourceDetail", ""),
        "averages": _summarize(matches),
        "pickSuggestion": suggestion,
        "coPlayers": party["coPlayers"],
        "partyCount": party["partyCount"],
        "matches": decorated,
    }


@app.get("/api/health")
def health() -> ResponseReturnValue:
    import ws_server as _ws

    return jsonify(
        {
            "ok": True,
            "service": "overseer",
            "appVersion": APP_VERSION,
            "protocol": _ws.PROTOCOL_VERSION,
            "wsReady": _ws.is_ready(),
            "wsPort": _ws.listening_port(),
            "dataSourcePreference": client.source_pref,
            "officialKey": bool(client.api_key),
            "clientStatus": "ok" if LocalAuth.available() else "not_running",
        }
    )


@app.get("/api/agents")
def agents() -> ResponseReturnValue:
    return jsonify({"agents": AGENTS, "count": len(AGENTS)})


@app.get("/api/settings")
def settings_get() -> ResponseReturnValue:
    with _SETTINGS_LOCK:
        return jsonify(_load_settings())


@app.post("/api/settings")
def settings_post() -> ResponseReturnValue:
    body = request.get_json(silent=True) or {}
    incoming = {k: v for k, v in body.items() if k in _SETTINGS_KEYS}
    with _SETTINGS_LOCK:
        merged = _load_settings()
        merged.update(incoming)
        try:
            _save_settings(merged)
        except Exception as e:
            app.logger.exception("settings save failed")
            return jsonify({"ok": False, "message": str(e), "settings": merged}), 200
    return jsonify({"ok": True, "settings": merged})


def _live_enabled() -> bool:
    return client.source_pref != "demo" and LocalAuth.available()


def _attach_encounters(board: dict[str, Any]) -> dict[str, Any]:
    is_live = board.get("source") == "local"
    self_team = board.get("selfTeam")
    for p in board.get("players") or []:
        if not isinstance(p, dict):
            continue
        enc = (
            encounter_log.encounter_for(board.get("selfPuuid"), p.get("puuid")) if is_live else None
        )

        if enc:
            if self_team is not None and p.get("team") == self_team:
                enc["withCount"] = max(0, enc["withCount"] - 1)
            else:
                enc["againstCount"] = max(0, enc["againstCount"] - 1)
        p["encounter"] = enc
    return board


def _attach_stacks(board: dict[str, Any]) -> dict[str, Any]:
    """Mark the players who look like they queued together.

    Riot only reveals a party for accounts whose presence is visible, which
    means friends and yourself. For everyone else this is the only signal
    there is, and it is a guess: it says so, and it carries the numbers it
    was drawn from so the guess can be judged rather than taken on trust.
    """
    if board.get("source") != "local":
        return board
    guesses = party_detector.likely_stacks(board, encounter_log.rosters_for(board.get("selfPuuid")))
    for p in board.get("players") or []:
        if isinstance(p, dict):
            p["stackGuess"] = guesses.get(p.get("puuid") or "")
    return board


def _client_notice() -> dict[str, Any]:
    if not LocalAuth.available():
        return {
            "level": "info",
            "action": "open_game",
            "message": "Open VALORANT to see live ranks, parties and stats.",
        }
    return {
        "level": "warn",
        "action": "restart_game",
        "message": "Couldn't read VALORANT. Please restart your game "
        "(close it completely and relaunch), then try again.",
    }


_LAST_GOOD: dict[str, Any] = {"board": None, "at": 0.0, "notReady": False}
_HOLD_SECS = 12

_BUILD_LOCK = threading.Lock()
_BUILD_FRESH = 3.5


def build_live(seed: int = 7, want_state: str | None = None) -> dict[str, Any]:
    notice = None
    if _live_enabled():
        with _BUILD_LOCK:
            if _LAST_GOOD["board"] and time.time() - _LAST_GOOD["at"] < _BUILD_FRESH:
                return _LAST_GOOD["board"]
            try:
                lm = live_match.LiveMatch(LocalAuth())
                board = lm.build_scoreboard(
                    include_stats=os.getenv("LIVE_INCLUDE_STATS", "true").lower() != "false"
                )
                board.setdefault("sourceDetail", "Local VALORANT client")
                board["selfPuuid"] = lm.self_puuid

                try:
                    session_tracker.observe(board, lm)
                    session_tracker.attach(board)
                except Exception:
                    app.logger.exception("session tracking failed")

                try:
                    encounter_log.record_board(board)
                    _attach_encounters(board)
                    _attach_stacks(board)
                except Exception:
                    app.logger.exception("encounter logging failed")
                board["appVersion"] = APP_VERSION
                _LAST_GOOD["board"], _LAST_GOOD["at"] = board, time.time()
                _LAST_GOOD["notReady"] = False
                return board
            except Exception as e:
                if isinstance(e, ClientNotReadyError):
                    if not _LAST_GOOD["notReady"]:
                        app.logger.info("live scoreboard: %s, waiting for sign-in", e)
                        _LAST_GOOD["notReady"] = True
                else:
                    app.logger.exception("live scoreboard failed")

                if _LAST_GOOD["board"] and time.time() - _LAST_GOOD["at"] < _HOLD_SECS:
                    return _LAST_GOOD["board"]
                notice = _client_notice()
                if client.source_pref == "local":
                    return {
                        "state": "OFFLINE",
                        "stateLabel": "Offline",
                        "source": "local",
                        "error": str(e),
                        "players": [],
                        "teams": {},
                        "parties": [],
                        "notice": notice,
                        "appVersion": APP_VERSION,
                    }
    elif client.source_pref != "demo" and not LocalAuth.available():
        notice = _client_notice()

    if client.source_pref == "demo":
        board = (
            sample_match.generate_lobby(seed)
            if (want_state or "").lower() == "menus"
            else sample_match.generate(seed)
        )
        board = _attach_encounters(board)
        if notice:
            board["notice"] = notice
        board["appVersion"] = APP_VERSION
        return board

    # No client, and nobody asked for the demo. A fabricated lobby here reads
    # as real data -- you cannot tell an invented Diamond 3 from a real one at
    # a glance -- so the UI gets an empty board and says what it is waiting
    # for. DATA_SOURCE=demo is the only way to see sample players now.
    return {
        "state": "OFFLINE",
        "stateLabel": "Waiting for VALORANT",
        "source": "idle",
        "players": [],
        "teams": {},
        "parties": [],
        "notice": notice
        or {
            "level": "info",
            "action": "open_game",
            "message": "Open VALORANT - lobby, Agent Select or a match.",
        },
        "appVersion": APP_VERSION,
    }


@app.get("/api/state")
def state() -> ResponseReturnValue:
    if _live_enabled():
        try:
            lm = live_match.LiveMatch(LocalAuth())
            st = lm.game_state(lm._presences())
            return jsonify({"state": st, "stateLabel": STATES.get(st, st), "source": "local"})
        except Exception as e:
            return jsonify(
                {"state": "OFFLINE", "stateLabel": "Offline", "source": "local", "error": str(e)}
            )
    return jsonify({"state": "INGAME", "stateLabel": "In Game", "source": "demo"})


@app.get("/api/live")
def live() -> ResponseReturnValue:
    try:
        seed = int(request.args.get("seed", 7))
    except (TypeError, ValueError):
        seed = 7
    return jsonify(build_live(seed, request.args.get("state")))


def _refresh_encounter_history(owner: str | None) -> None:
    if not owner or not _live_enabled():
        return
    now = time.time()
    with _ENCOUNTER_BACKFILL_LOCK:
        if now - _ENCOUNTER_BACKFILL_AT.get(owner, 0) < 600:
            return
        _ENCOUNTER_BACKFILL_AT[owner] = now
        try:
            lm = live_match.LiveMatch(LocalAuth())
            career = lm.player_career(owner, count=10)
            encounter_log.backfill_career(owner, career.get("matches") or [])

            season = lm.season_id()
            previous_season = lm.prev_season_id()
            for teammate in (career.get("coPlayers") or [])[:6]:
                if int(teammate.get("sharedMatches") or 0) < 2:
                    continue
                puuid = teammate.get("puuid")
                if not puuid:
                    continue
                rank = lm.rank_info(puuid, season, previous_season)
                tier = int(rank.get("tier") or 0)
                if tier <= 0:
                    continue
                current = rank_from_tier(tier)
                peak = rank_from_tier(rank.get("peak") or tier)
                encounter_log.enrich_player(
                    owner,
                    puuid,
                    {
                        "name": teammate.get("name"),
                        "rank": current["name"],
                        "peakRank": peak["name"],
                        "rankTier": current["tier"],
                        "peakTier": peak["tier"],
                        "rankColor": current["color"],
                        "winRate": rank.get("wr"),
                    },
                )
        except Exception:
            _ENCOUNTER_BACKFILL_AT.pop(owner, None)
            app.logger.exception("encounter history backfill failed")


@app.get("/api/encounters")
def encounters() -> ResponseReturnValue:
    if client.source_pref == "demo":
        return jsonify(
            {
                "players": sample_match.encounters(),
                "accountCount": 1,
                "scope": request.args.get("scope", "current"),
            }
        )
    owner = _current_puuid()
    _refresh_encounter_history(owner)
    scope = "all" if request.args.get("scope") == "all" else "current"
    return jsonify(
        {
            "players": encounter_log.get_all_accounts()
            if scope == "all"
            else encounter_log.get_all(owner),
            "accountCount": encounter_log.account_count(),
            "scope": scope,
        }
    )


@app.get("/api/recap")
def recap() -> ResponseReturnValue:
    live_recap = session_tracker.current_recap() if _live_enabled() else None
    try:
        seed = int(request.args.get("seed", 7))
    except (TypeError, ValueError):
        seed = 7
    return jsonify(live_recap or sample_match.recap(seed))


def _current_puuid() -> str | None:
    if not _live_enabled():
        return None
    try:
        auth = LocalAuth()
        auth.headers()
        return auth.puuid
    except Exception:
        return None


@app.get("/api/sessions")
def sessions_get() -> ResponseReturnValue:
    return jsonify(session_tracker.list_for(_current_puuid()))


@app.post("/api/session/start")
def session_start() -> ResponseReturnValue:
    owner = _current_puuid()
    baseline = history.payload(owner).get("summary", {}) if owner else None
    goal = (request.get_json(silent=True) or {}).get("goal")
    return jsonify(session_tracker.start(owner, goal, baseline))


@app.post("/api/session/end")
def session_end() -> ResponseReturnValue:
    return jsonify(session_tracker.end(_current_puuid()))


@app.delete("/api/sessions/<session_id>")
def session_delete(session_id: str) -> ResponseReturnValue:
    return jsonify(session_tracker.delete(_current_puuid(), session_id.strip()))


@app.post("/api/session/reset")
def session_reset() -> ResponseReturnValue:
    body = request.get_json(silent=True) or {}
    return jsonify(session_tracker.reset(_current_puuid(), body.get("goal")))


def _insights_payload(timezone_name: str | None = None) -> dict[str, Any]:
    puuid = None
    if _live_enabled():
        try:
            auth = LocalAuth()
            auth.headers()
            puuid = auth.puuid
            threading.Thread(
                target=history.refresh,
                args=(auth, timezone_name),
                daemon=True,
                name=f"rr-refresh-{str(puuid)[:8]}",
            ).start()
        except Exception:
            app.logger.exception("rr history refresh failed")
    return history.payload(puuid, timezone_name)


def _performance_payload(timezone_name: str | None = None, rich_limit: int = 20) -> dict[str, Any]:
    payload = _insights_payload(timezone_name)
    owner = (payload.get("account") or {}).get("puuid")
    if owner and _live_enabled():

        def enrich_recent() -> None:
            try:
                history.enrich(live_match.LiveMatch(LocalAuth()), owner, rich_limit)
            except Exception:
                app.logger.exception("performance enrichment failed")

        threading.Thread(
            target=enrich_recent, daemon=True, name=f"perf-enrich-{str(owner)[:8]}"
        ).start()
    if owner:
        session_tracker.ensure_active(owner, payload.get("summary", {}))
    payload["sessions"] = session_tracker.list_for(owner)
    payload["matchMeta"] = match_meta.get_all(owner)
    payload["encounters"] = encounter_log.get_all(owner)
    return payload


@app.get("/api/insights")
def insights() -> ResponseReturnValue:
    return jsonify(_insights_payload(request.args.get("tz")))


@app.get("/api/performance")
def performance() -> ResponseReturnValue:
    try:
        rich_limit = int(request.args.get("richLimit", 20))
    except (TypeError, ValueError):
        rich_limit = 20
    return jsonify(_performance_payload(request.args.get("tz"), rich_limit))


def _inventory_payload() -> dict[str, Any]:
    if not _live_enabled():
        return {
            "available": False,
            "retryable": client.source_pref != "demo",
            "error": "Live client not available.",
        }
    auth = LocalAuth()
    owner = None
    try:
        data = inventory.snapshot(auth)
        owner = getattr(auth, "puuid", None)
        return data
    except ClientNotReadyError:
        owner = getattr(auth, "puuid", None)
        cached = inventory.last_good(owner)
        if cached:
            return cached
        return {
            "available": False,
            "retryable": True,
            "error": "Your collection is still loading from Riot.",
        }
    except Exception:
        app.logger.exception("inventory snapshot failed")
        cached = inventory.last_good(owner)
        if cached:
            return cached
        return {
            "available": False,
            "retryable": True,
            "error": "Couldn't read your collection from the Riot client.",
        }


@app.get("/api/inventory")
def inventory_route() -> ResponseReturnValue:
    return jsonify(_inventory_payload())


@app.get("/api/encounters/<puuid>")
def encounter(puuid: str) -> ResponseReturnValue:
    return jsonify(encounter_log.get_one(_current_puuid(), puuid.strip()))


@app.put("/api/matches/<match_id>/meta")
def match_meta_update(match_id: str) -> ResponseReturnValue:
    return jsonify(
        match_meta.update(_current_puuid(), match_id.strip(), request.get_json(silent=True) or {})
    )


@app.get("/api/match/<match_id>")
def match(match_id: str) -> ResponseReturnValue:
    subject = request.args.get("subject")
    if _live_enabled():
        try:
            data = live_match.LiveMatch(LocalAuth()).match_detail(match_id, subject)
            if not data.get("error"):
                return jsonify(data)
        except Exception:
            app.logger.exception("match detail failed")
    return jsonify(sample_match.match_detail(match_id, subject))


@app.get("/api/debug/reveal")
def debug_reveal() -> ResponseReturnValue:
    if not _live_enabled():
        return jsonify({"error": "Live client not available. Open VALORANT."}), 400
    try:
        return jsonify(live_match.LiveMatch(LocalAuth()).diagnose_reveal())
    except Exception as e:
        app.logger.exception("debug reveal failed")
        return jsonify({"error": str(e)}), 500


@app.get("/api/profile/<puuid>")
def profile(puuid: str) -> ResponseReturnValue:
    puuid = puuid.strip()
    if not puuid:
        return jsonify({"error": "puuid required"}), 400

    now = time.time()
    cached = _CACHE.get(f"profile:{puuid}")
    if cached and now - cached[0] < _CACHE_TTL:
        return jsonify(cached[1])

    data = None
    if _live_enabled():
        try:
            data = live_match.LiveMatch(LocalAuth()).player_career(puuid)
            if not data.get("matches"):
                data = None
        except Exception:
            app.logger.exception("live profile failed")
            data = None
    if data is None:
        data = sample_match.career(puuid)

    _CACHE[f"profile:{puuid}"] = (now, data)
    return jsonify(data)


@app.get("/api/player/<puuid>")
def player(puuid: str) -> ResponseReturnValue:
    puuid = puuid.strip()
    if not puuid or len(puuid) < 6:
        return jsonify({"error": "A valid PUUID (or Riot identifier) is required."}), 400

    now = time.time()
    cached = _CACHE.get(puuid)
    if cached and now - cached[0] < _CACHE_TTL:
        return jsonify(cached[1])

    try:
        payload = build_player_payload(puuid)
    except Exception as e:
        app.logger.exception("player payload failed")
        return jsonify({"error": f"Failed to build player profile: {e}"}), 500

    _CACHE[puuid] = (now, payload)
    return jsonify(payload)


@app.get("/api/region")
def region() -> ResponseReturnValue:
    detected = None
    if LocalAuth.available():
        try:
            detected = LocalAuth().shard
        except Exception:
            detected = None
    return jsonify({"detected": detected, "regions": REGIONS})


@app.post("/api/launch-offline")
def launch_offline() -> ResponseReturnValue:
    import offline_launch

    body = request.get_json(silent=True) or {}
    result = offline_launch.launch(body.get("status"))
    return jsonify(result), (200 if result.get("ok") else 400)


@app.get("/api/offline-status")
def offline_status() -> ResponseReturnValue:
    import offline_launch

    return jsonify(offline_launch.status())


@app.post("/api/offline-toggle")
def offline_toggle() -> ResponseReturnValue:
    import offline_launch

    body = request.get_json(silent=True) or {}
    if "status" in body:
        result = offline_launch.set_status(str(body["status"]))
    else:
        result = offline_launch.set_enabled(bool(body.get("enabled", True)))
    return jsonify(result), (200 if result.get("ok") else 400)


@app.get("/")
def index() -> ResponseReturnValue:
    return jsonify(
        {
            "service": "Valorant Overseer API",
            "endpoints": [
                "/api/health",
                "/api/live",
                "/api/profile/<puuid>",
                "/api/agents",
                "/api/settings",
                "/api/encounters",
            ],
        }
    )


def _current_weapons(puuid: str) -> list[Any]:
    try:
        board = build_live(7, None)
        for p in board.get("players") or []:
            if p.get("puuid") == puuid:
                return p.get("weapons") or []
    except Exception:
        pass
    return []


def handle_data_request(req_type: str, params: dict[str, Any] | None) -> dict[str, Any]:
    params = params or {}
    try:
        if req_type == "profile":
            puuid = (params.get("puuid") or "").strip()
            if not puuid:
                return {"error": "puuid required"}
            data = None
            if _live_enabled():
                try:
                    d = live_match.LiveMatch(LocalAuth()).player_career(puuid)
                    if d.get("matches"):
                        data = d
                except Exception:
                    app.logger.exception("transport profile failed")
            if data is None:
                if client.source_pref != "demo":
                    return {"error": "No career available. Open VALORANT and sign in."}
                data = sample_match.career(puuid)
            out = dict(data)

            out["weapons"] = _current_weapons(puuid)
            try:
                out["encounter"] = encounter_log.get_one(_current_puuid(), puuid)
            except Exception:
                out["encounter"] = None
            return out

        if req_type == "match":
            match_id = (params.get("matchId") or "").strip()
            subject = params.get("subject")
            if not match_id:
                return {"error": "matchId required"}
            if _live_enabled():
                try:
                    d = live_match.LiveMatch(LocalAuth()).match_detail(match_id, subject)
                    if not d.get("error"):
                        return d
                except Exception:
                    app.logger.exception("transport match failed")
            if client.source_pref != "demo":
                return {"error": "No match detail available. Open VALORANT and sign in."}
            return sample_match.match_detail(match_id, subject)

        if req_type == "encounter":
            return (
                encounter_log.get_one(_current_puuid(), (params.get("puuid") or "").strip()) or {}
            )

        if req_type == "recap":
            live_recap = session_tracker.current_recap() if _live_enabled() else None
            if live_recap:
                return live_recap
            # Nothing has finished while the app was watching, so go and read
            # the last one that finished at all. Opening this view cold used to
            # show an empty panel, which reads as broken rather than as idle.
            if _live_enabled():
                try:
                    last = session_tracker.recap_of_last_match(live_match.LiveMatch(LocalAuth()))
                except Exception:
                    app.logger.exception("last match recap failed")
                    last = None
                if last:
                    return last
            if client.source_pref != "demo":
                return {"error": "No completed match found. Play one, or open VALORANT."}
            return sample_match.recap(int(params.get("seed") or 7))

        if req_type == "encounters":
            owner = _current_puuid()
            _refresh_encounter_history(owner)
            scope = "all" if params.get("scope") == "all" else "current"
            return {
                "players": encounter_log.get_all_accounts()
                if scope == "all"
                else encounter_log.get_all(owner),
                "accountCount": encounter_log.account_count(),
                "scope": scope,
            }

        if req_type == "insights":
            return _insights_payload(params.get("tz"))

        if req_type == "performance":
            return _performance_payload(params.get("tz"), int(params.get("richLimit") or 20))

        if req_type == "sessions":
            return session_tracker.list_for(_current_puuid())

        if req_type == "match_meta":
            return match_meta.get_one(_current_puuid(), (params.get("matchId") or "").strip())

        if req_type == "inventory":
            return _inventory_payload()
    except Exception as e:
        return {"error": f"request failed: {e}"}
    return {"error": f"unknown request '{req_type}'"}


def _start_ws_bridge() -> None:
    global _COMMAND_ROUTER
    import overseer_commands
    import ws_server

    frontend_url = os.getenv("FRONTEND_URL", "").strip().rstrip("/")
    ws_port = int(os.getenv("WS_PORT", "7878"))

    def ws_state_provider() -> dict[str, Any]:

        board = dict(build_live(7, None))
        board["agents"] = AGENTS
        return board

    router = overseer_commands.CommandRouter(
        riot_client=client,
        board_provider=ws_state_provider,
    )
    _COMMAND_ROUTER = router

    try:
        token = ws_server.start(
            board_provider=ws_state_provider,
            command_router=router,
            frontend_url=frontend_url,
            ws_port=ws_port,
            request_handler=handle_data_request,
            backend_port=int(os.getenv("BACKEND_PORT", os.getenv("PORT", "5000"))),
        )
    except Exception as e:
        app.logger.exception("VG-WS-001 WebSocket bridge failed to start")
        print(f"[app] VG-WS-001 WebSocket bridge failed: {e}", flush=True)
        raise SystemExit(1) from e
    _write_bridge_file(ws_port, token)


def _write_bridge_file(ws_port: int, token: str) -> None:
    import ws_server

    # write_atomic is the same mkstemp/replace dance this used to inline, and
    # it makedirs the parent itself. It returns False rather than raising.
    if not write_atomic(
        str(overseerlog.OVERSEER_DIR / "bridge.json"),
        {
            "wsPort": ws_port,
            "token": token,
            "apiToken": API_TOKEN,
            "protocol": ws_server.PROTOCOL_VERSION,
            "pid": os.getpid(),
        },
        prefix=".bridge-",
    ):
        app.logger.error("bridge.json write failed, CLI bridge unavailable")


if __name__ == "__main__" and "--self-check" in sys.argv:
    # The local API used to be readable and drivable by any page open in any
    # browser tab. This is the check that says it is not, and it runs without a
    # port or a Riot client.
    _c = app.test_client()
    assert _c.get("/api/health").status_code == 200, "readiness probe must stay open"

    for _route in ("/api/live", "/api/encounters", "/api/inventory", "/api/insights"):
        assert _c.get(_route).status_code == 401, f"{_route} must need the token"
    for _route in ("/api/launch-offline",):
        assert _c.post(_route, json={"dryRun": False}).status_code == 401, (
            f"{_route} must need the token"
        )

    # A near-miss is still a miss.
    assert _c.get("/api/live", headers={"X-Overseer-Token": API_TOKEN[:-1]}).status_code == 401
    assert _c.get("/api/live", headers={"X-Overseer-Token": API_TOKEN}).status_code == 200

    # No dashboard configured means no cross-origin headers at all, so a page
    # cannot read a reply even if it manages to send the request.
    _hdrs = _c.get("/api/health", headers={"Origin": "https://evil.example"}).headers
    assert "Access-Control-Allow-Origin" not in _hdrs, _hdrs

    print("app self-check OK (api requires a token, no wildcard cors)")
    raise SystemExit(0)

if __name__ == "__main__":
    port = int(os.getenv("BACKEND_PORT", os.getenv("PORT", "5000")))
    debug = os.getenv("FLASK_DEBUG", "false").lower() == "true"
    discord_presence.maybe_start()

    if not debug or os.getenv("WERKZEUG_RUN_MAIN") == "true":
        _start_ws_bridge()
    print(
        f"[app] Valorant Overseer API on http://127.0.0.1:{port}  "
        f"(source={client.source_pref}, key={'set' if client.api_key else 'unset'})",
        flush=True,
    )
    try:
        app.run(host="127.0.0.1", port=port, debug=debug)
    except OSError as e:
        app.logger.error("VG-BACKEND-001 could not bind 127.0.0.1:%s: %s", port, e)
        print(f"[app] VG-BACKEND-001 could not bind 127.0.0.1:{port}: {e}", flush=True)
        raise SystemExit(1) from e

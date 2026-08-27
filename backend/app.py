from __future__ import annotations

import os
import sys
import threading
import time
from typing import Any

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
import sample_match
import session_tracker
from agents import AGENTS
from common import write_atomic
from riot_client import ClientNotReadyError, LocalAuth, RiotClient
from vconstants import APP_VERSION, rank_from_tier

# The HTTP API this file used to serve is deleted, not disabled. Every route
# existed for a web dashboard; the terminal scoreboard never sent a single
# HTTP request. What remains is the data layer and the token-authenticated
# local WebSocket bridge, which is the whole surface now.
LOG = overseerlog.get_logger("backend")

client = RiotClient()

_ENCOUNTER_BACKFILL_AT: dict[str, float] = {}
_ENCOUNTER_BACKFILL_LOCK = threading.Lock()


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
                    LOG.exception("session tracking failed")

                try:
                    encounter_log.record_board(board)
                    _attach_encounters(board)
                    _attach_stacks(board)
                except Exception:
                    LOG.exception("encounter logging failed")
                board["appVersion"] = APP_VERSION
                _LAST_GOOD["board"], _LAST_GOOD["at"] = board, time.time()
                _LAST_GOOD["notReady"] = False
                return board
            except Exception as e:
                if isinstance(e, ClientNotReadyError):
                    if not _LAST_GOOD["notReady"]:
                        LOG.info("live scoreboard: %s, waiting for sign-in", e)
                        _LAST_GOOD["notReady"] = True
                else:
                    LOG.exception("live scoreboard failed")

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
            LOG.exception("encounter history backfill failed")


def _current_puuid() -> str | None:
    if not _live_enabled():
        return None
    try:
        auth = LocalAuth()
        auth.headers()
        return auth.puuid
    except Exception:
        return None


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
            LOG.exception("rr history refresh failed")
    return history.payload(puuid, timezone_name)


def _performance_payload(timezone_name: str | None = None, rich_limit: int = 20) -> dict[str, Any]:
    payload = _insights_payload(timezone_name)
    owner = (payload.get("account") or {}).get("puuid")
    if owner and _live_enabled():

        def enrich_recent() -> None:
            try:
                history.enrich(live_match.LiveMatch(LocalAuth()), owner, rich_limit)
            except Exception:
                LOG.exception("performance enrichment failed")

        threading.Thread(
            target=enrich_recent, daemon=True, name=f"perf-enrich-{str(owner)[:8]}"
        ).start()
    if owner:
        session_tracker.ensure_active(owner, payload.get("summary", {}))
    payload["sessions"] = session_tracker.list_for(owner)
    payload["matchMeta"] = match_meta.get_all(owner)
    payload["encounters"] = encounter_log.get_all(owner)
    return payload


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
        LOG.exception("inventory snapshot failed")
        cached = inventory.last_good(owner)
        if cached:
            return cached
        return {
            "available": False,
            "retryable": True,
            "error": "Couldn't read your collection from the Riot client.",
        }


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
                    LOG.exception("transport profile failed")
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
                    LOG.exception("transport match failed")
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
                    LOG.exception("last match recap failed")
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


def _start_ws_bridge() -> int:
    import overseer_commands
    import ws_server

    ws_port = int(os.getenv("WS_PORT", "7878"))

    def ws_state_provider() -> dict[str, Any]:

        board = dict(build_live(7, None))
        board["agents"] = AGENTS
        return board

    router = overseer_commands.CommandRouter(
        riot_client=client,
        board_provider=ws_state_provider,
    )

    try:
        token = ws_server.start(
            board_provider=ws_state_provider,
            command_router=router,
            ws_port=ws_port,
            request_handler=handle_data_request,
        )
    except Exception as e:
        LOG.exception("VG-WS-001 WebSocket bridge failed to start")
        print(f"[app] VG-WS-001 WebSocket bridge failed: {e}", flush=True)
        raise SystemExit(1) from e
    _write_bridge_file(ws_port, token)
    return ws_port


def _write_bridge_file(ws_port: int, token: str) -> None:
    import ws_server

    # write_atomic is the same mkstemp/replace dance this used to inline, and
    # it makedirs the parent itself. It returns False rather than raising.
    if not write_atomic(
        str(overseerlog.OVERSEER_DIR / "bridge.json"),
        {
            "wsPort": ws_port,
            "token": token,
            "protocol": ws_server.PROTOCOL_VERSION,
            "pid": os.getpid(),
            # Which launch wrote this. The pid cannot answer that: the venv's
            # python.exe on Windows re-execs, so the process the launcher
            # started and the process that writes this file have different
            # pids, and a launcher matching on pid waits for a match that can
            # never come.
            "launchId": os.getenv("OVERSEER_LAUNCH_ID", ""),
        },
        prefix=".bridge-",
    ):
        LOG.error("bridge.json write failed, CLI bridge unavailable")


if __name__ == "__main__" and "--self-check" in sys.argv:
    # The HTTP API is gone, so the bridge's request router is the whole
    # surface. This is the guard that its demo-mode answers stay well-formed
    # without a port, a game or a network. Demo is forced rather than assumed
    # so the check gives one answer no matter what backend\.env says.
    client.source_pref = "demo"

    assert handle_data_request("nope", None) == {"error": "unknown request 'nope'"}

    _board = build_live(7, None)
    assert _board.get("players"), "demo board must have players"

    for _req, _params in (
        ("recap", {}),
        ("encounters", {}),
        ("sessions", {}),
        ("profile", {"puuid": str((_board["players"][0] or {}).get("puuid") or "demo")}),
    ):
        _out = handle_data_request(_req, _params)
        assert isinstance(_out, dict) and not _out.get("error"), (_req, _out)

    print("app self-check OK (bridge requests answer in demo mode)")
    raise SystemExit(0)

if __name__ == "__main__":
    discord_presence.maybe_start()
    port = _start_ws_bridge()
    print(
        f"[app] Valorant Overseer bridge on ws://127.0.0.1:{port}  "
        f"(source={client.source_pref}, key={'set' if client.api_key else 'unset'})",
        flush=True,
    )
    # The bridge runs in a daemon thread; this thread's only job is to stay
    # alive until the launcher stops the process.
    try:
        threading.Event().wait()
    except KeyboardInterrupt:
        pass

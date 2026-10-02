# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""The backend: the bridge the window reads.

It builds the board from the VALORANT client, or from the demo, and answers
the screens' requests for history, careers, lineups and offline chat.
"""

from __future__ import annotations

import os
import sys
import threading
import time
from typing import Any

try:
    from dotenv import load_dotenv

    load_dotenv()
except Exception as e:
    # run.py has already put both .env files into the environment this process
    # inherits, so a bad file only matters when app.py is run by hand.
    import overseerlog

    overseerlog.get_logger("backend").warning(f"backend/.env not loaded: {e!r}")

import contextlib

import cards
import encounter_log
import history
import live_match
import overseerlog
import party_detector
import past_games
import sample_match
import session_tracker
import tags
from agents import AGENTS
from common import write_atomic
from common.check import check
from riot_client import ClientNotReadyError, LocalAuth, RiotClient
from vconstants import APP_VERSION

import lineups

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
    """Mark the players who look like they queued together."""
    # Riot only reveals a party for accounts whose presence is visible, which
    # means friends and yourself. For everyone else this guess is the only
    # signal there is, so it carries the numbers it was drawn from and can be
    # judged rather than taken on trust.
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
        "message": "Couldn't read VALORANT. Close the game completely and open it again.",
    }


_LAST_GOOD: dict[str, Any] = {"board": None, "at": 0.0, "notReady": False}
_HOLD_SECS = 12

_BUILD_LOCK = threading.Lock()
_BUILD_FRESH = 3.5


def build_live(seed: int = 7, want_state: str | None = None) -> dict[str, Any]:
    """Return the board: the live one when the client is up, else the demo for `seed`."""
    notice = None
    if _live_enabled():
        with _BUILD_LOCK:
            if _LAST_GOOD["board"] and time.time() - _LAST_GOOD["at"] < _BUILD_FRESH:
                return _LAST_GOOD["board"]
            try:
                lm = live_match.LiveMatch(LocalAuth())
                built = lm.build_scoreboard()
                # The lobby's board is cached for twenty seconds and the same
                # players come back every tick. Everything below writes into
                # them, and tagging drops the habits it reads, so they get
                # their own copies or the next tick's tags have nothing to read.
                board = {
                    **built,
                    "players": [
                        dict(p) if isinstance(p, dict) else p for p in built.get("players") or []
                    ],
                }
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
                tags.attach(board)
                cards.attach(board)
                board["appVersion"] = APP_VERSION
                _LAST_GOOD["board"], _LAST_GOOD["at"] = board, time.time()
                _LAST_GOOD["notReady"] = False
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
                        "notice": notice,
                        "appVersion": APP_VERSION,
                    }
            else:
                return board
    elif client.source_pref != "demo" and not LocalAuth.available():
        notice = _client_notice()

    if client.source_pref == "demo":
        # DEMO_STATE picks the lobby or agent select, for looking at the rows
        # nobody has picked an agent for yet.
        state = (want_state or os.getenv("DEMO_STATE") or "").lower()
        if state == "menus":
            board = sample_match.generate_lobby(seed)
        elif state == "pregame":
            board = sample_match.generate_pregame(seed)
        else:
            board = sample_match.generate(seed)
        board = cards.attach(tags.attach(_attach_encounters(board)))
        if notice:
            board["notice"] = notice
        board["appVersion"] = APP_VERSION
        return board

    # No client, and nobody asked for the demo. An invented Diamond 3 looks
    # just like a real one, so the board stays empty and says what it is
    # waiting for. DATA_SOURCE=demo is the only way to see sample players.
    return {
        "state": "OFFLINE",
        "stateLabel": "Waiting for VALORANT",
        "source": "idle",
        "players": [],
        "teams": {},
        "notice": notice
        or {
            "level": "info",
            "action": "open_game",
            "message": "Open VALORANT and join a lobby, Agent Select or a match.",
        },
        "appVersion": APP_VERSION,
    }


def _current_puuid() -> str | None:
    if not _live_enabled():
        return None
    try:
        auth = LocalAuth()
        auth.headers()
    except (ClientNotReadyError, FileNotFoundError):
        # The game is closed or still signing in, which is normal.
        return None
    except Exception as e:
        # LocalAuth can fail any number of ways while the client starts or
        # after a patch. A request without an account still gets an answer.
        LOG.warning("could not read the signed-in account: %r", e)
        return None
    else:
        return auth.puuid


def _board_player(puuid: str) -> dict[str, Any]:
    try:
        board = build_live(7, None)
        for p in board.get("players") or []:
            if p.get("puuid") == puuid:
                return p
    except Exception as e:
        LOG.warning("could not read the board to find %s: %r", puuid, e)
    return {}


def _current_weapons(puuid: str) -> list[Any]:
    return _board_player(puuid).get("weapons") or []


def _lineup_request(req_type: str, params: dict[str, Any]) -> dict[str, Any]:
    """Answer the Lineups screen's requests.

    A lineup that can't be saved or played answers with why, in words for
    the player.
    """
    try:
        if req_type == "lineups":
            # The blacked-out figure is cut from the agent played most, which the
            # board only knows in a lobby, so History's record answers otherwise.
            return {**lineups.overview(), "main": history.main_agent()}
        if req_type == "lineup_save":
            return lineups.save(params.get("lineup"), params.get("clip"))
        if req_type == "lineup_delete":
            return lineups.delete(params.get("map"), params.get("id"))
        if req_type == "lineup_probe":
            source = str(params.get("source") or "").strip()
            return {"source": source, "length": lineups.length(source)}
        if req_type == "lineup_watch":
            return lineups.watch(params.get("source"))
        if req_type == "lineup_drawing":
            return lineups.save_drawing(params.get("map"), params.get("shapes"))
    except lineups.LineupError as e:
        return {"error": str(e)}
    return {"error": f"The backend has no '{req_type}' request."}


def _offline_set(*, on: bool) -> dict[str, Any]:
    """Hide you in chat, or show you again.

    Hiding needs the Riot client started through the chat proxy, so the first
    time that restarts it. After that it is a switch.
    """
    import offline_launch

    state = offline_launch.status()
    if on and not state.get("running"):
        result = offline_launch.launch("offline")
    else:
        result = offline_launch.set_enabled(enabled=on)
    return {**result, **offline_launch.status()}


def handle_data_request(req_type: str, params: dict[str, Any] | None) -> dict[str, Any]:
    """Answer one of the screens' data requests, or return an error saying what went wrong."""
    params = params or {}
    try:
        if req_type == "profile":
            puuid = (params.get("puuid") or "").strip()
            if not puuid:
                return {"error": "The profile request needs a puuid."}
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
                now = _board_player(puuid)
                data = sample_match.career(puuid, now.get("rankTier"), now.get("rr"))
            out = dict(data)
            out["weapons"] = _current_weapons(puuid)
            # Your own career reads who you play with from every lobby on
            # record. Its last eight matches would miss most of a stack.
            owner = _current_puuid()
            if owner and puuid == owner:
                mates = encounter_log.teammates(owner)
                if mates:
                    out["coPlayers"], out["coPlayersFrom"] = mates, "log"
            try:
                out["encounter"] = encounter_log.get_one(_current_puuid(), puuid)
            except Exception as e:
                LOG.warning("encounter lookup for %s failed: %r", puuid, e)
                out["encounter"] = None
            return out

        if req_type == "history":
            try:
                count = int(params.get("count") or 10)
            except (TypeError, ValueError):
                count = 10
            owner = _current_puuid()
            if owner:
                return past_games.recent(LocalAuth(), owner, count)
            if client.source_pref != "demo":
                return {"error": "Open VALORANT and sign in to see your past games."}
            return sample_match.history(count)

        if req_type.startswith("lineup"):
            return _lineup_request(req_type, params)

        if req_type == "offline":
            import offline_launch

            return {"ok": True, **offline_launch.status()}

        if req_type == "offline_set":
            return _offline_set(on=bool(params.get("on")))
    except Exception as e:
        # One bad request answers with an error and leaves the bridge serving.
        LOG.exception("request %r failed with params %r", req_type, params)
        return {
            "error": f"The {req_type} request failed ({e}). The trace is in .overseer/backend.log."
        }
    return {"error": f"The backend has no '{req_type}' request."}


def _start_ws_bridge() -> int:
    import ws_server

    ws_port = int(os.getenv("WS_PORT", "7878"))

    def ws_state_provider() -> dict[str, Any]:
        board = dict(build_live(7, None))
        board["agents"] = AGENTS
        return board

    try:
        token = ws_server.start(
            board_provider=ws_state_provider,
            ws_port=ws_port,
            request_handler=handle_data_request,
        )
    except Exception as e:
        LOG.exception("VG-WS-001 WebSocket bridge failed to start")
        print(f"[app] VG-WS-001 WebSocket bridge failed: {e}", flush=True)
        raise SystemExit(1) from e
    _write_bridge_file(ws_port, token)
    return ws_port


def _warm_lineups() -> None:
    """Fetch what the Lineups screen draws from while nobody waits on it.

    The maps and agents come from valorant-api and the minimaps and icons go
    onto disk, so the first time the screen asks, the answer comes from memory
    instead of the network.
    """
    try:
        lineups.prune_cache()
        lineups.overview()
    except Exception:
        LOG.exception("lineups warm-up failed")


def _write_bridge_file(ws_port: int, token: str) -> None:
    import ws_server

    if not write_atomic(
        str(overseerlog.OVERSEER_DIR / "bridge.json"),
        {
            "wsPort": ws_port,
            "token": token,
            "protocol": ws_server.PROTOCOL_VERSION,
            "pid": os.getpid(),
            # The launcher matches on this, not the pid. The venv's python.exe
            # re-execs, so the process the launcher started is not the one
            # that writes this file.
            "launchId": os.getenv("OVERSEER_LAUNCH_ID", ""),
        },
        prefix=".bridge-",
    ):
        LOG.error("bridge.json write failed, so the window cannot find the bridge")


if __name__ == "__main__" and "--self-check" in sys.argv:
    from types import SimpleNamespace
    from unittest import mock

    # The bridge's request router is the whole data surface, so this checks
    # that its demo answers stay well formed with no port, game or network.
    # Demo is forced so backend\.env cannot change the answer.
    client.source_pref = "demo"

    check(handle_data_request("nope", None) == {"error": "The backend has no 'nope' request."})

    _board = build_live(7, None)
    check(_board.get("players"), "demo board must have players")

    for _req, _params in (
        ("history", {"count": 5}),
        ("profile", {"puuid": str((_board["players"][0] or {}).get("puuid") or "demo")}),
    ):
        _out = handle_data_request(_req, _params)
        check(isinstance(_out, dict) and not _out.get("error"), (_req, _out))
    _games = handle_data_request("history", {"count": "7"})["games"]
    _refused = handle_data_request("lineup_save", {"lineup": {"map": "Ascent"}})
    check(_refused == {"error": "A lineup needs a title and an agent."}, _refused)
    check(len(_games) == 7 and _games[0]["score"] and _games[0]["rrDelta"], _games[0])

    # A cached board comes back as the same objects every tick, and tagging it
    # must not use up the habits its tags are read from.
    _cached = {
        "state": "MENUS",
        "players": [
            {"puuid": "me", "habits": {"matches": 5, "rounds": 100, "carried": {"Operator": 30}}}
        ],
    }
    _fake = SimpleNamespace(build_scoreboard=lambda **_: _cached, self_puuid="me")
    with (
        mock.patch(f"{__name__}._live_enabled", return_value=True),
        mock.patch(f"{__name__}.LocalAuth"),
        mock.patch.object(live_match, "LiveMatch", return_value=_fake),
        mock.patch.object(session_tracker, "observe"),
        mock.patch.object(session_tracker, "attach"),
        mock.patch.object(encounter_log, "record_board"),
        mock.patch(f"{__name__}._attach_encounters"),
        mock.patch(f"{__name__}._attach_stacks"),
        mock.patch.object(cards, "attach"),
    ):
        for _tick in range(2):
            _LAST_GOOD["board"] = None
            _read = [t["tag"] for t in build_live()["players"][0]["autoTags"]]
            check("op" in _read, (_tick, _read))

    print("app self-check OK (bridge requests answer in demo mode, cached boards keep their tags)")
    raise SystemExit(0)

if __name__ == "__main__":
    port = _start_ws_bridge()
    threading.Thread(target=_warm_lineups, daemon=True, name="lineups-warm").start()
    print(
        f"[app] Valorant Overseer bridge on ws://127.0.0.1:{port}  "
        f"(source={client.source_pref}, key={'set' if client.api_key else 'unset'})",
        flush=True,
    )
    # The bridge runs in a daemon thread. This one only has to stay alive
    # until the launcher stops the process.
    with contextlib.suppress(KeyboardInterrupt):
        threading.Event().wait()

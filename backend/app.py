# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""The backend: the bridge the window reads."""

from __future__ import annotations

import os
import sys
import threading
import time
from pathlib import Path
from typing import Any

from common import load_env

if __name__ == "__main__":
    load_env(Path(__file__).parent / ".env")

import contextlib

import agents
import art
import brief
import cards
import encounter_log
import history
import live_match
import newest
import overseerlog
import party_detector
import past_games
import presence
import refresh
import sample_match
import session_tracker
import tags
from agents import AGENTS
from common import data_path, env_number, read_json, write_atomic
from common.check import check
from common.system import process_names
from riot_client import (
    ClientNotReadyError,
    GameNotStartedError,
    LocalAuth,
    RiotClient,
    chat_presences,
    trust_windows_roots,
)
from vconstants import APP_VERSION

import lineups

LOG = overseerlog.get_logger("backend")

client = RiotClient()


def _live_enabled() -> bool:
    return client.source_pref != "demo" and LocalAuth.available()


def _attach_encounters(board: dict[str, Any]) -> dict[str, Any]:
    owner = board.get("selfPuuid") if board.get("source") == "local" else None
    ours = board.get("selfTeam")
    for p in board.get("players") or []:
        if not isinstance(p, dict):
            continue
        met = encounter_log.encounter_for(owner, p.get("puuid")) if owner else None
        if met:
            side = "withCount" if ours is not None and p.get("team") == ours else "againstCount"
            met[side] = max(0, met[side] - 1)
        p["encounter"] = met
    return board


def _attach_stacks(board: dict[str, Any]) -> dict[str, Any]:
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


def _waiting_for(e: Exception) -> str | None:
    if not LocalAuth.available() or isinstance(e, (ClientNotReadyError, FileNotFoundError)):
        _GAME_WAIT.clear()
        return "riot"
    if isinstance(e, GameNotStartedError):
        if not _game_running():
            _GAME_WAIT.clear()
            return "game"
        now = time.time()
        began = _GAME_WAIT.setdefault("since", now)
        # VALORANT names its region within a minute of starting, so a log
        # that still hasn't after two is something waiting won't fix.
        return "game" if now - began < _GAME_PATIENCE else None
    return None


def _fault(e: BaseException) -> str:
    at = e.__traceback__
    while at is not None and at.tb_next is not None:
        at = at.tb_next
    where = f"{at.tb_frame.f_code.co_filename}:{at.tb_lineno}" if at is not None else ""
    return f"{type(e).__name__} {where}"


def _game_running() -> bool:
    try:
        return _GAME_EXE in process_names()
    except Exception as e:
        LOG.warning("could not list processes to look for VALORANT: %r", e)
        return True


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


_LAST_GOOD: dict[str, Any] = {"board": None, "at": 0.0, "notReady": False, "failing": ""}
# When the client was first found up with VALORANT's region still unknown.
_GAME_WAIT: dict[str, float] = {}
_GAME_PATIENCE = 120.0
_GAME_EXE = "valorant-win64-shipping.exe"
_HOLD_SECS = 12
_INGAME_POLL = env_number("WS_STATE_POLL_INGAME", 15.0, low=0.5, high=60.0)

_BUILD_LOCK = threading.Lock()
_BUILD_FRESH = 3.5


_YOU = data_path("you.json")
_YOU_KEPT: dict[str, Any] = {}


def _keep_you(board: dict[str, Any]) -> None:
    me = next(
        (
            p
            for p in board.get("players") or []
            if isinstance(p, dict) and p.get("puuid") and p.get("puuid") == board.get("selfPuuid")
        ),
        None,
    )
    if me is None or me == _YOU_KEPT.get("player"):
        return
    _YOU_KEPT.update(player=me, at=time.time())
    write_atomic(_YOU, dict(_YOU_KEPT), prefix=".you-", durable=False)


def saved_you() -> dict[str, Any] | None:
    """Return your last kept row and when it was kept, or None before there was one."""
    if not _YOU_KEPT:
        saved = read_json(_YOU, {})
        if isinstance(saved, dict) and isinstance(saved.get("player"), dict):
            _YOU_KEPT.update(player=saved["player"], at=saved.get("at"))
    return dict(_YOU_KEPT) or None


def _in_match() -> bool:
    return (_LAST_GOOD["board"] or {}).get("state") == "INGAME"


def board_pace(base: float) -> float:
    """How long to wait before the next board: slower while a match is on."""
    return max(base, _INGAME_POLL) if _in_match() else base


def build_live(seed: int = 7, want_state: str | None = None) -> dict[str, Any]:
    """Return the board: the live one when the client is up, else the demo for `seed`."""
    notice = None
    if _live_enabled():
        with _BUILD_LOCK:
            stale = refresh.take_stale()
            age = time.time() - _LAST_GOOD["at"]
            if _LAST_GOOD["board"] and not stale and 0 <= age < _BUILD_FRESH:
                return _LAST_GOOD["board"]
            try:
                lm = live_match.LiveMatch(LocalAuth())
                built = lm.build_scoreboard()
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
                brief.attach(board)
                cards.attach(board)
                board["appVersion"] = APP_VERSION
                try:
                    _keep_you(board)
                except (OSError, TypeError, ValueError):
                    LOG.exception("keeping your own row failed")
                board["you"] = saved_you()
                _LAST_GOOD["board"], _LAST_GOOD["at"] = board, time.time()
                _LAST_GOOD["notReady"] = False
                _LAST_GOOD["failing"] = ""
                _GAME_WAIT.clear()
            except Exception as e:
                waiting = _waiting_for(e)
                if waiting:
                    if not _LAST_GOOD["notReady"]:
                        LOG.info("live scoreboard: %s, waiting for %s", e, waiting)
                        _LAST_GOOD["notReady"] = True
                elif _LAST_GOOD["failing"] != _fault(e):
                    # Once for each fault. A board is built every few seconds,
                    # and one that fails all evening the same way would fill
                    # every log file with the one trace.
                    _LAST_GOOD["failing"] = _fault(e)
                    LOG.exception("live scoreboard failed")

                # Once a match is on, keep its board through a dropped or slow
                # connection until a good build shows the match is over. The
                # round only ends in the game, so bad Wi-Fi is no reason to
                # clear the board you are reading. The client closing is: then
                # the lockfile is gone and the usual hold runs out.
                fresh = 0 <= time.time() - _LAST_GOOD["at"] < _HOLD_SECS
                if _LAST_GOOD["board"] and fresh:
                    return _LAST_GOOD["board"]
                if _LAST_GOOD["board"] and _in_match() and LocalAuth.available():
                    return {**_LAST_GOOD["board"], "heldSince": _LAST_GOOD["at"], "reading": None}
                notice = _client_notice()
                if client.source_pref != "demo":
                    return {
                        "state": "OFFLINE",
                        "stateLabel": "Offline",
                        "source": "local",
                        "error": str(e),
                        "waiting": waiting,
                        "players": [],
                        "teams": {},
                        "notice": notice,
                        "you": saved_you(),
                        "appVersion": APP_VERSION,
                    }
            else:
                return board
    elif client.source_pref != "demo" and not LocalAuth.available():
        notice = _client_notice()

    if client.source_pref == "demo":
        state = (want_state or os.getenv("DEMO_STATE") or "").lower()
        if state == "menus":
            board = sample_match.generate_lobby(seed)
        elif state == "pregame":
            board = sample_match.generate_pregame(seed)
        else:
            board = sample_match.generate(seed)
        board = cards.attach(brief.attach(tags.attach(_attach_encounters(board))))
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
        "waiting": "riot",
        "players": [],
        "teams": {},
        "you": saved_you(),
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
        return None
    except Exception as e:
        LOG.warning("the signed-in account couldn't be read: %r", e)
        return None
    return auth.puuid


def _board_player(puuid: str) -> dict[str, Any]:
    try:
        board = build_live()
        for p in board.get("players") or []:
            if p.get("puuid") == puuid:
                return p
    except Exception as e:
        LOG.warning("could not read the board to find %s: %r", puuid, e)
    return {}


def _lineup_request(req_type: str, params: dict[str, Any]) -> dict[str, Any]:
    try:
        if req_type == "lineups":
            return {**lineups.overview(), "main": history.main_agent()}
        if req_type == "lineup_save":
            return lineups.save(
                params.get("lineup"), params.get("clip"), small=bool(params.get("small"))
            )
        if req_type == "lineup_delete":
            return lineups.delete(params.get("map"), params.get("id"))
        if req_type == "lineup_confirm":
            return lineups.confirm(params.get("map"), params.get("id"))
        if req_type == "lineup_probe":
            source = str(params.get("source") or "").strip()
            return {"source": source, "length": lineups.length(source)}
        if req_type == "lineup_watch":
            return lineups.watch(params.get("source"))
        if req_type == "lineup_drawing":
            return lineups.save_drawing(params.get("map"), params.get("shapes"))
        if req_type == "lineup_export":
            return lineups.export_code(params.get("map"), params.get("ids"))
        if req_type == "lineup_import":
            return lineups.import_code(params.get("code"), small=bool(params.get("small")))
    except lineups.LineupError as e:
        return {"error": str(e)}
    return {"error": f"The backend has no '{req_type}' request."}


def _offline_set(*, on: bool) -> dict[str, Any]:
    import offline_launch

    state = offline_launch.status()
    if on and not state.get("running"):
        last = (_LAST_GOOD["board"] or {}).get("state")
        if last in ("PREGAME", "INGAME") and LocalAuth.available():
            msg = "Finish this match first. Turning it on restarts the Riot Client and VALORANT."
            return {**state, "ok": False, "message": msg}
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
            why = "No career available. Open VALORANT and sign in."
            if _live_enabled():
                try:
                    d = live_match.LiveMatch(LocalAuth()).player_career(puuid)
                    if d.get("matches"):
                        data = d
                    elif d.get("unread"):
                        why = "Riot didn't send this player's games. Try again in a minute."
                    else:
                        why = "This player has no recent games in a standard mode."
                except Exception:
                    LOG.exception("transport profile failed")
            if data is None:
                if client.source_pref != "demo":
                    return {"error": why}
                now = _board_player(puuid)
                data = sample_match.career(puuid, now.get("rankTier"), now.get("rr"))
            out = dict(data)
            out["weapons"] = _board_player(puuid).get("weapons") or []
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
                try:
                    games = past_games.recent(LocalAuth(), owner, count)
                except Exception:
                    # Riot didn't answer, which offline is the usual case.
                    # The saved games stand in, and say how old they are.
                    kept = past_games.saved(owner, count)
                    if kept is None:
                        raise
                    LOG.warning(
                        "history: Riot didn't answer, sending the saved games", exc_info=True
                    )
                    return kept
                return games
            if client.source_pref != "demo":
                return past_games.saved(None, count) or {
                    "error": "Open VALORANT and sign in to see your past games."
                }
            return sample_match.history(count)

        if req_type.startswith("lineup"):
            return _lineup_request(req_type, params)

        if req_type == "games_read":
            try:
                live_match.set_depth(
                    int(params.get("count") or 20),
                    int(params.get("days") or 0),
                    int(params.get("keep", 14)),
                )
            except (TypeError, ValueError):
                return {"error": "games_read needs a count."}
            return {"ok": True}

        if req_type == "discord":
            presence.set_on(on=bool(params.get("on")))
            return {"ok": True}

        if req_type == "offline":
            import offline_launch

            return {"ok": True, **offline_launch.status()}

        if req_type == "offline_set":
            return _offline_set(on=bool(params.get("on")))

        if req_type == "art":
            if client.source_pref == "demo":
                return {"ok": False}
            return art.fetch(params.get("kind"), params.get("name"))

        if req_type == "refresh":
            refresh.soon()
            return {}

        if req_type == "newest":
            return {"newest": None if client.source_pref == "demo" else newest.newest()}
    except Exception as e:
        LOG.exception("request %r failed with params %r", req_type, params)
        return {
            "error": f"The {req_type} request failed ({e}). The trace is in .overseer/backend.log."
        }
    return {"error": f"The backend has no '{req_type}' request."}


def _start_ws_bridge() -> int:
    import ws_server

    ws_port = int(env_number("WS_PORT", 7878, low=1, high=65535))

    def ws_state_provider() -> dict[str, Any]:
        board = dict(build_live())
        # Every board, the empty one after VALORANT closes included, so the
        # status clears with the game. A demo lobby is nobody's match.
        presence.observe(board if client.source_pref != "demo" else {})
        board["agents"] = AGENTS
        return board

    try:
        token = ws_server.start(
            board_provider=ws_state_provider,
            ws_port=ws_port,
            request_handler=handle_data_request,
            pace=board_pace,
        )
    except Exception as e:
        LOG.exception("VG-WS-001 WebSocket bridge failed to start")
        print(f"[app] VG-WS-001 WebSocket bridge failed: {e}", flush=True)
        raise SystemExit(1) from e
    _write_bridge_file(ws_port, token)
    return ws_port


def _watch_state() -> None:
    lm = None
    last = None
    failing = None
    while True:
        time.sleep(2.0)
        if not _live_enabled():
            continue
        try:
            lm = lm or live_match.LiveMatch(LocalAuth())
            state = lm.game_state(chat_presences(lm.auth))
        except Exception as e:
            # The client closed or restarted, with a new lockfile to read.
            if repr(e) != failing:
                failing = repr(e)
                LOG.warning("watching VALORANT's state failed: %r", e)
            lm, last = None, None
            time.sleep(4.0)
            continue
        failing = None
        seen = last if last is not None else (_LAST_GOOD["board"] or {}).get("state")
        if seen is not None and state != seen:
            refresh.soon()
        last = state


def _warm_lineups() -> None:
    try:
        lineups.prune_cache()
        lineups.overview()
        agents.learn()
    except Exception:
        LOG.exception("lineups warm-up failed")


def _write_bridge_file(ws_port: int, token: str) -> None:
    import ws_server

    if not write_atomic(
        str(overseerlog.OVERSEER_DIR / "bridge.json"),
        {
            "wsPort": ws_port,
            "token": token,
            "protocol": ws_server.SPEAKS,
            "pid": os.getpid(),
            # The launcher matches on this, not the pid. The venv's python.exe
            # re-execs, so the process the launcher started is not the one
            # that writes this file.
            "launchId": os.getenv("OVERSEER_LAUNCH_ID", ""),
        },
        prefix=".bridge-",
    ):
        msg = "bridge.json write failed, so the window cannot find the bridge"
        raise OSError(msg)


if __name__ == "__main__" and "--self-check" in sys.argv:
    from types import SimpleNamespace
    from unittest import mock

    client.source_pref = "demo"

    check(handle_data_request("nope", None) == {"error": "The backend has no 'nope' request."})
    check(handle_data_request("newest", None) == {"newest": None}, "demo asks nobody")

    _board = build_live()
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
            _built = build_live()
            _read = [t["tag"] for t in _built["players"][0]["autoTags"]]
            check("op" in _read, (_tick, _read))
            check("you" in _built, "a live board carries your kept row")

    with mock.patch.object(LocalAuth, "available", return_value=False):
        check(_waiting_for(ValueError()) == "riot")
    # VALORANT that never names its region stops being one that is starting.
    _GAME_WAIT.clear()
    with (
        mock.patch.object(LocalAuth, "available", return_value=True),
        mock.patch(f"{__name__}._game_running", return_value=True),
        mock.patch(f"{__name__}.time.time", side_effect=[1000.0, 1000.0 + _GAME_PATIENCE]),
    ):
        check(_waiting_for(GameNotStartedError()) == "game")
        check(_waiting_for(GameNotStartedError()) is None)
    with (
        mock.patch.object(LocalAuth, "available", return_value=True),
        mock.patch(f"{__name__}._game_running", return_value=False),
    ):
        check(_waiting_for(GameNotStartedError()) == "game" and not _GAME_WAIT, _GAME_WAIT)
    _GAME_WAIT["since"] = 0.0
    with mock.patch.object(LocalAuth, "available", return_value=False):
        check(_waiting_for(GameNotStartedError()) == "riot" and not _GAME_WAIT, _GAME_WAIT)
    _auth = mock.MagicMock()
    _auth.available.return_value = True
    with (
        mock.patch.object(client, "source_pref", "auto"),
        mock.patch(f"{__name__}.LocalAuth", _auth),
        mock.patch.object(live_match, "LiveMatch", side_effect=GameNotStartedError("soon")),
    ):
        _LAST_GOOD["board"] = None
        _waited = build_live()
    check(_waited["state"] == "OFFLINE" and _waited["waiting"] == "game", _waited)
    _GAME_WAIT.clear()

    _LAST_GOOD["board"] = {"state": "MENUS"}
    check(board_pace(4.0) == 4.0)
    _LAST_GOOD["board"] = {"state": "INGAME"}
    check(board_pace(4.0) == _INGAME_POLL)
    with (
        mock.patch("offline_launch.status", return_value={"running": False}),
        mock.patch("offline_launch.launch", return_value={"ok": True}) as _launched,
        mock.patch.object(LocalAuth, "available", return_value=True) as _up,
    ):
        _refused = _offline_set(on=True)
        check(_refused["ok"] is False and "Finish this match" in _refused["message"], _refused)
        # A match the client crashed out of is over, and nothing is left to close.
        _up.return_value = False
        check(_offline_set(on=True)["ok"] is True and _launched.call_count == 1)

    _LAST_GOOD["board"], _LAST_GOOD["at"] = {"state": "INGAME", "players": []}, 0.0
    with (
        mock.patch(f"{__name__}._live_enabled", return_value=True),
        mock.patch(f"{__name__}.LocalAuth"),
        mock.patch.object(client, "source_pref", "auto"),
        mock.patch.object(live_match, "LiveMatch", side_effect=OSError("wifi")),
        mock.patch(f"{__name__}.time.time", return_value=1e9),
    ):
        with mock.patch.object(LocalAuth, "available", return_value=True):
            _held = build_live()
            check(_held["state"] == "INGAME", "a match holds through a drop")
            check(_held.get("heldSince") == 0.0, "and says it is held")
        _LAST_GOOD["board"] = {"state": "INGAME", "players": []}
        with (
            mock.patch.object(LocalAuth, "available", return_value=False),
            mock.patch.object(LOG, "exception") as _traced,
        ):
            _LAST_GOOD["failing"] = ""
            with mock.patch(f"{__name__}._waiting_for", return_value=None):
                _states = [build_live()["state"] for _ in range(3)]
            check(_states == ["OFFLINE"] * 3, "a closed client lets the match go")
            check(_traced.call_count == 1, _traced.call_count)
    _LAST_GOOD["board"] = None
    _LAST_GOOD["failing"] = ""

    import tempfile
    from pathlib import Path

    with (
        tempfile.TemporaryDirectory() as _tmp,
        mock.patch(f"{__name__}._YOU", str(Path(_tmp) / "you.json")),
    ):
        _YOU_KEPT.clear()
        check(saved_you() is None, "nothing kept yet")
        _keep_you({"selfPuuid": "me", "players": [{"puuid": "them"}, {"puuid": "me", "kd": 1.2}]})
        _YOU_KEPT.clear()
        _kept = saved_you()
        check(_kept is not None and _kept["player"] == {"puuid": "me", "kd": 1.2}, _kept)
        _keep_you({"selfPuuid": None, "players": [{"puuid": None}]})
        check(saved_you() == _kept, "a board without you keeps the last row")
    _YOU_KEPT.clear()

    print(
        "app self-check OK (bridge requests answer in demo mode, cached boards keep their tags, "
        "empty boards say what they wait on, a match holds through a dropped connection, "
        "your last stats are kept)"
    )
    raise SystemExit(0)


def start() -> int:
    """Start the bridge and what feeds it, each on its own thread, and return its port."""
    trust_windows_roots()
    port = _start_ws_bridge()
    threading.Thread(target=_warm_lineups, daemon=True, name="lineups-warm").start()
    threading.Thread(target=_watch_state, daemon=True, name="state-watch").start()
    print(
        f"[app] Valorant Overseer bridge on ws://127.0.0.1:{port}  "
        f"(source={client.source_pref}, key={'set' if client.api_key else 'unset'})",
        flush=True,
    )
    return port


if __name__ == "__main__":
    start()
    # The bridge runs in a daemon thread. This one only has to stay alive
    # until the launcher stops the process.
    with contextlib.suppress(KeyboardInterrupt):
        threading.Event().wait()

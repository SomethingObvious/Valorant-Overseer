# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""The local WebSocket bridge on 127.0.0.1 the front ends connect to.

It wants the per-launch token first and closes any connection with an Origin
header, since no browser has any business here.
"""

from __future__ import annotations

import asyncio
import functools
import hmac
import json
import os
import secrets
import threading
from dataclasses import dataclass
from http import HTTPStatus
from typing import TYPE_CHECKING, Any

import overseerlog
import refresh
from common import console_logger
from vconstants import APP_VERSION

if TYPE_CHECKING:
    from collections.abc import Callable

try:
    from websockets.exceptions import ConnectionClosed
    from websockets.legacy.server import serve as _ws_serve
except ImportError as e:
    msg = (
        "The 'websockets' package is required for local WebSocket mode. "
        "Run install.bat to repair the installation."
    )
    raise RuntimeError(msg) from e

LOG = overseerlog.get_logger("ws", "websocket")

PROTOCOL_VERSION = 1
SUPPORTED_PROTOCOLS = {1}
CAPABILITIES = ["state", "commands", "requests"]

CLOSE_AUTH = 4401
CLOSE_ORIGIN = 4403
CLOSE_PROTOCOL = 4406


# Speaks even under OVERSEER_QUIET, because the bridge failing to start is
# what the user needs to see.
_log = console_logger("ws", quiet_aware=False, also=LOG)


@dataclass
class _State:
    """The bridge's state for this launch."""

    # The token a client must send first, made when the bridge starts.
    session_token: str = ""


_STATE = _State()


_CLIENTS: set[Any] = set()


async def _process_request(_path: str, request_headers: Any) -> Any:
    try:
        get = request_headers.get
    except AttributeError:
        return None

    # Only a browser sends an Origin header, and no browser has any business
    # here. Whatever page is asking, the answer is no.
    origin = get("Origin")
    if origin is not None:
        _log(f"rejected browser origin: {origin}")
        return (
            HTTPStatus.FORBIDDEN,
            [("Content-Type", "text/plain"), ("Content-Length", "16")],
            b"Forbidden origin",
        )

    return None


async def _safe_send(ws: Any, obj: dict[str, Any]) -> bool:
    try:
        await ws.send(json.dumps(obj, default=str))
    except ConnectionClosed:
        return False
    except (TypeError, ValueError) as e:
        LOG.warning("could not encode a %r message: %r", obj.get("type"), e)
        return False
    else:
        return True


def _parse(raw: Any) -> dict[str, Any]:
    try:
        data = json.loads(raw)
    except (TypeError, ValueError, RecursionError) as e:
        # Only the error goes in the log. The raw text can be an auth message
        # with the session token in it.
        LOG.warning("dropped a message that is not JSON: %r", e)
        return {}
    return data if isinstance(data, dict) else {}


async def _broadcast(obj: Any) -> None:
    if not _CLIENTS:
        return
    payload = json.dumps(obj, default=str)
    dead = []
    for ws in list(_CLIENTS):
        try:
            await ws.send(payload)
        except ConnectionClosed:
            dead.append(ws)
        except Exception as e:
            # One broken client must not stop the others getting the board.
            LOG.warning("dropping a client after a failed send: %r", e)
            dead.append(ws)
    for ws in dead:
        _CLIENTS.discard(ws)


def _self_handshake(ws_port: int, timeout: float = 6.0) -> None:
    from websockets.sync.client import connect as _sync_connect

    with _sync_connect(f"ws://127.0.0.1:{ws_port}", open_timeout=timeout, close_timeout=2) as ws:
        ws.send(
            json.dumps(
                {"type": "auth", "token": _STATE.session_token, "protocol": PROTOCOL_VERSION}
            )
        )
        reply = json.loads(ws.recv(timeout=timeout))
        if reply.get("type") != "auth_ok":
            msg = f"self-handshake got {reply.get('type')!r}"
            raise RuntimeError(msg)


# How long the board loop waits after an early wake for the rest of a burst.
_SETTLE = 0.4


def start(
    *,
    board_provider: Callable[[], dict[str, Any]],
    ws_port: int,
    request_handler: Callable[[str, dict[str, Any]], dict[str, Any]] | None = None,
    poll_interval: float | None = None,
) -> str:
    """Start the WebSocket bridge on a thread and return once it answers."""
    _STATE.session_token = secrets.token_urlsafe(32)
    interval = float(
        poll_interval if poll_interval is not None else os.getenv("WS_STATE_POLL", "4.0")
    )

    ready = threading.Event()
    boot_error: list[BaseException] = []

    def _run() -> None:
        loop = asyncio.new_event_loop()
        asyncio.set_event_loop(loop)

        async def handler(websocket: Any) -> None:
            origin = websocket.request_headers.get("Origin")
            if origin is not None:
                await websocket.close(code=CLOSE_ORIGIN, reason="Forbidden origin")
                return

            try:
                raw = await asyncio.wait_for(websocket.recv(), timeout=5.0)
            except TimeoutError:
                await _safe_send(
                    websocket,
                    {
                        "type": "auth_error",
                        "code": "timeout",
                        "message": "No token arrived within 5 seconds. Restart Valorant Overseer",
                    },
                )
                await websocket.close(code=CLOSE_AUTH, reason="Auth timeout")
                return
            except ConnectionClosed:
                return

            msg = _parse(raw)
            if msg.get("type") != "auth" or not hmac.compare_digest(
                str(msg.get("token") or ""), _STATE.session_token
            ):
                await _safe_send(
                    websocket,
                    {
                        "type": "auth_error",
                        "code": "bad_token",
                        "message": "The bridge didn't accept this token. Restart Valorant Overseer",
                    },
                )
                await websocket.close(code=CLOSE_AUTH, reason="Invalid token")
                return

            client_proto = msg.get("protocol", 1)
            if not isinstance(client_proto, int) or client_proto not in SUPPORTED_PROTOCOLS:
                LOG.warning(
                    "rejected client protocol %r (supported: %s)",
                    client_proto,
                    sorted(SUPPORTED_PROTOCOLS),
                )
                await _safe_send(
                    websocket,
                    {
                        "type": "auth_error",
                        "code": "incompatible_protocol",
                        "supported": sorted(SUPPORTED_PROTOCOLS),
                        "appVersion": APP_VERSION,
                        "message": "This scoreboard does not match the installed "
                        "Valorant Overseer version. Reinstall the app.",
                    },
                )
                await websocket.close(code=CLOSE_PROTOCOL, reason="Incompatible protocol")
                return

            await _safe_send(
                websocket,
                {
                    "type": "auth_ok",
                    "protocol": PROTOCOL_VERSION,
                    "supported": sorted(SUPPORTED_PROTOCOLS),
                    "appVersion": APP_VERSION,
                    "capabilities": CAPABILITIES,
                },
            )
            _CLIENTS.add(websocket)

            try:
                board = await loop.run_in_executor(None, board_provider)
                await _safe_send(websocket, {"type": "state", "data": board})
            except Exception:
                # The client still gets the next broadcast.
                LOG.exception("first board for a new client failed")

            try:
                async for raw in websocket:
                    m = _parse(raw)
                    mtype = m.get("type")
                    if mtype == "pong":
                        continue
                    if mtype == "ping":
                        await _safe_send(websocket, {"type": "pong"})
                        continue
                    if mtype == "request" and request_handler is not None:
                        rtype = str(m.get("request") or "")
                        params = m.get("params") or {}
                        rid = m.get("id")
                        try:
                            data = await loop.run_in_executor(
                                None, functools.partial(request_handler, rtype, params)
                            )
                            await _safe_send(
                                websocket, {"type": "response", "id": rid, "ok": True, "data": data}
                            )
                        except Exception as e:
                            LOG.exception("request %r failed", rtype)
                            await _safe_send(
                                websocket,
                                {"type": "response", "id": rid, "ok": False, "error": str(e)},
                            )
                        continue
            except ConnectionClosed:
                pass
            finally:
                _CLIENTS.discard(websocket)

        async def _broadcast_loop() -> None:
            last = None
            failing = ""
            while True:
                try:
                    board = await loop.run_in_executor(None, board_provider)
                    data_json = json.dumps(board, sort_keys=True, default=str)
                    if data_json != last:
                        last = data_json
                        await _broadcast({"type": "state", "data": board})
                    failing = ""
                except Exception as e:
                    # The loop has to outlive a bad frame. A failure that
                    # repeats every few seconds is logged once, not every time.
                    if repr(e) != failing:
                        failing = repr(e)
                        LOG.exception("board broadcast failed")
                # Sooner when something on the board changed. A moment's wait
                # after the first ask lets the rest of a burst ride along, so
                # ten players' K/D landing together is one board, not ten.
                if await loop.run_in_executor(None, refresh.wait, interval):
                    await asyncio.sleep(_SETTLE)
                refresh.clear()

        async def _heartbeat_loop() -> None:
            while True:
                await asyncio.sleep(30)
                await _broadcast({"type": "ping"})

        async def _main() -> None:
            async with _ws_serve(
                handler,
                "127.0.0.1",
                ws_port,
                process_request=_process_request,
                ping_interval=None,
                max_queue=16,
            ):
                _log(
                    f"listening on ws://127.0.0.1:{ws_port} "
                    f"(protocol {PROTOCOL_VERSION}, local clients only)"
                )
                ready.set()
                await asyncio.gather(_broadcast_loop(), _heartbeat_loop())

        try:
            loop.run_until_complete(_main())
        except Exception as e:
            boot_error.append(e)
            LOG.exception("VG-WS-001 server stopped")
            _log(f"server stopped: {e}")
            ready.set()

    threading.Thread(target=_run, daemon=True, name="overseer-ws").start()

    if not ready.wait(timeout=15) or boot_error:
        reason = str(boot_error[0]) if boot_error else "timed out waiting for the listener"
        msg = f"VG-WS-001 local WebSocket bridge failed to start on 127.0.0.1:{ws_port}: {reason}"
        raise RuntimeError(msg)

    try:
        _self_handshake(ws_port)
        LOG.info("authenticated self-handshake ok on port %s", ws_port)
    except Exception as e:
        msg = f"VG-WS-001 WebSocket self-handshake failed on 127.0.0.1:{ws_port}: {e}"
        raise RuntimeError(msg) from e

    return _STATE.session_token

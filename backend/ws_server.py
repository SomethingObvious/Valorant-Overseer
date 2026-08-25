from __future__ import annotations

import asyncio
import functools
import hmac
import json
import os
import secrets
import threading
from http import HTTPStatus

from common import console_logger
from overseer_commands import ACK_FIELDS

try:
    from websockets.exceptions import ConnectionClosed
    from websockets.legacy.server import serve as _ws_serve
except Exception as e:
    raise RuntimeError(
        "The 'websockets' package is required for local WebSocket mode. "
        "Run install.bat to repair the installation."
    ) from e

from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from collections.abc import Callable

import overseerlog
from vconstants import APP_VERSION

LOG = overseerlog.get_logger("ws", "websocket")

PROTOCOL_VERSION = 1
SUPPORTED_PROTOCOLS = {1}
CAPABILITIES = ["state", "commands", "requests"]

CLOSE_AUTH = 4401
CLOSE_ORIGIN = 4403
CLOSE_PROTOCOL = 4406


# Not quiet-aware: the bridge failing to start is what the user needs to see.
_log = console_logger("ws", quiet_aware=False, also=LOG)


SESSION_TOKEN: str = ""

_CLIENTS: set[Any] = set()
_LOOP: asyncio.AbstractEventLoop | None = None
_READY = threading.Event()
_WS_PORT: int | None = None


async def _process_request(path: str, request_headers: Any) -> Any:
    try:
        get = request_headers.get
    except AttributeError:
        return None

    # Only a browser sends an Origin header, and there is no browser client
    # any more: the web dashboard is deleted. Whatever page is asking, the
    # answer is no.
    origin = get("Origin")
    if origin is not None:
        _log(f"rejected browser origin: {origin}")
        return (
            HTTPStatus.FORBIDDEN,
            [("Content-Type", "text/plain"), ("Content-Length", "16")],
            b"Forbidden origin",
        )

    return None


def is_ready() -> bool:
    return _READY.is_set()


def listening_port() -> int | None:
    return _WS_PORT if _READY.is_set() else None


async def _safe_send(ws: Any, obj: Any) -> bool:
    try:
        await ws.send(json.dumps(obj, default=str))
        return True
    except Exception:
        return False


def _parse(raw: Any) -> dict[str, Any]:
    try:
        data = json.loads(raw)
        return data if isinstance(data, dict) else {}
    except Exception:
        return {}


async def _broadcast(obj: Any) -> None:
    if not _CLIENTS:
        return
    payload = json.dumps(obj, default=str)
    dead = []
    for ws in list(_CLIENTS):
        try:
            await ws.send(payload)
        except Exception:
            dead.append(ws)
    for ws in dead:
        _CLIENTS.discard(ws)


def _self_handshake(ws_port: int, timeout: float = 6.0) -> None:
    from websockets.sync.client import connect as _sync_connect

    with _sync_connect(f"ws://127.0.0.1:{ws_port}", open_timeout=timeout, close_timeout=2) as ws:
        ws.send(json.dumps({"type": "auth", "token": SESSION_TOKEN, "protocol": PROTOCOL_VERSION}))
        reply = json.loads(ws.recv(timeout=timeout))
        if reply.get("type") != "auth_ok":
            raise RuntimeError(f"self-handshake got {reply.get('type')!r}")


def start(
    *,
    board_provider: Callable[[], dict[str, Any]],
    command_router: Any,
    ws_port: int,
    request_handler: Callable[[str, dict[str, Any]], dict[str, Any]] | None = None,
    poll_interval: float | None = None,
) -> str:
    global SESSION_TOKEN, _WS_PORT

    _READY.clear()
    _WS_PORT = None
    SESSION_TOKEN = secrets.token_urlsafe(32)
    interval = float(
        poll_interval if poll_interval is not None else os.getenv("WS_STATE_POLL", "4.0")
    )

    ready = threading.Event()
    boot_error: list[BaseException] = []

    def _run() -> None:
        global _LOOP, _WS_PORT
        loop = asyncio.new_event_loop()
        _LOOP = loop
        asyncio.set_event_loop(loop)

        async def handler(websocket: Any) -> None:

            origin = websocket.request_headers.get("Origin")
            if origin is not None:
                await websocket.close(code=CLOSE_ORIGIN, reason="Forbidden origin")
                return

            client_id = "ws:" + secrets.token_urlsafe(8)

            try:
                raw = await asyncio.wait_for(websocket.recv(), timeout=5.0)
            except TimeoutError:
                await _safe_send(
                    websocket, {"type": "auth_error", "code": "timeout", "message": "Auth timeout"}
                )
                await websocket.close(code=CLOSE_AUTH, reason="Auth timeout")
                return
            except ConnectionClosed:
                return

            msg = _parse(raw)
            if msg.get("type") != "auth" or not hmac.compare_digest(
                str(msg.get("token") or ""), SESSION_TOKEN
            ):
                await _safe_send(
                    websocket,
                    {"type": "auth_error", "code": "bad_token", "message": "Invalid token"},
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
                pass

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
                            await _safe_send(
                                websocket,
                                {"type": "response", "id": rid, "ok": False, "error": str(e)},
                            )
                        continue
                    if mtype == "command":
                        cmd = m.get("command")
                        payload = m.get("payload") or {}
                        cid = m.get("id")
                        result = await loop.run_in_executor(
                            None,
                            functools.partial(
                                command_router.execute,
                                client_id=client_id,
                                command=cmd,
                                payload=payload,
                                command_id=cid,
                            ),
                        )
                        ack = {
                            "type": "command_ack",
                            "id": cid,
                            "ok": bool(result.get("ok")),
                            "message": result.get("message", ""),
                        }
                        for k in ACK_FIELDS:
                            if k in result:
                                ack[k] = result[k]
                        await _safe_send(websocket, ack)
            except ConnectionClosed:
                pass
            finally:
                _CLIENTS.discard(websocket)

        async def _broadcast_loop() -> None:
            last = None
            while True:
                try:
                    board = await loop.run_in_executor(None, board_provider)
                    data_json = json.dumps(board, sort_keys=True, default=str)
                    if data_json != last:
                        last = data_json
                        await _broadcast({"type": "state", "data": board})
                except Exception:
                    pass
                await asyncio.sleep(interval)

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
            LOG.error("VG-WS-001 server stopped: %s", e)
            _log(f"server stopped: {e}")
            ready.set()
        finally:
            _READY.clear()
            _WS_PORT = None

    threading.Thread(target=_run, daemon=True, name="overseer-ws").start()

    if not ready.wait(timeout=15) or boot_error:
        reason = str(boot_error[0]) if boot_error else "timed out waiting for the listener"
        raise RuntimeError(
            f"VG-WS-001 local WebSocket bridge failed to start on 127.0.0.1:{ws_port}: {reason}"
        )

    try:
        _self_handshake(ws_port)
        LOG.info("authenticated self-handshake ok on port %s", ws_port)
    except Exception as e:
        raise RuntimeError(
            f"VG-WS-001 WebSocket self-handshake failed on 127.0.0.1:{ws_port}: {e}"
        ) from e

    _WS_PORT = ws_port
    _READY.set()
    return SESSION_TOKEN

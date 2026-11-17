# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""The bridge the window reads the board through, a WebSocket on 127.0.0.1."""

from __future__ import annotations

import asyncio
import hmac
import json
import secrets
import threading
import time
from concurrent.futures import ThreadPoolExecutor
from http import HTTPStatus
from typing import TYPE_CHECKING, Any

import overseerlog
import refresh
from common import console_logger, env_number
from common.check import check
from vconstants import APP_VERSION

if TYPE_CHECKING:
    from collections.abc import Callable

try:
    from websockets.exceptions import ConnectionClosed
    from websockets.legacy.server import serve as _serve
except ImportError as e:
    msg = "Overseer needs the websockets package. Run install.bat to repair the install."
    raise RuntimeError(msg) from e

LOG = overseerlog.get_logger("ws", "websocket")
_say = console_logger("ws", quiet_aware=False, also=LOG)

SPEAKS = 2

_HELLO_WAIT = 5.0
_SETTLE = 0.4
_PING_EVERY = 30
# How long the board loop may go without finishing a pass before the pings
# stop. A pass is a build and a wait of 15 seconds at most, so this is a
# build that hung. Without pings the window counts the backend as gone and
# starts a new one, where pings alone would keep a frozen board looking live.
_STUCK = 180.0
_UNREAD = frozenset({"riotRequests", "queue", "agents", "teams"})

# Close codes: the window reconnects after the first and gives up on the rest.
_CLOSE_KEY = 4401
_CLOSE_BROWSER = 4403
_CLOSE_SPEAKS = 4406


def _frame(kind: str, **fields: Any) -> str:
    return json.dumps({"kind": kind, **fields}, default=str)


def _read(raw: Any) -> dict[str, Any]:
    try:
        got = json.loads(raw)
    except (TypeError, ValueError, RecursionError) as e:
        LOG.warning("a message that isn't JSON was dropped: %r", e)
        return {}
    return got if isinstance(got, dict) else {}


async def _no_browsers(_path: str, headers: Any) -> Any:
    origin = getattr(headers, "get", lambda _key: None)("Origin")
    if origin is None:
        return None
    _say(f"turned away a browser from {origin}")
    body = b"No browsers"
    return HTTPStatus.FORBIDDEN, [("Content-Length", str(len(body)))], body


class _Bridge:
    def __init__(
        self,
        boards: Callable[[], dict[str, Any]],
        answer: Callable[[str, dict[str, Any]], dict[str, Any]],
        pace: Callable[[float], float],
    ) -> None:
        self.key = secrets.token_urlsafe(32)
        self.watching: set[Any] = set()
        self.boards = boards
        self.answer = answer
        self.pace = pace
        self.every = env_number("WS_STATE_POLL", 4.0, low=0.5, high=60.0)
        self.loop = asyncio.new_event_loop()
        # Boards build on threads of their own. Questions share the default
        # pool, and on a bad connection a few slow profile lookups can fill
        # it, which would freeze the board while the pings still went out.
        self.board_pool = ThreadPoolExecutor(max_workers=2, thread_name_prefix="board")
        self.passed = time.monotonic()

    def stuck(self) -> bool:
        return time.monotonic() - self.passed > _STUCK

    async def tell(self, ws: Any, kind: str, **fields: Any) -> bool:
        try:
            await ws.send(_frame(kind, **fields))
        except ConnectionClosed:
            return False
        except (TypeError, ValueError) as e:
            LOG.warning("a %s message wouldn't encode: %r", kind, e)
            return False
        return True

    async def tell_all(self, kind: str, **fields: Any) -> None:
        text = _frame(kind, **fields)
        for ws in list(self.watching):
            try:
                await ws.send(text)
            except Exception as e:
                if not isinstance(e, ConnectionClosed):
                    LOG.warning("dropped a window whose send failed: %r", e)
                self.watching.discard(ws)

    async def turn_away(self, ws: Any, why: str, say: str, code: int) -> None:
        await self.tell(ws, "refused", why=why, say=say, version=APP_VERSION)
        await ws.close(code=code, reason=why)

    async def hello(self, ws: Any) -> bool:
        try:
            got = _read(await asyncio.wait_for(ws.recv(), timeout=_HELLO_WAIT))
        except TimeoutError:
            await self.turn_away(ws, "late", "No key came. Restart Valorant Overseer.", _CLOSE_KEY)
            return False
        except ConnectionClosed:
            return False
        # Compared as bytes, since compare_digest raises on text outside
        # ASCII, and a wrong key with odd characters is still just wrong.
        sent = str(got.get("key") or "").encode("utf-8", "replace")
        if got.get("kind") != "hello" or not hmac.compare_digest(sent, self.key.encode()):
            await self.turn_away(
                ws, "key", "The bridge didn't know this key. Restart Valorant Overseer.", _CLOSE_KEY
            )
            return False
        if got.get("speaks") != SPEAKS:
            LOG.warning("a window speaking %r was turned away", got.get("speaks"))
            await self.turn_away(
                ws,
                "speaks",
                "This window and the backend are from different versions. Run the setup again.",
                _CLOSE_SPEAKS,
            )
            return False
        return await self.tell(ws, "welcome", speaks=SPEAKS, version=APP_VERSION)

    async def reply(self, ws: Any, asked: dict[str, Any]) -> None:
        what = str(asked.get("what") or "")
        given = asked.get("with")
        try:
            got = await self.loop.run_in_executor(
                None, self.answer, what, given if isinstance(given, dict) else {}
            )
        except Exception as e:
            LOG.exception("the %r question failed", what)
            await self.tell(ws, "reply", n=asked.get("n"), failed=str(e))
            return
        await self.tell(ws, "reply", n=asked.get("n"), answer=got)

    async def serve(self, ws: Any) -> None:
        if ws.request_headers.get("Origin") is not None:
            await ws.close(code=_CLOSE_BROWSER, reason="No browsers")
            return
        if not await self.hello(ws):
            return
        self.watching.add(ws)
        try:
            first = await self.loop.run_in_executor(self.board_pool, self.boards)
            await self.tell(ws, "board", board=first)
        except Exception:
            LOG.exception("a new window's first board failed")
        pending: set[asyncio.Task[None]] = set()
        try:
            async for raw in ws:
                got = _read(raw)
                if got.get("kind") == "ask":
                    task = asyncio.create_task(self.reply(ws, got))
                    pending.add(task)
                    task.add_done_callback(pending.discard)
        except ConnectionClosed:
            pass
        finally:
            self.watching.discard(ws)

    async def send_boards(self) -> None:
        sent = None
        failing = ""
        while True:
            try:
                board = await self.loop.run_in_executor(self.board_pool, self.boards)
                seen = json.dumps(
                    {k: v for k, v in board.items() if k not in _UNREAD},
                    sort_keys=True,
                    default=str,
                )
                if seen != sent:
                    sent = seen
                    await self.tell_all("board", board=board)
                failing = ""
            except Exception as e:
                if repr(e) != failing:
                    failing = repr(e)
                    LOG.exception("sending a board failed")
            self.passed = time.monotonic()
            waited = self.pace(self.every)
            if await self.loop.run_in_executor(self.board_pool, refresh.wait, waited):
                await asyncio.sleep(_SETTLE)
            refresh.clear()

    async def send_pings(self) -> None:
        said = False
        while True:
            await asyncio.sleep(_PING_EVERY)
            if self.stuck():
                if not said:
                    LOG.error("no board in %ss, so the pings stop for a restart", _STUCK)
                    said = True
                continue
            said = False
            await self.tell_all("ping")

    async def main(self, port: int, up: threading.Event) -> None:
        async with _serve(
            self.serve,
            "127.0.0.1",
            port,
            process_request=_no_browsers,
            ping_interval=None,
            max_queue=16,
            # The default 1 MiB is a few hundred brush strokes on one map. Past
            # it the socket closes, and the window sends the same drawing again
            # each time it reconnects.
            max_size=16 * 2**20,
        ):
            _say(f"listening on 127.0.0.1:{port}, speaking version {SPEAKS}, local windows only")
            up.set()
            await asyncio.gather(self.send_boards(), self.send_pings())


def _say_hello(port: int, key: str, wait: float = 6.0) -> None:
    from websockets.sync.client import connect

    with connect(f"ws://127.0.0.1:{port}", open_timeout=wait, close_timeout=2) as ws:
        ws.send(_frame("hello", key=key, speaks=SPEAKS))
        got = _read(ws.recv(timeout=wait))
    if got.get("kind") != "welcome":
        msg = f"it said {got.get('kind')!r} to a hello"
        raise RuntimeError(msg)


def start(
    *,
    board_provider: Callable[[], dict[str, Any]],
    ws_port: int,
    request_handler: Callable[[str, dict[str, Any]], dict[str, Any]],
    pace: Callable[[float], float],
) -> str:
    """Start the bridge on its own thread, and return its key once it answers a hello."""
    bridge = _Bridge(board_provider, request_handler, pace)
    up = threading.Event()
    failed: list[BaseException] = []

    def run() -> None:
        asyncio.set_event_loop(bridge.loop)
        try:
            bridge.loop.run_until_complete(bridge.main(ws_port, up))
        except Exception as e:
            failed.append(e)
            LOG.exception("VG-WS-001 the bridge stopped")
            _say(f"the bridge stopped: {e}")
            up.set()

    threading.Thread(target=run, daemon=True, name="overseer-ws").start()
    where = f"127.0.0.1:{ws_port}"
    if not up.wait(timeout=15) or failed:
        why = str(failed[0]) if failed else "it never started listening"
        msg = f"VG-WS-001 the bridge didn't start on {where}: {why}"
        raise RuntimeError(msg)
    try:
        _say_hello(ws_port, bridge.key)
    except Exception as e:
        msg = f"VG-WS-001 the bridge on {where} didn't answer a hello: {e}"
        raise RuntimeError(msg) from e
    LOG.info("the bridge on %s answered a hello", where)
    return bridge.key


if __name__ == "__main__":
    _bridge = _Bridge(dict, lambda _what, _with: {}, lambda every: every)
    check(not _bridge.stuck(), "a fresh loop isn't stuck")
    _bridge.passed -= _STUCK + 1
    check(_bridge.stuck(), "a loop with no pass in three minutes is")
    print("ws_server self-check OK (pings stop when the board loop hangs)")

from __future__ import annotations

import collections
import threading
import time
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from collections.abc import Callable


ALLOWED_COMMANDS = {
    "check_side",
    "launch_offline",
    "offline_status",
    "offline_toggle",
    "reset_session",
    "start_session",
    "end_session",
    "delete_session",
    "update_match_meta",
}

ACK_FIELDS = (
    "remoteUrl",
    "remoteSessionId",
    "side",
    "map",
    "status",
    "agent",
    "configured",
    "perMap",
    "rateLimited",
    "dedup",
    "queue",
    "queueId",
    "inQueue",
    "running",
    "active",
    "enabled",
    "connected",
    "friendsLoaded",
    "session",
    "sessions",
    "sessionId",
    "meta",
    "matchId",
    "deprecated",
)

RATE_LIMIT = 5
RATE_WINDOW = 10.0

DEDUP_TTL = 120.0
DEDUP_MAX = 256


class CommandRouter:
    def __init__(
        self,
        *,
        riot_client: Any,
        board_provider: Callable[[], dict[str, Any]],
    ) -> None:
        self.riot_client = riot_client

        self.board_provider = board_provider

        self._lock = threading.Lock()

        self._calls: dict[str, collections.deque[float]] = collections.defaultdict(
            collections.deque
        )

        self._seen: dict[str, collections.OrderedDict[str, float]] = collections.defaultdict(
            collections.OrderedDict
        )

    def _rate_ok(self, client_id: str) -> bool:
        now = time.time()
        dq = self._calls[client_id]
        while dq and now - dq[0] > RATE_WINDOW:
            dq.popleft()
        if len(dq) >= RATE_LIMIT:
            return False
        dq.append(now)
        return True

    def _is_duplicate(self, client_id: str, command_id: str | None) -> bool:
        if not command_id:
            return False
        now = time.time()
        seen = self._seen[client_id]

        for k in [k for k, ts in seen.items() if now - ts > DEDUP_TTL]:
            seen.pop(k, None)
        if command_id in seen:
            return True
        seen[command_id] = now
        while len(seen) > DEDUP_MAX:
            seen.popitem(last=False)
        return False

    def execute(
        self,
        *,
        client_id: str,
        command: str,
        payload: dict[str, Any] | None,
        command_id: str | None = None,
    ) -> dict[str, Any]:
        payload = payload if isinstance(payload, dict) else {}

        with self._lock:
            if command not in ALLOWED_COMMANDS:
                return {"ok": False, "message": f"Unknown command '{command}'."}
            if self._is_duplicate(client_id, command_id):
                return {"ok": False, "dedup": True, "message": "Duplicate command ignored."}
            if not self._rate_ok(client_id):
                return {
                    "ok": False,
                    "rateLimited": True,
                    "message": "Rate limit exceeded: max 5 commands / 10s.",
                }

        try:
            if command == "check_side":
                return self._check_side(payload)
            if command == "launch_offline":
                return self._launch_offline(payload)
            if command == "offline_status":
                return self._offline_status(payload)
            if command == "offline_toggle":
                return self._offline_toggle(payload)
            if command == "reset_session":
                return self._reset_session(payload)
            if command == "start_session":
                return self._start_session(payload)
            if command == "end_session":
                return self._end_session(payload)
            if command == "delete_session":
                return self._delete_session(payload)
            if command == "update_match_meta":
                return self._update_match_meta(payload)
        except Exception as e:
            return {"ok": False, "message": f"Command failed: {e}"}
        return {"ok": False, "message": f"Unhandled command '{command}'."}

    def _check_side(self, _payload: dict[str, Any]) -> dict[str, Any]:
        board = self.board_provider() or {}
        side = board.get("side")
        mapn = board.get("map")
        if side:
            return {
                "ok": True,
                "side": side,
                "map": mapn,
                "message": f"You are {side}" + (f" on {mapn}" if mapn else "") + ".",
            }
        return {"ok": True, "side": None, "map": mapn, "message": "Not in agent select / a match."}

    def _launch_offline(self, payload: dict[str, Any]) -> dict[str, Any]:
        import offline_launch

        return offline_launch.launch(payload.get("status"))

    def _offline_status(self, _payload: dict[str, Any]) -> dict[str, Any]:
        import offline_launch

        return {"ok": True, **offline_launch.status()}

    def _offline_toggle(self, payload: dict[str, Any]) -> dict[str, Any]:
        import offline_launch

        if "status" in payload:
            return offline_launch.set_status(str(payload.get("status")))
        return offline_launch.set_enabled(bool(payload.get("enabled", True)))

    def _owner(self) -> str | None:
        return (self.board_provider() or {}).get("selfPuuid")

    def _reset_session(self, payload: dict[str, Any]) -> dict[str, Any]:
        import session_tracker

        return session_tracker.reset(self._owner(), payload.get("goal"))

    def _start_session(self, payload: dict[str, Any]) -> dict[str, Any]:
        import history
        import session_tracker

        owner = self._owner()
        baseline = history.payload(owner).get("summary", {}) if owner else None
        return session_tracker.start(owner, payload.get("goal"), baseline)

    def _end_session(self, _payload: dict[str, Any]) -> dict[str, Any]:
        import session_tracker

        return session_tracker.end(self._owner())

    def _delete_session(self, payload: dict[str, Any]) -> dict[str, Any]:
        import session_tracker

        return session_tracker.delete(self._owner(), str(payload.get("sessionId") or "").strip())

    def _update_match_meta(self, payload: dict[str, Any]) -> dict[str, Any]:
        import match_meta

        return match_meta.update(self._owner(), str(payload.get("matchId") or "").strip(), payload)


if __name__ == "__main__":
    import offline_launch

    surfaced = set(offline_launch.status()) - {"configPort", "chatPort"}
    missing = surfaced - set(ACK_FIELDS)
    assert not missing, f"ACK_FIELDS is missing {sorted(missing)}"

    for cmd in ("launch_offline", "offline_status", "offline_toggle", "reset_session"):
        assert cmd in ALLOWED_COMMANDS, cmd

    print(f"overseer_commands self-check OK ({len(ACK_FIELDS)} ack fields)")

# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""Keep match JSON compressed between reads, bounded by count and payload bytes."""

from __future__ import annotations

import json
import threading
import zlib
from typing import Any

from common.check import check, present


class MatchCache:
    """A thread-safe FIFO of compressed JSON, with fresh objects on each read.

    The byte budget covers retained compressed payloads. Decoded documents,
    temporary compression buffers and the bounded index use additional RAM.
    """

    def __init__(self, *, max_entries: int, max_bytes: int) -> None:
        """Set both retention limits; a nonpositive limit disables retention."""
        self._max_entries = max_entries
        self._max_bytes = max_bytes
        self._entries: dict[str, bytes] = {}
        self._bytes = 0
        self._lock = threading.Lock()

    def __contains__(self, mid: str) -> bool:
        """Check availability without decoding a match."""
        with self._lock:
            return mid in self._entries

    def get(self, mid: str) -> dict[str, Any] | None:
        """Decode a retained match, or return None when it is not here."""
        with self._lock:
            payload = self._entries.get(mid)
        # The immutable snapshot survives eviction. Decoding outside the lock
        # lets the board and career workers read different matches in parallel.
        return json.loads(zlib.decompress(payload)) if payload is not None else None

    def put(self, mid: str, detail: dict[str, Any]) -> None:
        """Keep a snapshot, evicting the oldest entries until both limits fit."""
        if self._max_entries <= 0 or self._max_bytes <= 0:
            return
        payload = zlib.compress(json.dumps(detail, separators=(",", ":")).encode(), level=1)
        with self._lock:
            previous = self._entries.pop(mid, None)
            if previous is not None:
                self._bytes -= len(previous)
            # One unusually large match must not empty the rest of the cache.
            if len(payload) > self._max_bytes:
                return
            while self._entries and (
                len(self._entries) >= self._max_entries
                or self._bytes + len(payload) > self._max_bytes
            ):
                self._bytes -= len(self._entries.pop(next(iter(self._entries))))
            self._entries[mid] = payload
            self._bytes += len(payload)

    def clear(self) -> None:
        """Release all retained payloads and reset their byte count."""
        with self._lock:
            self._entries.clear()
            self._bytes = 0

    def usage(self) -> tuple[int, int]:
        """Return the entry count and retained compressed bytes atomically."""
        with self._lock:
            return len(self._entries), self._bytes


def _self_check() -> None:
    from concurrent.futures import ThreadPoolExecutor
    from unittest import mock

    detail: dict[str, Any] = {"players": [{"subject": "fixture", "name": "雪"}], "optional": None}
    cache = MatchCache(max_entries=2, max_bytes=1024)
    cache.put("a", detail)
    check(cache.get("a") == detail, "JSON round trip")
    detail["players"][0]["name"] = "changed"
    first = present(cache.get("a"))
    check(first["players"][0]["name"] == "雪", "writes keep a snapshot")
    first["players"].clear()
    check(present(cache.get("a"))["players"], "readers cannot mutate the cache")
    cache.put("b", {})
    cache.put("b", detail)
    check("a" in cache and "b" in cache, "replacement does not evict another entry")
    cache.put("c", {})
    check(cache.get("a") is None and "c" in cache, "oldest entry evicted at count limit")
    with mock.patch("zlib.decompress", side_effect=AssertionError("unexpected decode")):
        check("b" in cache and "missing" not in cache, "membership does not decode")

    # Leave room for exactly one of the larger documents, regardless of zlib's
    # version. These checks exercise byte eviction independently of the count.
    cache.clear()
    cache.put("b", detail)
    payload_size = cache.usage()[1]
    bounded = MatchCache(max_entries=20, max_bytes=payload_size)
    bounded.put("a", detail)
    bounded.put("b", detail)
    check("a" not in bounded and bounded.get("b") == detail, "byte budget evicts oldest")
    bounded.put("b", {})
    bounded.put("c", {})
    check("b" in bounded and "c" in bounded, "replacement releases its previous bytes")
    oversized = {"values": list(range(1000))}
    bounded.put("too-big", oversized)
    check("b" in bounded and "c" in bounded and "too-big" not in bounded)
    bounded.put("b", oversized)
    check("b" not in bounded and "c" in bounded, "oversized replacement cannot serve stale data")
    bounded.clear()
    check(bounded.usage() == (0, 0), "clear resets accounting")
    bounded.put("a", detail)
    check(bounded.get("a") == detail, "full budget is available after clear")
    for entries, size in ((0, 100), (10, 0), (-1, 100)):
        disabled = MatchCache(max_entries=entries, max_bytes=size)
        disabled.put("a", detail)
        check("a" not in disabled, "nonpositive limits disable retention")

    concurrent = MatchCache(max_entries=7, max_bytes=payload_size * 3)

    def worker(index: int) -> None:
        key = str(index % 11)
        concurrent.put(key, detail)
        decoded = concurrent.get(key)
        check(decoded is None or decoded == detail, "concurrent read is whole or evicted")
        count, size = concurrent.usage()
        check(count <= 7 and size <= payload_size * 3, "concurrent writes respect both limits")
        check(size == count * payload_size, "concurrent replacement accounts for bytes exactly")

    with ThreadPoolExecutor(max_workers=8) as pool:
        list(pool.map(worker, range(300)))
    print("match_cache self-check OK (round trips, isolation, count/byte limits, concurrency)")


if __name__ == "__main__":
    _self_check()

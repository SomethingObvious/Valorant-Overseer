# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""Talking to the VALORANT client on this PC and to Riot's servers through it."""

from __future__ import annotations

import base64
import contextlib
import heapq
import json
import os
import re
import ssl
import threading
import time
from collections import deque
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import TYPE_CHECKING, Any

import requests
import urllib3
from common import console_logger, data_path, env_number, read_json, write_atomic
from requests.adapters import HTTPAdapter
from urllib3.util.retry import Retry
from urllib3.util.ssl_ import create_urllib3_context
from vconstants import GAMEMODES, map_name_from_path

if TYPE_CHECKING:
    from collections.abc import Callable, Iterator

# The Riot client serves its local API on 127.0.0.1 with a self-signed
# certificate, so the loopback calls pass verify=False. The *.a.pvp.net calls
# carry the entitlements and access tokens and must verify.
urllib3.disable_warnings(urllib3.exceptions.InsecureRequestWarning)


@dataclass
class _State:
    client_version: str | None = None
    party_logged: bool = False
    queue_started: float | None = None


_STATE = _State()

_SESSIONS = threading.local()
# Windows drops a DNS answer for Riot's hosts now and then even on a steady
# line, so a resolve or connect that fails is tried twice more, a second or so
# apart. A slow answer isn't, as that would double an 8 second wait.
# A 429 has to come back as a response, since urllib3 otherwise raises on
# its Retry-After, and the hold and the lockout below never see it.
_RETRY = Retry(
    total=2,
    connect=2,
    read=0,
    status=0,
    other=0,
    backoff_factor=0.5,
    respect_retry_after_header=False,
)
# The client's token, by the lockfile it came through: the headers, whose
# they are and when they were fetched. Every tick makes a new LocalAuth, and
# a token lasts about an hour.
_TOKENS: dict[tuple[str, str], tuple[dict[str, Any], str, float]] = {}
_TOKEN_SECS = 300.0
# The two TLS contexts every session shares, by whether they verify.
_TLS: dict[bool, ssl.SSLContext] = {}
_TLS_LOCK = threading.Lock()


def _tls(*, verified: bool) -> ssl.SSLContext:
    with _TLS_LOCK:
        if verified not in _TLS:
            context = create_urllib3_context(
                cert_reqs=ssl.CERT_REQUIRED if verified else ssl.CERT_NONE
            )
            if verified:
                chosen = os.environ.get("REQUESTS_CA_BUNDLE") or os.environ.get("CURL_CA_BUNDLE")
                roots = Path(chosen or requests.certs.where())
                if roots.is_dir():
                    context.load_verify_locations(capath=str(roots))
                else:
                    context.load_verify_locations(cafile=str(roots))
            _TLS[verified] = context
        return _TLS[verified]


class _OneContext(HTTPAdapter):
    def __init__(self, context: ssl.SSLContext, **kwargs: Any) -> None:
        self.context = context
        super().__init__(**kwargs)

    def init_poolmanager(self, *args: Any, **kwargs: Any) -> None:
        super().init_poolmanager(*args, ssl_context=self.context, **kwargs)

    def proxy_manager_for(self, proxy: str, **kwargs: Any) -> Any:
        return super().proxy_manager_for(proxy, ssl_context=self.context, **kwargs)

    def send(self, request: Any, *args: Any, **kwargs: Any) -> Any:
        kwargs["verify"] = self.context.verify_mode == ssl.CERT_REQUIRED
        return super().send(request, *args, **kwargs)

    def cert_verify(self, conn: Any, url: str, verify: Any, cert: Any) -> None:
        del url, cert
        conn.cert_reqs = "CERT_REQUIRED" if verify else "CERT_NONE"
        conn.ca_certs = conn.ca_cert_dir = None


def _session() -> requests.Session:
    session: requests.Session | None = getattr(_SESSIONS, "session", None)
    if session is None:
        session = requests.Session()
        session.mount("https://", _OneContext(_tls(verified=True), max_retries=_RETRY))
        session.mount("https://127.0.0.1:", _OneContext(_tls(verified=False)))
        _SESSIONS.session = session
    return session


_log = console_logger("riot_client")

# What the calls to the client on this PC pass as their proxies. Windows'
# proxy setting covers 127.0.0.1 unless somebody excepts it, and a proxy on
# another machine then looks for the client there and finds nothing.
_DIRECT = {"no_proxy": "127.0.0.1"}
_SERVER_AUTH = "1.3.6.1.5.5.7.3.1"


def trust_windows_roots() -> None:
    """Have every web request trust the roots Windows trusts, beside the bundled ones."""
    if os.environ.get("REQUESTS_CA_BUNDLE") or os.environ.get("CURL_CA_BUNDLE"):
        return
    path = Path(data_path("roots.pem"))
    tmp = path.with_suffix(".tmp")
    try:
        roots = Path(requests.certs.where()).read_text(encoding="utf-8")
        for der, kind, trust in ssl.enum_certificates("ROOT"):
            for_sites = trust is True or (not isinstance(trust, bool) and _SERVER_AUTH in trust)
            if kind == "x509_asn" and for_sites:
                roots += ssl.DER_cert_to_PEM_cert(der)
        path.parent.mkdir(parents=True, exist_ok=True)
        tmp.write_text(roots, encoding="utf-8")
        ssl.create_default_context(cafile=str(tmp))
        tmp.replace(path)
    except (OSError, ValueError) as e:
        _log(f"Windows' roots couldn't be added ({e!r}), so only the bundled ones are trusted")
        return
    os.environ["REQUESTS_CA_BUNDLE"] = str(path)


_RIOT_RATE_LOCK = threading.Lock()
_RIOT_MAX_RPS = env_number("RIOT_MAX_RPS", 10.0, low=0.0)
_RIOT_BURST = _RIOT_MAX_RPS if _RIOT_MAX_RPS > 0 else 1.0
_RIOT_BUCKET = {"tokens": _RIOT_BURST, "at": 0.0}

_MMR_BURST = 24.0
_MMR_RPS = 0.4
_MMR_BUCKET = {"tokens": _MMR_BURST, "at": 0.0}

# A 429's Retry-After holds back every endpoint in its family until then.
# Throttle times are monotonic, so a clock set back or forward can't stall
# lookups for the size of the jump or let a burst through.
_HOLD_UNTIL = {"mmr": 0.0, "other": 0.0, "glz": 0.0}


def _family(endpoint: str) -> str:
    # The glz server runs the match itself, so a 429 on match history must
    # not hold agent select or the in-game board.
    if endpoint.startswith("glz:"):
        return "glz"
    return "mmr" if endpoint.startswith("/mmr/") else "other"


def held_secs(endpoint: str) -> float:
    """Return how long Riot asked calls like `endpoint` to wait, in seconds."""
    with _RIOT_RATE_LOCK:
        return max(0.0, _HOLD_UNTIL[_family(endpoint)] - time.monotonic())


def mmr_spare() -> int:
    """Return how many rank lookups could go now without waiting."""
    with _RIOT_RATE_LOCK:
        now = time.monotonic()
        since = now - (_MMR_BUCKET["at"] or now)
        return int(min(_MMR_BURST, _MMR_BUCKET["tokens"] + since * _MMR_RPS))


def _set_hold(endpoint: str, seconds: float) -> None:
    fam = _family(endpoint)
    with _RIOT_RATE_LOCK:
        _HOLD_UNTIL[fam] = max(_HOLD_UNTIL[fam], time.monotonic() + seconds)


def _take_token(bucket: dict[str, Any], burst: float, rps: float) -> None:
    while True:
        with _RIOT_RATE_LOCK:
            now = time.monotonic()
            since = now - (bucket["at"] or now)
            bucket["at"] = now
            bucket["tokens"] = min(burst, bucket["tokens"] + since * rps)
            short = 1.0 - bucket["tokens"]
            if short <= 0:
                bucket["tokens"] -= 1.0
                return
        time.sleep(short / rps)


# Riot lets about 60 match and history lookups through in any minute, then
# locks them all out for a minute (measured October 2026: 60 to 66 at any
# speed, then Retry-After 60). Staying under that never costs the lockout.
# The limit is learned. A lockout lowers it a little under what got through
# and saves it. Below where it starts, each minute held at it without one
# raises it by one, and above that it takes ten, since probing near the real
# limit costs a lockout and a lockout costs a whole minute.
_BUDGET_FILE = data_path("riot_budget.json")
_BUDGET_MAX = 60
_BUDGET_MIN = 10
_BUDGET_START = 55
_BUDGET_CLEAN = 10
_BUDGET: dict[str, Any] = {"limit": _BUDGET_START, "window": 60.0}
_BUDGET_SEEN: dict[str, Any] = {
    "loaded": False,
    "clean": 0,
    "counted_at": 0.0,
    "waiting": 0,
    "urgent": 0,
    "reserve": 5,
    "locked_at": 0.0,
}
_SENT: deque[float] = deque()


def _budget_load() -> None:
    if _BUDGET_SEEN["loaded"]:
        return
    _BUDGET_SEEN["loaded"] = True
    kept = read_json(_BUDGET_FILE, {})
    try:
        limit = int(kept.get("limit", _BUDGET["limit"]))
        window = float(kept.get("window", _BUDGET["window"]))
    except (AttributeError, TypeError, ValueError):
        return
    _BUDGET["limit"] = min(_BUDGET_MAX, max(_BUDGET_MIN, limit))
    _BUDGET["window"] = min(300.0, max(60.0, window))


_FIRST_READ = 6


def keep_for_first_reads(players: int) -> None:
    """Hold room in Riot's minute back from deeper reads, for `players` yet to turn up."""
    with _RIOT_RATE_LOCK:
        _BUDGET_SEEN["reserve"] = max(0, players)


def _budget_room(now: float, *, background: bool, urgent: bool = False) -> float:
    # Seconds until a lookup can go out, under the lock. A deeper read leaves
    # room for five players' first reads while more can still arrive, and
    # stands aside for one that is waiting, so a new player's first five never
    # queue behind it.
    _budget_load()
    window = float(_BUDGET["window"])
    while _SENT and now - _SENT[0] >= window:
        _SENT.popleft()
    limit = int(_BUDGET["limit"])
    if background and _BUDGET_SEEN["waiting"]:
        return 1.0
    if not urgent and _BUDGET_SEEN["urgent"]:
        return 1.0
    held = min(_FIRST_READ * _BUDGET_SEEN["reserve"], limit // 2) if background else 0
    cap = limit - held
    over = len(_SENT) - cap
    if over < 0:
        return 0.0
    if len(_SENT) >= limit and now - _BUDGET_SEEN["counted_at"] >= window:
        # A minute held at the limit with no lockout says it may be low.
        _BUDGET_SEEN["counted_at"] = now
        _BUDGET_SEEN["clean"] += 1
        need = 1 if limit < _BUDGET_START else _BUDGET_CLEAN
        if _BUDGET_SEEN["clean"] >= need and limit < _BUDGET_MAX:
            _BUDGET.update(limit=limit + 1)
            _BUDGET_SEEN["clean"] = 0
            write_atomic(_BUDGET_FILE, _BUDGET, prefix="riot_budget")
    return max(0.05, _SENT[over] + window - now)


def _budget_take(*, background: bool, urgent: bool = False) -> float:
    # Takes a lookup's place in the minute and returns 0, or says how long
    # until there is one.
    with _RIOT_RATE_LOCK:
        now = time.monotonic()
        wait = _budget_room(now, background=background, urgent=urgent)
        if wait <= 0:
            _SENT.append(now)
        return wait


def budget_wait() -> float:
    """Return how many seconds until a deeper read could go out, 0 if one can now."""
    with _RIOT_RATE_LOCK:
        room = _budget_room(time.monotonic(), background=True)
    return max(room, held_secs("/match-details/"))


def budget_eta(lookups: int) -> float:
    """Return about how many seconds `lookups` deeper reads take at the pace Riot allows."""
    with _RIOT_RATE_LOCK:
        _budget_load()
        now = time.monotonic()
        window = float(_BUDGET["window"])
        while _SENT and now - _SENT[0] >= window:
            _SENT.popleft()
        limit = int(_BUDGET["limit"])
        cap = max(1, limit - min(_FIRST_READ * _BUDGET_SEEN["reserve"], limit // 2))
        sent = list(_SENT)
    over = max(0, len(sent) - cap)
    slots = [now] * max(0, cap - len(sent)) + [at + window for at in sent[over:]]
    heapq.heapify(slots)
    last = now
    for _ in range(lookups):
        last = heapq.heappop(slots)
        heapq.heappush(slots, last + window)
    return last - now + 1.0 + held_secs("/match-details/")


def _budget_lockout(retry_after: float) -> None:
    # Riot counted more than this side did, so the limit drops to a little
    # under what got through, by 2 at least and 8 at most, and is kept for
    # next time. The game itself spends the same allowance, at a match's end
    # and on its career pages, so what this side counted can be well short of
    # Riot's limit. Under half of it says the game spent the minute, and only
    # the hold Riot asked for applies, since a cut on that takes hours to
    # climb back. The lookups in flight all come back 429 together, and that
    # is one lockout.
    with _RIOT_RATE_LOCK:
        _budget_load()
        now = time.monotonic()
        if now - _BUDGET_SEEN["locked_at"] < _BUDGET["window"]:
            return
        _BUDGET_SEEN["locked_at"] = now
        limit = int(_BUDGET["limit"])
        got = len(_SENT)
        if got < limit // 2:
            _log(f"held back after {got} lookups in a minute, the game spent the rest")
            return
        new = max(_BUDGET_MIN, min(limit, max(got, limit - 6)) - 2)
        window = min(300.0, max(float(_BUDGET["window"]), retry_after))
        _BUDGET.update(limit=new, window=window)
        _BUDGET_SEEN["clean"] = 0
        write_atomic(_BUDGET_FILE, _BUDGET, prefix="riot_budget")
    _log(f"locked out after {got} lookups in a minute, keeping to {new} from now on")


def _wait_out_hold(endpoint: str) -> None:
    while (wait := held_secs(endpoint)) > 0:
        time.sleep(wait)


@contextlib.contextmanager
def _queued(*, urgent: bool) -> Iterator[None]:
    with _RIOT_RATE_LOCK:
        _BUDGET_SEEN["waiting"] += 1
        _BUDGET_SEEN["urgent"] += urgent
    try:
        yield
    finally:
        with _RIOT_RATE_LOCK:
            _BUDGET_SEEN["waiting"] -= 1
            _BUDGET_SEEN["urgent"] -= urgent


def _riot_throttle(endpoint: str, *, budget: bool = False, urgent: bool = False) -> None:
    if _RIOT_MAX_RPS <= 0:
        return
    _wait_out_hold(endpoint)
    if budget:
        while (wait := _budget_take(background=False, urgent=urgent)) > 0:
            with _queued(urgent=urgent):
                time.sleep(wait)
        # Riot can ask for a pause while this waited on the budget, and a
        # call sent into it only makes the pause longer.
        _wait_out_hold(endpoint)
    if _family(endpoint) == "mmr":
        _take_token(_MMR_BUCKET, _MMR_BURST, _MMR_RPS)
    _take_token(_RIOT_BUCKET, _RIOT_BURST, _RIOT_MAX_RPS)


def _retry_after(resp: requests.Response) -> float:
    # Capped like the learned window, since a hold this long stops every
    # lookup, and an "inf" would make time.sleep raise.
    try:
        return min(300.0, max(0.0, float(resp.headers.get("Retry-After") or 0)))
    except (TypeError, ValueError):
        return 0.0


class ClientNotReadyError(Exception):
    """The Riot client isn't answering, or has no entitlements to hand out yet."""


class GameNotStartedError(Exception):
    """VALORANT hasn't got far enough into starting to say which region it plays in."""


# What every call that Riot turned away, or never sent, answers with.
_THROTTLED = {"errorCode": "RATE_LIMITED", "status": 429}


def _decoded(private: Any) -> dict[str, Any] | None:
    try:
        got = json.loads(base64.b64decode(str(private)))
    except ValueError:
        return None
    return got if isinstance(got, dict) else None


def _lockfile_path() -> Path:
    return Path(os.getenv("LOCALAPPDATA", "")) / r"Riot Games\Riot Client\Config\lockfile"


def _shooter_log_path() -> Path:
    return Path(os.getenv("LOCALAPPDATA", "")) / r"VALORANT\Saved\Logs\ShooterGame.log"


# The region's servers as the log names them. Every call to them carries the
# account's tokens, so only a plain label under a.pvp.net is ever taken from
# the log, and a line that reads any other way names no server.
_PD_HOST = re.compile(r"([a-z0-9-]+)\.a\.pvp\.net/account-xp/v1/", re.IGNORECASE)
_GLZ_HOST = re.compile(r"\bglz-([a-z0-9-]+)\.([a-z0-9-]+)\.a\.pvp\.net/", re.IGNORECASE)
# The region last read from the log, and how that log began. Every board
# makes a LocalAuth, and finding the region again is hundreds of lines put
# through two patterns, which measured as most of an idle board's work. The
# log opens with the time VALORANT started, so a new launch is read anew.
_REGION: dict[str, Any] = {"began": b"", "region": None}


class LocalAuth:
    """The VALORANT client on this PC, and Riot's servers reached with its tokens."""

    def __init__(self) -> None:
        """Read the lockfile, and the region from the client's log."""
        self.lockfile = self._get_lockfile()
        shard, glz = self._get_region()
        self.pd_url = f"https://pd.{shard}.a.pvp.net"
        self.glz_url = f"https://glz-{glz[0]}.{glz[1]}.a.pvp.net"
        self.shard = shard
        self.kept_headers: dict[str, Any] | None = None
        self.puuid = ""
        self.req_count = 0

    @staticmethod
    def available() -> bool:
        """Return whether the Riot client is running, by whether its lockfile is there."""
        return _lockfile_path().is_file()

    def _get_lockfile(self) -> dict[str, Any]:
        keys = ["name", "PID", "port", "password", "protocol"]
        try:
            text = _lockfile_path().read_text(encoding="utf-8")
            return dict(zip(keys, text.split(":"), strict=True))
        except (PermissionError, ValueError) as e:
            msg = "the Riot client is still starting"
            raise ClientNotReadyError(msg) from e

    def _get_region(self) -> list[Any]:
        pd_url = glz_url = None
        if not _shooter_log_path().is_file():
            msg = "VALORANT hasn't written ShooterGame.log yet"
            raise GameNotStartedError(msg)
        with _shooter_log_path().open("rb") as head:
            began = head.read(64)
        if began and began == _REGION["began"]:
            return list(_REGION["region"])
        with _shooter_log_path().open(encoding="utf8", errors="replace") as f:
            for line in f:
                if found := _PD_HOST.search(line):
                    pd_url = found[1]
                elif found := _GLZ_HOST.search(line):
                    glz_url = [found[1], found[2]]
                if pd_url and glz_url:
                    region = ["na", ["na-1", "na"]] if pd_url == "pbe" else [pd_url, glz_url]
                    _REGION.update(began=began, region=region)
                    return list(region)
        msg = "could not parse region from ShooterGame.log"
        raise GameNotStartedError(msg)

    def _local_headers(self) -> dict[str, str]:
        token = base64.b64encode(("riot:" + self.lockfile["password"]).encode()).decode()
        return {"Authorization": f"Basic {token}"}

    def client_version(self) -> str:
        """Return the client's version, from its presence, valorant-api or its log."""
        if not _STATE.client_version:
            for where, find in (
                ("local presence", self._version_from_presence),
                ("valorant-api", _version_from_api),
                ("ShooterGame.log", _version_from_log),
            ):
                if found := find():
                    _STATE.client_version = found
                    _log(f"client version ({where}): {found}")
                    break
        return _STATE.client_version or "release-09.00"

    def _version_from_presence(self) -> str | None:
        try:
            listed = (self.local_get("/chat/v4/presences") or {}).get("presences") or []
        except (requests.RequestException, ClientNotReadyError, ValueError, AttributeError) as e:
            _log(f"no version from the local presences ({e!r})")
            return None
        for presence in listed:
            if not isinstance(presence, dict) or presence.get("product") != "valorant":
                continue
            private = _decoded(presence.get("private")) or {}
            party = private.get("partyPresenceData") or {}
            if found := party.get("partyClientVersion") or private.get("partyClientVersion"):
                return str(found)
        return None

    def headers(self, *, refresh: bool = False) -> dict[str, Any]:
        """Return the headers Riot's servers want, with this account's tokens."""
        if self.kept_headers and not refresh:
            return self.kept_headers
        key = (str(self.lockfile.get("port")), str(self.lockfile.get("password")))
        kept = _TOKENS.get(key)
        if kept and not refresh and 0 <= time.time() - kept[2] < _TOKEN_SECS:
            self.kept_headers, self.puuid = kept[0], kept[1]
            return self.kept_headers
        resp = self._from_client("/entitlements/v1/token")
        try:
            ent = resp.json()
        except ValueError:
            ent = None
        if not isinstance(ent, dict) or not all(
            k in ent for k in ("subject", "accessToken", "token")
        ):
            msg = f"entitlements not ready (HTTP {resp.status_code})"
            raise ClientNotReadyError(msg)
        self.puuid = ent["subject"]
        self.kept_headers = {
            "Authorization": f"Bearer {ent['accessToken']}",
            "X-Riot-Entitlements-JWT": ent["token"],
            "X-Riot-ClientPlatform": (
                "ew0KCSJwbGF0Zm9ybVR5cGUiOiAiUEMiLA0KCSJwbGF0Zm9ybU9TIjog"
                "IldpbmRvd3MiLA0KCSJwbGF0Zm9ybU9TVmVyc2lvbiI6ICIxMC4wLjE5"
                "MDQyLjEuMjU2LjY0Yml0IiwNCgkicGxhdGZvcm1DaGlwc2V0IjogIlVua25vd24iDQp9"
            ),
            "X-Riot-ClientVersion": self.client_version(),
            "User-Agent": "ShooterGame/13 Windows/10.0.19043.1.256.64bit",
        }
        _TOKENS[key] = (self.kept_headers, self.puuid, time.time())
        return self.kept_headers

    @staticmethod
    def _json(resp: requests.Response) -> dict[str, Any]:
        try:
            return resp.json()
        except ValueError:
            # A 429 usually comes with no body at all.
            return dict(_THROTTLED) if resp.status_code == 429 else {}

    def glz_get(self, endpoint: str) -> dict[str, Any]:
        """Return a GET from the glz server, the one that runs matches and parties."""
        name = f"glz:{endpoint}"
        if held_secs(name) > 0:
            return dict(_THROTTLED)
        _riot_throttle(name)
        self.req_count += 1
        resp = _session().get(self.glz_url + endpoint, headers=self.headers(), timeout=8)
        if resp.status_code != 429:
            return self._json(resp)
        _set_hold(name, _retry_after(resp) or 3.0)
        return dict(_THROTTLED)

    def pd_get(
        self,
        endpoint: str,
        *,
        retries: int = 0,
        background: bool = False,
        urgent: bool = False,
    ) -> dict[str, Any]:
        """Return a GET from the pd server, retrying a 429 `retries` times with backoff."""
        budgeted = _family(endpoint) == "other"
        pause = 3.0
        for _ in range(retries + 1):
            if not background:
                _riot_throttle(endpoint, budget=budgeted, urgent=urgent)
            elif held_secs(endpoint) > 0 or (budgeted and _budget_take(background=True) > 0):
                return dict(_THROTTLED)
            else:
                _riot_throttle(endpoint)
            self.req_count += 1
            resp = _session().get(self.pd_url + endpoint, headers=self.headers(), timeout=8)
            if resp.status_code != 429:
                return self._json(resp)
            asked = _retry_after(resp) or pause
            _set_hold(endpoint, asked)
            if budgeted:
                _budget_lockout(asked)
            pause += 3.0
        return dict(_THROTTLED)

    def pd_put(self, endpoint: str, payload: Any, *, refresh: bool = False) -> dict[str, Any]:
        """Return a PUT to the pd server."""
        _riot_throttle(endpoint)
        self.req_count += 1
        return self._json(
            _session().put(
                self.pd_url + endpoint,
                headers=self.headers(refresh=refresh),
                json=payload,
                timeout=8,
            )
        )

    def _from_client(self, endpoint: str) -> requests.Response:
        try:
            return _session().get(
                f"https://127.0.0.1:{self.lockfile['port']}{endpoint}",
                headers=self._local_headers(),
                verify=False,
                timeout=5,
                proxies=_DIRECT,
            )
        except requests.ConnectionError as e:
            # The lockfile outlives a client that crashed or was ended from
            # Task Manager, and nothing listens on the port it names.
            msg = "the Riot client isn't answering"
            raise ClientNotReadyError(msg) from e

    def local_get(self, endpoint: str) -> dict[str, Any]:
        """Return a GET from the client's own API on 127.0.0.1."""
        return self._from_client(endpoint).json()


def _offline_presence_private() -> dict[str, Any] | None:
    try:
        from offline_launch import captured_presence_private

        return captured_presence_private()
    except Exception as e:
        _log(f"offline presence lookup failed: {e!r}")
        return None


def chat_presences(auth: LocalAuth) -> list[dict[str, Any]]:
    """Return everyone's chat presences, with yours as offline mode last saw it."""
    data = auth.local_get("/chat/v4/presences")
    presences = [dict(p) for p in ((data or {}).get("presences", []) or []) if isinstance(p, dict)]
    private = _offline_presence_private()
    if not private:
        return presences

    encoded = base64.b64encode(json.dumps(private, separators=(",", ":")).encode("utf-8")).decode(
        "ascii"
    )
    mine = next(
        (
            i
            for i, p in enumerate(presences)
            if p.get("puuid") == auth.puuid and p.get("product") in (None, "valorant")
        ),
        None,
    )
    if mine is None:
        presences.insert(0, {"puuid": auth.puuid, "product": "valorant", "private": encoded})
    else:
        presence = presences.pop(mine)
        presence["product"] = "valorant"
        presence["private"] = encoded
        presences.insert(0, presence)
    return presences


def _iso_to_epoch(s: str | None) -> float | None:
    try:
        # 3.11+ fromisoformat parses the trailing Z itself.
        dt = datetime.fromisoformat(s or "")
        return dt.timestamp() if dt.year >= 2000 else None
    except (ValueError, TypeError, OverflowError, OSError):
        return None


def _self_presence_private(auth: LocalAuth) -> dict[str, Any] | None:
    try:
        presences = chat_presences(auth)
    except (
        requests.RequestException,
        ClientNotReadyError,
        ValueError,
        AttributeError,
        TypeError,
    ) as e:
        _log(f"presence read failed: {e!r}")
        return None
    own = next(
        (
            p
            for p in presences
            if p.get("puuid") == auth.puuid
            and p.get("private")
            and p.get("product") in (None, "valorant")
        ),
        None,
    )
    return None if own is None else _decoded(own["private"])


def _label(queue: str) -> str:
    return GAMEMODES.get(queue, queue.replace("_", " ").title())


def _from_glz_party(snap: dict[str, Any], party: dict[str, Any], puuid: str) -> None:
    if not _STATE.party_logged:
        _STATE.party_logged = True
        _log(f"party payload keys: {sorted(party)}")
    members = party.get("Members") or []
    if queue := ((party.get("MatchmakingData") or {}).get("QueueID") or "").lower():
        snap.update(queueId=queue, queueName=_label(queue))
    if party.get("State"):
        snap.update(state=party["State"], inQueue="MATCHMAKING" in party["State"])
    snap["eligible"] = [{"id": q, "name": _label(q)} for q in party.get("EligibleQueues") or []]
    snap["queuedAt"] = _iso_to_epoch(party.get("QueueEntryTime"))
    snap["partySize"] = len(members)
    mine: dict[str, Any] = next((m for m in members if m.get("Subject") == puuid), {})
    if "IsOwner" in mine:
        snap["isOwner"] = bool(mine["IsOwner"])
    snap["allReady"] = all(bool(m.get("IsReady", True)) for m in members)
    custom = (party.get("CustomGameData") or {}).get("Settings") or {}
    if "CUSTOM_GAME" in str(snap["state"]) and custom.get("Map"):
        snap["map"] = map_name_from_path(custom["Map"])


def _time_queue(snap: dict[str, Any]) -> None:
    if not snap["inQueue"]:
        _STATE.queue_started = snap["queuedAt"] = None
        return
    now = time.time()
    told = snap["queuedAt"]
    if told and told <= now:
        _STATE.queue_started = told
    elif _STATE.queue_started is None:
        _STATE.queue_started = now
    snap["queuedAt"] = _STATE.queue_started
    snap["queueElapsed"] = max(0, round(now - _STATE.queue_started, 1))


def party_snapshot(auth: LocalAuth) -> dict[str, Any]:
    """Return your party and its queue: who is in it, its state, and the queue time."""
    own = _self_presence_private(auth) or {}
    party_data = own.get("partyPresenceData") or {}
    party_id = party_data.get("partyId") or own.get("partyId")
    if not party_id:
        return {"available": False}
    state = party_data.get("partyState") or "DEFAULT"
    queue = (
        own.get("queueId") or (own.get("matchPresenceData") or {}).get("queueId") or ""
    ).lower()
    snap = {
        "available": True,
        "partyId": party_id,
        "queueId": queue or None,
        "queueName": _label(queue) if queue else None,
        "eligible": [],
        "state": state,
        "inQueue": "MATCHMAKING" in state,
        "queuedAt": None,
        "partySize": party_data.get("partySize") or own.get("partySize") or 1,
        "isOwner": bool(party_data.get("isPartyOwner", True)),
        "allReady": True,
    }
    party = auth.glz_get(f"/parties/v1/parties/{party_id}")
    if isinstance(party, dict) and party.get("Members"):
        _from_glz_party(snap, party, auth.puuid)
    elif isinstance(party, dict) and party.get("status") == 429:
        snap["throttled"] = True
    _time_queue(snap)
    return snap


def _version_from_api() -> str | None:
    try:
        got = requests.get("https://valorant-api.com/v1/version", timeout=6).json()
        return (got.get("data") or {}).get("riotClientVersion") or None
    except (requests.RequestException, ValueError, AttributeError) as e:
        _log(f"no version from valorant-api ({e!r})")
        return None


def _version_from_log() -> str | None:
    marker = "CI server version:"
    try:
        with _shooter_log_path().open(encoding="utf8", errors="replace") as log:
            line = next((line for line in log if marker in line), "")
    except (OSError, ValueError) as e:
        _log(f"no version from ShooterGame.log ({e!r})")
        return None
    return line.partition(marker)[2].strip() or None


class RiotClient:
    """What the environment says: the Riot API key and where data comes from."""

    def __init__(self) -> None:
        """Read RIOT_API_KEY and DATA_SOURCE from the environment."""
        self.api_key = os.getenv("RIOT_API_KEY", "").strip()
        self.source_pref = os.getenv("DATA_SOURCE", "auto").strip().lower()


def _self_check() -> None:
    import tempfile
    from unittest import mock

    from common.check import check

    def not_ready(ask: Callable[[], object]) -> bool:
        try:
            ask()
        except ClientNotReadyError:
            return True
        return False

    with (
        tempfile.TemporaryDirectory() as tmp,
        mock.patch(f"{__name__}._BUDGET_FILE", f"{tmp}/b.json"),
    ):
        _BUDGET.update(limit=20, window=60.0)
        _BUDGET_SEEN.update(loaded=True, clean=0, counted_at=0.0, waiting=0, locked_at=0.0)
        now = time.monotonic()
        _SENT.clear()
        _SENT.extend([now - 50] * 5 + [now - 1] * 5)
        check(_budget_take(background=True) > 0)
        check(_budget_take(background=False) == 0)
        check(9 < budget_wait() < 11, budget_wait())
        keep_for_first_reads(0)
        check(budget_wait() == 0)
        keep_for_first_reads(5)
        _SENT.clear()
        _BUDGET_SEEN["waiting"] = 1
        check(_budget_take(background=True) > 0)
        _BUDGET_SEEN["waiting"] = 0
        _BUDGET_SEEN["urgent"] = 1
        check(_budget_take(background=False) > 0)
        check(_budget_take(background=False, urgent=True) == 0)
        _BUDGET_SEEN["urgent"] = 0
        _SENT.clear()
        # A lockout after 18 of 20 lowers the limit to 16 and keeps it, one
        # after 12 of 20 lowers it by 8 at most, and one after only 5 of 30
        # leaves it, as the game spent that minute.
        _SENT.extend([now] * 18)
        _budget_lockout(60.0)
        _budget_lockout(60.0)
        check(_BUDGET["limit"] == 16, _BUDGET)
        check(read_json(f"{tmp}/b.json", {}).get("limit") == 16)
        _BUDGET_SEEN["loaded"] = False
        _BUDGET.update(limit=50)
        _budget_load()
        check(_BUDGET["limit"] == 16, "a saved limit wasn't read back")
        _SENT.clear()
        _SENT.extend([now] * 15)
        _BUDGET_SEEN["locked_at"] = 0.0
        _BUDGET.update(limit=30)
        _budget_lockout(60.0)
        check(_BUDGET["limit"] == 22, _BUDGET)
        _SENT.clear()
        _SENT.extend([now] * 5)
        _BUDGET_SEEN["locked_at"] = 0.0
        _budget_lockout(60.0)
        check(_BUDGET["limit"] == 22, "the game's minute cut the limit")
        # Under where it started, a minute held at it without a lockout
        # raises it by one. Above, it takes ten.
        _SENT.clear()
        _SENT.extend([now] * 22)
        _BUDGET_SEEN.update(clean=0, counted_at=0.0)
        check(_budget_take(background=False) > 0)
        check(_BUDGET["limit"] == 23, _BUDGET)
        _BUDGET.update(limit=_BUDGET_START)
        _SENT.clear()
        _SENT.extend([now] * _BUDGET_START)
        _BUDGET_SEEN.update(clean=0, counted_at=0.0)
        check(_budget_take(background=False) > 0)
        check(_BUDGET["limit"] == _BUDGET_START, _BUDGET)
        _BUDGET.update(limit=14)
        _SENT.clear()
        keep_for_first_reads(0)
        check(budget_eta(10) < 2, budget_eta(10))
        _SENT.extend([now - 30] * 14)
        check(29 < budget_eta(5) < 32, budget_eta(5))
        check(89 < budget_eta(20) < 92, budget_eta(20))
        keep_for_first_reads(5)
        _SENT.clear()
    # The region is read from the log's own lines, and a line that would point
    # the account's tokens at another host names no server at all.
    with tempfile.TemporaryDirectory() as tmp:
        log, lockfile = Path(tmp) / "ShooterGame.log", Path(tmp) / "lockfile"
        lockfile.write_text("Riot Client:1:2:pw:https", encoding="utf-8")
        with (
            mock.patch(f"{__name__}._shooter_log_path", return_value=log),
            mock.patch(f"{__name__}._lockfile_path", return_value=lockfile),
        ):
            log.write_text(
                "LogHttp: server glz-elsewhere.example/x.y/z\n"
                "LogHttp: GET https://pd.eu.a.pvp.net/account-xp/v1/players/me\n",
                encoding="utf8",
            )
            try:
                taken: str | None = LocalAuth().glz_url
            except GameNotStartedError:
                taken = None
            check(taken is None, taken)
            with log.open("ab") as more:
                more.write(b"LogInit: \xff\xfe\n")
                more.write(b"LogHttp: GET https://glz-eu-1.eu.a.pvp.net/session/v1/sessions/me\n")
            auth = LocalAuth()
            with mock.patch(f"{__name__}._PD_HOST") as searched:
                check(LocalAuth().shard == "eu" and not searched.search.called, "read again")
            log.write_text(
                "Log file open, the next launch\n"
                "LogHttp: GET https://pd.ap.a.pvp.net/account-xp/v1/players/me\n"
                "LogHttp: GET https://glz-ap-1.ap.a.pvp.net/session/v1/sessions/me\n",
                encoding="utf8",
            )
            check(LocalAuth().glz_url == "https://glz-ap-1.ap.a.pvp.net", "a stale region")
            # A lockfile caught half written is a client still starting.
            lockfile.write_text("Riot Client:1", encoding="utf-8")
            check(not_ready(LocalAuth), "half a lockfile was read as a whole one")
    check(auth.pd_url == "https://pd.eu.a.pvp.net", auth.pd_url)
    check(auth.glz_url == "https://glz-eu-1.eu.a.pvp.net", auth.glz_url)
    # The client on this PC is asked directly, whatever proxy Windows has set.
    with mock.patch(f"{__name__}._session") as session:
        auth.local_get("/x")
        check(session.return_value.get.call_args.kwargs.get("proxies") == _DIRECT)
        # A lockfile left by a client that is gone reads as no client, both
        # for the tokens and for everything asked after them.
        session.return_value.get.side_effect = requests.ConnectionError("refused")
        check(not_ready(auth.headers), "a client that is gone read as a failure")
        check(not_ready(lambda: auth.local_get("/x")), "and so did what was asked after")
    with (
        tempfile.TemporaryDirectory() as tmp,
        mock.patch.dict(os.environ, {"REQUESTS_CA_BUNDLE": "theirs.pem"}),
        mock.patch(f"{__name__}.data_path", return_value=str(Path(tmp) / "roots.pem")),
    ):
        os.environ.pop("CURL_CA_BUNDLE", None)
        trust_windows_roots()
        check(os.environ["REQUESTS_CA_BUNDLE"] == "theirs.pem", "a chosen bundle was replaced")
        del os.environ["REQUESTS_CA_BUNDLE"]
        trust_windows_roots()
        made = Path(os.environ.get("REQUESTS_CA_BUNDLE", ""))
        bundled = Path(requests.certs.where()).read_text(encoding="utf-8")
        trusted = made.read_text(encoding="utf-8") if made.name == "roots.pem" else ""
        check(trusted.startswith(bundled) and len(trusted) > len(bundled), made)
    # A 429 from the match server holds its calls back for as long as it asks,
    # answered throttled without being sent, and holds nothing else.
    auth.kept_headers = {"Authorization": "Bearer t"}
    busy = mock.Mock(status_code=429, headers={"Retry-After": "30"})
    with mock.patch(f"{__name__}._session") as session:
        session.return_value.get.return_value = busy
        check(auth.glz_get("/parties/v1/parties/p").get("status") == 429)
        check(auth.glz_get("/parties/v1/parties/p").get("status") == 429)
        check(session.return_value.get.call_count == 1, session.return_value.get.call_count)
    check(29 < held_secs("glz:/x") <= 30 and held_secs("/match-details/") == 0, _HOLD_UNTIL)
    for said, held in (("inf", 300.0), ("99999", 300.0), ("nan", 0.0), ("soon", 0.0)):
        check(_retry_after(mock.Mock(headers={"Retry-After": said})) == held, said)
    _HOLD_UNTIL["glz"] = 0.0
    # A failed resolve is tried again on Riot's hosts, never on the client's own.
    riot = _session().get_adapter("https://pd.na.a.pvp.net/")
    local = _session().get_adapter("https://127.0.0.1:1234/")
    check(isinstance(riot, _OneContext) and riot.max_retries.connect == 2, riot)
    check(isinstance(local, _OneContext) and not local.max_retries.total, local)
    # Riot's servers are verified by name against one set of roots, shared by
    # every thread, and only the client on this PC goes unverified.
    check(riot.context.verify_mode == ssl.CERT_REQUIRED and riot.context.check_hostname)
    check(riot.poolmanager.connection_pool_kw["ssl_context"] is riot.context)
    check(local.context.verify_mode == ssl.CERT_NONE and local.context is not riot.context)
    others: list[Any] = []
    asked = threading.Thread(target=lambda: others.append(_session().get_adapter(auth.pd_url)))
    asked.start()
    asked.join()
    check(others[0] is not riot and others[0].context is riot.context, "one context per thread")
    # A 429 must reach pd_get rather than end as urllib3's RetryError.
    check(not riot.max_retries.is_retry("GET", 429, has_retry_after=True), riot.max_retries)
    print(
        "riot_client self-check OK (a minute's budget, first reads ahead, learned limit, "
        "lookups retried)"
    )


if __name__ == "__main__":
    _self_check()

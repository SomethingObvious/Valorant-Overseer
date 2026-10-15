# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""Talking to the VALORANT client on this PC and to Riot's servers through it.

Reads the lockfile, the tokens and the region, and throttles every call.
"""

from __future__ import annotations

import base64
import json
import os
import threading
import time
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import Any

import requests
import urllib3
from common import console_logger
from vconstants import GAMEMODES, map_name_from_path

# The Riot client serves its local API on 127.0.0.1 with a self-signed
# certificate, so the loopback calls pass verify=False. The *.a.pvp.net calls
# carry the entitlements and access tokens and must verify.
urllib3.disable_warnings(urllib3.exceptions.InsecureRequestWarning)


REGION_MAP = {
    "na": ("na", "na-1", "na"),
    "eu": ("eu", "eu-1", "eu"),
    "ap": ("ap", "ap-1", "ap"),
    "kr": ("kr", "kr-1", "kr"),
    "latam": ("latam", "na-1", "latam"),
    "br": ("br", "na-1", "br"),
}


@dataclass
class _State:
    """What the client has said once, kept for every later ask."""

    # The client's version, read once from its presence or its log.
    client_version: str | None = None
    # Whether the party payload's keys have been logged once.
    party_logged: bool = False
    # When the party started queueing, while it queues.
    queue_started: float | None = None


_STATE = _State()

_SESSIONS = threading.local()
# The client's token, by the lockfile it came through: the headers, whose
# they are and when they were fetched. Every tick made a new LocalAuth and
# fetched them again, and a token lasts about an hour.
_TOKENS: dict[tuple[str, str], tuple[dict[str, Any], str, float]] = {}
_TOKEN_SECS = 300.0


def _session() -> requests.Session:
    """Return this thread's session, which keeps its connections open.

    A new one per call built a TLS context and read the Windows certificate
    store every time, which was nine tenths of what the backend did while idle.
    """
    session: requests.Session | None = getattr(_SESSIONS, "session", None)
    if session is None:
        session = requests.Session()
        _SESSIONS.session = session
    return session


_QUEUE_NAMES = {
    "competitive": "Competitive",
    "unrated": "Unrated",
    "swiftplay": "Swiftplay",
    "spikerush": "Spike Rush",
    "deathmatch": "Deathmatch",
    "ggteam": "Escalation",
    "hurm": "Team Deathmatch",
    "": "Custom",
}


_log = console_logger("riot_client")


_RIOT_RATE_LOCK = threading.Lock()
try:
    _RIOT_MAX_RPS = max(0.0, float(os.getenv("RIOT_MAX_RPS", "10")))
except ValueError:
    _log(f"RIOT_MAX_RPS {os.getenv('RIOT_MAX_RPS')!r} is not a number (using 10)")
    _RIOT_MAX_RPS = 10.0
_RIOT_BURST = _RIOT_MAX_RPS if _RIOT_MAX_RPS > 0 else 1.0
_RIOT_BUCKET = {"tokens": _RIOT_BURST, "at": 0.0}

_MMR_BURST = 24.0
_MMR_RPS = 0.4
_MMR_BUCKET = {"tokens": _MMR_BURST, "at": 0.0}

# A 429's Retry-After holds back every endpoint in its family until then.
_HOLD_UNTIL = {"mmr": 0.0, "other": 0.0}


def _family(endpoint: str) -> str:
    return "mmr" if endpoint.startswith("/mmr/") else "other"


def held_secs(endpoint: str) -> float:
    """Return how long Riot asked calls like `endpoint` to wait, in seconds."""
    with _RIOT_RATE_LOCK:
        return max(0.0, _HOLD_UNTIL[_family(endpoint)] - time.time())


def _set_hold(endpoint: str, seconds: float) -> None:
    fam = _family(endpoint)
    with _RIOT_RATE_LOCK:
        _HOLD_UNTIL[fam] = max(_HOLD_UNTIL[fam], time.time() + seconds)


def _take_token(bucket: dict[str, Any], burst: float, rps: float) -> None:
    while True:
        with _RIOT_RATE_LOCK:
            now = time.time()
            if bucket["at"] == 0.0:
                bucket["at"] = now
            bucket["tokens"] = min(burst, bucket["tokens"] + (now - bucket["at"]) * rps)
            bucket["at"] = now
            if bucket["tokens"] >= 1.0:
                bucket["tokens"] -= 1.0
                return
            wait = (1.0 - bucket["tokens"]) / rps
        time.sleep(wait)


def _riot_throttle(endpoint: str = "") -> None:
    if _RIOT_MAX_RPS <= 0:
        return
    while True:
        wait = held_secs(endpoint)
        if wait <= 0:
            break
        time.sleep(wait)
    if _family(endpoint) == "mmr":
        _take_token(_MMR_BUCKET, _MMR_BURST, _MMR_RPS)
    _take_token(_RIOT_BUCKET, _RIOT_BURST, _RIOT_MAX_RPS)


class ClientNotReadyError(Exception):
    """The Riot client is running but has no entitlements to hand out yet."""


class GameNotStartedError(Exception):
    """VALORANT hasn't got far enough into starting to say which region it plays in."""


def _lockfile_path() -> Path:
    return Path(os.getenv("LOCALAPPDATA", "")) / r"Riot Games\Riot Client\Config\lockfile"


def _shooter_log_path() -> Path:
    return Path(os.getenv("LOCALAPPDATA", "")) / r"VALORANT\Saved\Logs\ShooterGame.log"


class LocalAuth:
    """The VALORANT client on this PC, and Riot's servers reached with its tokens."""

    def __init__(self, region: str | None = None) -> None:
        """Read the lockfile, and the region from `region` or the client's log."""
        self.lockfile = self._get_lockfile()

        region = (region or "").strip().lower()
        if region in REGION_MAP:
            shard, ga, gb = REGION_MAP[region]
            self.region: list[Any] = [shard, [ga, gb]]
        else:
            self.region = self._get_region()
        self.pd_url = f"https://pd.{self.region[0]}.a.pvp.net"
        self.glz_url = f"https://glz-{self.region[1][0]}.{self.region[1][1]}.a.pvp.net"
        self.shard = self.region[0]
        self.kept_headers: dict[str, Any] | None = None
        self.puuid = ""
        self.req_count = 0

    @staticmethod
    def available() -> bool:
        """Return whether the Riot client is running, by whether its lockfile is there."""
        return _lockfile_path().is_file()

    def _get_lockfile(self) -> dict[str, Any]:
        with _lockfile_path().open(encoding="utf-8") as f:
            keys = ["name", "PID", "port", "password", "protocol"]
            return dict(zip(keys, f.read().split(":"), strict=True))

    def _get_region(self) -> list[Any]:
        pd_url = glz_url = None
        if not _shooter_log_path().is_file():
            msg = "VALORANT hasn't written ShooterGame.log yet"
            raise GameNotStartedError(msg)
        with _shooter_log_path().open(encoding="utf8") as f:
            for line in f:
                if ".a.pvp.net/account-xp/v1/" in line:
                    pd_url = line.split(".a.pvp.net/account-xp/v1/")[0].split(".")[-1]
                elif "https://glz" in line:
                    glz_url = [
                        line.split("https://glz-")[1].split(".")[0],
                        line.split("https://glz-")[1].split(".")[1],
                    ]
                if pd_url and glz_url:
                    if pd_url == "pbe":
                        return ["na", ["na-1", "na"]]
                    return [pd_url, glz_url]
        # The log starts over each launch, and names the region's servers a
        # little way into starting up.
        msg = "could not parse region from ShooterGame.log"
        raise GameNotStartedError(msg)

    def _local_headers(self) -> dict[str, str]:
        token = base64.b64encode(("riot:" + self.lockfile["password"]).encode()).decode()
        return {"Authorization": f"Basic {token}"}

    def client_version(self) -> str:
        """Return the client's version, from its presence, valorant-api or its log.

        Kept once found. release-09.00 when nothing says, without keeping that.
        """
        if _STATE.client_version:
            return _STATE.client_version
        try:
            data = self.local_get("/chat/v4/presences")
            for pr in (data or {}).get("presences", []) or []:
                if pr.get("product") != "valorant" or not pr.get("private"):
                    continue
                try:
                    priv = json.loads(base64.b64decode(str(pr["private"])).decode("utf-8"))
                except ValueError:
                    continue
                v = (priv.get("partyPresenceData") or {}).get("partyClientVersion") or priv.get(
                    "partyClientVersion"
                )
                if v:
                    _STATE.client_version = v
                    _log(f"client version (local presence): {v}")
                    return v
        except (requests.RequestException, ValueError, AttributeError, TypeError) as e:
            _log(f"local presence version lookup failed ({e!r}), trying valorant-api")
        try:
            data = requests.get("https://valorant-api.com/v1/version", timeout=6).json()
            rcv = (data.get("data") or {}).get("riotClientVersion")
            if rcv:
                _STATE.client_version = rcv
                _log(f"client version (valorant-api): {rcv}")
                return rcv
        except (requests.RequestException, ValueError, AttributeError) as e:
            _log(f"valorant-api version lookup failed ({e!r}), using the log")
        try:
            with _shooter_log_path().open(encoding="utf8") as f:
                for line in f:
                    if "CI server version:" in line:
                        v = line.split("CI server version:", 1)[1].strip()
                        if v:
                            _STATE.client_version = v
                            return v
        except (OSError, ValueError) as e:
            _log(f"ShooterGame.log version lookup failed ({e!r}), using release-09.00")
        return "release-09.00"

    def headers(self, *, refresh: bool = False) -> dict[str, Any]:
        """Return the headers Riot's servers want, with this account's tokens.

        Kept for the lockfile's session, and fetched again with `refresh`.
        """
        if self.kept_headers and not refresh:
            return self.kept_headers
        key = (str(self.lockfile.get("port")), str(self.lockfile.get("password")))
        kept = _TOKENS.get(key)
        if kept and not refresh and time.time() - kept[2] < _TOKEN_SECS:
            self.kept_headers, self.puuid = kept[0], kept[1]
            return self.kept_headers
        resp = _session().get(
            f"https://127.0.0.1:{self.lockfile['port']}/entitlements/v1/token",
            headers=self._local_headers(),
            verify=False,
            timeout=5,
        )
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
            if resp.status_code == 429:
                return {"errorCode": "RATE_LIMITED", "status": 429}
            return {}

    def glz_get(self, endpoint: str) -> dict[str, Any]:
        """Return a GET from the glz server, the one that runs matches and parties."""
        _riot_throttle()
        self.req_count += 1
        resp = _session().get(self.glz_url + endpoint, headers=self.headers(), timeout=8)
        return self._json(resp)

    def pd_get(self, endpoint: str, *, refresh: bool = False, retries: int = 0) -> dict[str, Any]:
        """Return a GET from the pd server, retrying a 429 `retries` times with backoff."""
        backoff = 3.0
        for attempt in range(retries + 1):
            _riot_throttle(endpoint)
            self.req_count += 1
            resp = _session().get(
                self.pd_url + endpoint, headers=self.headers(refresh=refresh), timeout=8
            )
            if resp.status_code == 429:
                try:
                    ra = float(resp.headers.get("Retry-After") or 0)
                except (TypeError, ValueError):
                    ra = 0.0
                _set_hold(endpoint, ra or backoff)
                if attempt < retries:
                    backoff += 3.0
                    continue
                return {"errorCode": "RATE_LIMITED", "status": 429}
            return self._json(resp)
        return {"errorCode": "RATE_LIMITED", "status": 429}

    def pd_put(self, endpoint: str, payload: Any, *, refresh: bool = False) -> dict[str, Any]:
        """Return a PUT to the pd server."""
        _riot_throttle(endpoint)
        self.req_count += 1
        return self._json(
            requests.put(
                self.pd_url + endpoint,
                headers=self.headers(refresh=refresh),
                json=payload,
                timeout=8,
            )
        )

    def local_get(self, endpoint: str) -> dict[str, Any]:
        """Return a GET from the client's own API on 127.0.0.1."""
        return (
            _session()
            .get(
                f"https://127.0.0.1:{self.lockfile['port']}{endpoint}",
                headers=self._local_headers(),
                verify=False,
                timeout=5,
            )
            .json()
        )


def _offline_presence_private() -> dict[str, Any] | None:
    try:
        import offline_launch

        return offline_launch.captured_presence_private()
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
    except (requests.RequestException, ValueError, AttributeError, TypeError) as e:
        _log(f"presence read failed: {e!r}")
        return None
    for pr in presences:
        if pr.get("puuid") != auth.puuid or not pr.get("private"):
            continue
        if pr.get("product") not in (None, "valorant"):
            continue
        try:
            priv = json.loads(base64.b64decode(str(pr["private"])).decode("utf-8"))
            return priv if isinstance(priv, dict) else None
        except ValueError:
            return None
    return None


def party_snapshot(auth: LocalAuth) -> dict[str, Any]:
    """Return your party and its queue: who is in it, its state, and the queue time."""
    priv = _self_presence_private(auth)
    if not priv:
        return {"available": False}
    pdata = priv.get("partyPresenceData") or {}
    pid = pdata.get("partyId") or priv.get("partyId")
    if not pid:
        return {"available": False}

    def _label(q: str) -> str:
        return GAMEMODES.get(q, q.replace("_", " ").title())

    state = pdata.get("partyState") or "DEFAULT"
    qid = (
        priv.get("queueId") or (priv.get("matchPresenceData") or {}).get("queueId") or ""
    ).lower()
    snap = {
        "available": True,
        "partyId": pid,
        "queueId": qid or None,
        "queueName": _label(qid) if qid else None,
        "eligible": [],
        "state": state,
        "inQueue": "MATCHMAKING" in state,
        "queuedAt": None,
        "partySize": pdata.get("partySize") or priv.get("partySize") or 1,
        "isOwner": bool(pdata.get("isPartyOwner", True)),
        "allReady": True,
    }

    party = auth.glz_get(f"/parties/v1/parties/{pid}")
    if isinstance(party, dict) and party.get("Members"):
        if not _STATE.party_logged:
            _STATE.party_logged = True
            _log(f"party payload keys: {sorted(party.keys())}")
        members = party.get("Members") or []
        mine: dict[str, Any] = next((m for m in members if m.get("Subject") == auth.puuid), {})
        gqid = ((party.get("MatchmakingData") or {}).get("QueueID") or "").lower()
        if gqid:
            snap["queueId"], snap["queueName"] = gqid, _label(gqid)
        if party.get("State"):
            snap["state"] = party["State"]
            snap["inQueue"] = "MATCHMAKING" in party["State"]
        snap["eligible"] = [
            {"id": q, "name": _label(q)} for q in (party.get("EligibleQueues") or [])
        ]
        snap["queuedAt"] = _iso_to_epoch(party.get("QueueEntryTime"))
        snap["partySize"] = len(members)
        if "IsOwner" in mine:
            snap["isOwner"] = bool(mine.get("IsOwner"))
        snap["allReady"] = all(bool(m.get("IsReady", True)) for m in members)
        # A custom game has its map picked in the lobby. Matchmaking picks one
        # only once a match is found, so a queue lobby has none.
        custom = (party.get("CustomGameData") or {}).get("Settings") or {}
        if "CUSTOM_GAME" in str(snap["state"]) and custom.get("Map"):
            snap["map"] = map_name_from_path(custom["Map"])
    elif isinstance(party, dict) and party.get("status") == 429:
        snap["throttled"] = True

    if snap["inQueue"]:
        now = time.time()
        glz_at = snap["queuedAt"]
        if glz_at and glz_at <= now:
            _STATE.queue_started = glz_at
        elif _STATE.queue_started is None:
            _STATE.queue_started = now
        snap["queuedAt"] = _STATE.queue_started

        snap["queueElapsed"] = max(0, round(now - _STATE.queue_started, 1))
    else:
        _STATE.queue_started = None
        snap["queuedAt"] = None
    return snap


class RiotClient:
    """What the environment says: the Riot API key and where data comes from."""

    def __init__(self) -> None:
        """Read RIOT_API_KEY and DATA_SOURCE from the environment."""
        self.api_key = os.getenv("RIOT_API_KEY", "").strip()
        self.source_pref = os.getenv("DATA_SOURCE", "auto").strip().lower()

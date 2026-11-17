# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""Appear Offline: friends see you offline, away or on mobile while you play."""

from __future__ import annotations

import asyncio
import base64
import codecs
import contextlib
import functools
import http.client
import json
import os
import re
import secrets
import ssl
import subprocess
import sys
import threading
import time
import uuid
from datetime import UTC, datetime
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import TYPE_CHECKING, Any

import requests
from common.check import check, present
from common.system import TASKKILL, process_names

if TYPE_CHECKING:
    from collections.abc import Callable

    from cryptography import x509

# A public name that resolves to 127.0.0.1, with a certificate anyone can
# download. The Riot Client checks the chat host's certificate against its
# name, so the proxy needs a real one.
CHAT_DOMAIN = "deceive-localhost.molenzwiebel.xyz"
CERT_URL = os.getenv("OVERSEER_OFFLINE_CERT_URL", "https://mln.cx/deceive/localhost.pfx")
RIOT_CONFIG = "https://clientconfig.rpg.riotgames.com"
CHAT_AFFINITY = "https://riot-geo.pas.si.riotgames.com/pas/v1/service/chat"

_HOME = Path(os.getenv("LOCALAPPDATA", str(Path.home()))) / "Valorant Overseer" / "offline"
_CERT = _HOME / "chat.pem"
# A download a week keeps up with a certificate replaced every two months.
_CERT_FRESH = 7 * 86400
_LOG = _HOME / "engine.log"
_LOOK_FILE = _HOME / "status"
_HELPER_FILE = Path(os.getenv("OVERSEER_OFFLINE_HELPER_STATE", str(_HOME / "helper.json")))
_IS_HELPER = os.getenv("OVERSEER_OFFLINE_HELPER") == "1"
_HELPER_SPEAKS = 2
_HELPER_STARTING = threading.Lock()
_INSTALLS = (
    Path(os.getenv("PROGRAMDATA", r"C:\ProgramData")) / "Riot Games" / "RiotClientInstalls.json"
)

LOOKS = ("online", "offline", "away", "mobile")
_FIRST_LOOK = "offline"
_RIOT_EXES = (
    "RiotClientServices.exe",
    "VALORANT.exe",
    "VALORANT-Win64-Shipping.exe",
    "RiotClientCrashHandler.exe",
)
_GAMES = re.compile(
    r"<(valorant|league_of_legends|bacon|lion|keystone|riot_client)\b[^>]*?(?:/>|>.*?</\1>)",
    re.DOTALL,
)
_STATUS = re.compile(r"<status\s*/>|<status>.*?</status>", re.DOTALL)
_SHOW = re.compile(r"<show\s*/>|<show>.*?</show>", re.DOTALL)
_BODY = re.compile(r"<body>(.*?)</body>", re.DOTALL)
_ROSTER = re.compile(rb"<query xmlns=['\"]jabber:iq:riotgames:roster['\"]>")
_PRIVATE = re.compile(r"<valorant\b[^>]*>.*?<p>([A-Za-z0-9+/=]+)</p>.*?</valorant>", re.DOTALL)

_STAND_IN = "0b5e55ed-6a3e-4f1a-9c00-00000000a11e"
_STAND_IN_JID = f"{_STAND_IN}@eu1.pvp.net"
_STAND_IN_FROM = f"{_STAND_IN_JID}/overseer"
_STAND_IN_NAME = "&#9;Valorant Overseer Active"
_NOBODY = "00000000-0000-0000-0000-000000000000"


@functools.cache
def _http() -> requests.Session:
    return requests.Session()


def _note(text: str, *, loud: bool = False) -> None:
    # ValueError is the UnicodeEncodeError a lone surrogate from Riot raises,
    # and this is where failures go, so it can't fail itself.
    try:
        _LOG.parent.mkdir(parents=True, exist_ok=True)
        # The helper can run for weeks between proxy starts, which clear it.
        mode = "w" if _LOG.is_file() and _LOG.stat().st_size > 2_000_000 else "a"
        with _LOG.open(mode, encoding="utf-8") as log:
            log.write(f"{time.strftime('%H:%M:%S')} {text}\n")
    except (OSError, ValueError):
        pass
    if loud:
        with contextlib.suppress(OSError, ValueError):
            print(f"[offline] {text}", flush=True)


def _saved_look() -> str:
    try:
        look = _LOOK_FILE.read_text(encoding="utf-8").strip().lower()
    except (OSError, ValueError):
        return _FIRST_LOOK
    return look if look in LOOKS else _FIRST_LOOK


def _save_look(look: str) -> None:
    try:
        _LOOK_FILE.parent.mkdir(parents=True, exist_ok=True)
        _LOOK_FILE.write_text(look, encoding="utf-8")
    except OSError as e:
        _note(f"look: {look} wasn't saved to {_LOOK_FILE} ({e!r})")


def _told(look: str) -> str:
    if look == "online":
        return "Appear Offline is paused, so your friends see you ONLINE again."
    if look == "away":
        return "Your friends now see you as AWAY."
    return f"Your friends now see you as {look.upper()}."


def _close_of(text: str, start: int, name: str) -> int:
    head = text.find(">", start)
    if head < 0:
        return -1
    if text[head - 1] == "/":
        return head + 1
    shut = text.find(f"</{name}>", head)
    return -1 if shut < 0 else shut + len(name) + 3


def _partial_open(text: str, names: tuple[str, ...]) -> str:
    for size in range(min(len(text), max(map(len, names)) + 1), 0, -1):
        end = text[-size:]
        if any(f"<{name}".startswith(end) for name in names):
            return end
    return ""


def _cut(text: str, names: tuple[str, ...]) -> tuple[list[tuple[str, str]], str]:
    parts: list[tuple[str, str]] = []
    at = 0
    while True:
        found = [(k, name) for name in names if (k := text.find(f"<{name}", at)) >= 0]
        if not found:
            held = _partial_open(text[at:], names)
            parts.append(("", text[at : len(text) - len(held)]))
            return parts, held
        start, name = min(found)
        parts.append(("", text[at:start]))
        end = _close_of(text, start, name)
        if end < 0:
            return parts, text[start:]
        parts.append((name, text[start:end]))
        at = end


def _addressed(stanza: str) -> bool:
    return re.search(r"\bto=", stanza[: stanza.find(">") + 1]) is not None


def _disguise(stanza: str, look: str) -> str:
    if "<presence" not in stanza or _addressed(stanza):
        return stanza
    shown = _SHOW.sub(f"<show>{look}</show>", stanza, count=1)
    plain = _GAMES.sub("", _STATUS.sub("", shown))
    if plain != stanza:
        _note(f"presence: shown as {look} ({len(stanza)} to {len(plain)} bytes)")
    return plain


def hide_presences(
    text: str,
    look: str = _FIRST_LOOK,
    seen: Callable[[str], None] | None = None,
    *,
    rewrite: bool = True,
) -> tuple[str, str]:
    """Disguise every whole presence in `text`. Returns what to send and what to hold back."""
    parts, held = _cut(text, ("presence",))
    sent = []
    for name, part in parts:
        if name and seen is not None and not _addressed(part):
            seen(part)
        sent.append(_disguise(part, look) if name and rewrite else part)
    return "".join(sent), held


def drop_stand_in(text: str) -> str:
    """Return the client's chat without the stanzas for the stand-in, whom Riot never heard of."""
    parts, held = _cut(text, ("message", "iq", "presence"))
    return "".join(part for name, part in parts if not (name and _STAND_IN in part)) + held


def add_stand_in(data: bytes) -> bytes | None:
    """Return the friend list with the stand-in at the top, or None when `data` has no list."""
    found = _ROSTER.search(data)
    if found is None:
        return None
    entry = (
        f"<item jid='{_STAND_IN_JID}' name='{_STAND_IN_NAME}' subscription='both'"
        f" puuid='{_STAND_IN}'><group priority='9999'>Valorant Overseer</group>"
        f"<state>online</state><id name='{_STAND_IN_NAME}' tagline='OFFLINE'/>"
        f"<lol name='{_STAND_IN_NAME}'/><platforms><riot name='{_STAND_IN_NAME}'"
        " tagline='OFFLINE'/></platforms></item>"
    ).encode()
    return data[: found.end()] + entry + data[found.end() :]


def _private_of(stanza: str) -> dict[str, Any] | None:
    blob = _PRIVATE.search(stanza or "")
    if blob is None:
        return None
    # Read on every presence the client sends, so a bad one is passed over.
    # A bad base64, JSON or UTF-8 error is a ValueError each.
    try:
        got = json.loads(base64.b64decode(blob.group(1)))
    except ValueError:
        return None
    return got if isinstance(got, dict) else None


def _client_version(stanza: str) -> str | None:
    party = (_private_of(stanza) or {}).get("partyPresenceData")
    version = party.get("partyClientVersion") if isinstance(party, dict) else None
    return version if isinstance(version, str) and version else None


def _stand_in_private(version: str | None, look: str, now_ms: int) -> dict[str, Any]:
    owner = {
        "partyOwnerSessionLoopState": "MENUS",
        "partyOwnerMatchMap": "",
        "partyOwnerProvisioningFlow": "Invalid",
        "partyOwnerMatchScoreAllyTeam": 0,
        "partyOwnerMatchScoreEnemyTeam": 0,
    }
    party = {
        "partyId": _NOBODY,
        "isPartyOwner": True,
        "partyState": "DEFAULT",
        "partyAccessibility": "CLOSED",
        "partyLFM": False,
        "partyClientVersion": version or "unknown",
        "partyVersion": now_ms,
        "partySize": 1,
        "maxPartySize": 5,
        "queueEntryTime": "0001.01.01-00.00.00",
        "isPartyCrossPlayEnabled": False,
        "isPlayerCrossPlayEnabled": False,
        "partyPrecisePlatformTypes": 1,
        "customGameName": "Valorant Overseer Active",
        "customGameTeam": "",
        "tournamentId": "",
        "rosterId": "",
        **owner,
    }
    premier = {
        "rosterId": "",
        "rosterName": f"{look.capitalize()} Mode Active",
        "rosterTag": "Valorant Overseer Active",
        "rosterType": "VCT",
        "division": 0,
        "score": 0,
        "plating": 0,
        "showAura": False,
        "showTag": True,
        "showPlating": False,
    }
    return {
        "isValid": True,
        "isIdle": False,
        "queueId": "competitive",
        "provisioningFlow": "Invalid",
        "partyId": _NOBODY,
        "partySize": 1,
        "maxPartySize": 5,
        "partyOwnerMatchScoreAllyTeam": 0,
        "partyOwnerMatchScoreEnemyTeam": 0,
        "premierPresenceData": premier,
        "matchPresenceData": {
            "sessionLoopState": "MENUS",
            "provisioningFlow": "Invalid",
            "matchMap": "",
            "queueId": "competitive",
        },
        "partyPresenceData": party,
        "playerPresenceData": {
            "playerCardId": "d93ad22d-4db7-b6bc-5e9c-e5959bb9dd76",
            "playerTitleId": "e3ca05a4-4e44-9afe-3791-7d96ca8f71fa",
            "accountLevel": 999,
            "competitiveTier": 27,
            "leaderboardPosition": 1,
        },
    }


def _stand_in_presence(version: str | None, look: str) -> bytes:
    now = int(time.time() * 1000)
    private = base64.b64encode(json.dumps(_stand_in_private(version, look, now)).encode())
    games = (
        f"<keystone><st>chat</st><s.t>{now}</s.t><s.p>keystone</s.p><pty/></keystone>"
        f"<league_of_legends><st>chat</st><s.t>{now}</s.t><s.p>league_of_legends</s.p>"
        "<s.c>live</s.c><p>{&quot;pty&quot;:true}</p></league_of_legends>"
        f"<valorant><st>chat</st><s.t>{now}</s.t><s.p>valorant</s.p><s.r>PC</s.r>"
        f"<p>{private.decode('ascii')}</p><pty/></valorant>"
        f"<bacon><st>chat</st><s.t>{now}</s.t><s.l>bacon_availability_online</s.l>"
        "<s.p>bacon</s.p></bacon>"
    )
    return (
        f"<presence from='{_STAND_IN_FROM}' id='b-{uuid.uuid4()}'><games>{games}</games>"
        "<show>chat</show><platform>riot</platform><status/></presence>"
    ).encode()


def _from_stand_in(text: str) -> bytes:
    sent = time.strftime("%Y-%m-%d %H:%M:%S.000", time.gmtime())
    return (
        f"<message from='{_STAND_IN_FROM}' stamp='{sent}' id='overseer-{uuid.uuid4()}'"
        f" type='chat'><body>{text}</body></message>"
    ).encode()


def _riot_client() -> str | None:
    try:
        installs = json.loads(_INSTALLS.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None
    paths = [installs.get(k) for k in ("rc_default", "rc_live", "rc_beta")]
    return next((p for p in paths if isinstance(p, str) and Path(p).is_file()), None)


def _close_riot() -> None:
    for exe in _RIOT_EXES:
        # taskkill fails for a program that isn't running, which is most of
        # these most of the time, so its exit code says nothing.
        try:
            subprocess.run(
                [TASKKILL, "/F", "/IM", exe],
                capture_output=True,
                timeout=10,
                creationflags=subprocess.CREATE_NO_WINDOW,
                check=False,
            )
        except (OSError, subprocess.SubprocessError) as e:
            _note(f"launch: closing {exe} failed ({e!r})")


def _quiet(**more: Any) -> dict[str, Any]:
    # The backend runs below normal, and a child given no class of its own
    # takes that, so the Riot Client and VALORANT would run below it too.
    return {
        "stdin": subprocess.DEVNULL,
        "stdout": subprocess.DEVNULL,
        "stderr": subprocess.DEVNULL,
        "creationflags": subprocess.CREATE_NO_WINDOW | subprocess.NORMAL_PRIORITY_CLASS,
        **more,
    }


def _cert_fault(cert: x509.Certificate) -> str | None:
    from cryptography import x509

    try:
        names = cert.extensions.get_extension_for_class(
            x509.SubjectAlternativeName
        ).value.get_values_for_type(x509.DNSName)
    except x509.ExtensionNotFound:
        names = []
    if CHAT_DOMAIN not in names:
        return f"it is for {names or 'no name'}, not {CHAT_DOMAIN}"
    began, ends = cert.not_valid_before_utc, cert.not_valid_after_utc
    if not began <= datetime.now(UTC) <= ends:
        return f"it runs from {began:%Y-%m-%d} to {ends:%Y-%m-%d}"
    return None


def _pem_from(pfx: bytes) -> bytes | None:
    try:
        from cryptography.hazmat.primitives.serialization import (
            Encoding,
            NoEncryption,
            PrivateFormat,
            pkcs12,
        )

        key, cert, chain = pkcs12.load_key_and_certificates(pfx, None)
        if key is None or cert is None:
            _note("cert: the download has no key or no certificate")
            return None
        fault = _cert_fault(cert)
        if fault:
            _note(f"cert: turned down, as {fault}")
            return None
        pem = b"".join(c.public_bytes(Encoding.PEM) for c in [cert, *(chain or [])])
        return pem + key.private_bytes(Encoding.PEM, PrivateFormat.PKCS8, NoEncryption())
    except Exception as e:
        # cryptography can be missing, or can refuse the pfx with several
        # kinds of exception, and every one of them means no certificate.
        _note(f"cert: the pfx didn't convert ({e!r})")
        return None


def _fetch_cert() -> bool:
    try:
        got = _http().get(CERT_URL, timeout=15)
        got.raise_for_status()
    except requests.RequestException as e:
        _note(f"cert: the download failed ({e})")
        return False
    pem = _pem_from(got.content)
    if pem is None:
        return False
    fresh = _CERT.with_suffix(".tmp")
    try:
        _CERT.parent.mkdir(parents=True, exist_ok=True)
        fresh.write_bytes(pem)
        # Loaded before it replaces the saved one, so a PEM the ssl module
        # refuses never becomes the one the proxy starts with. This is also
        # what says the key is the certificate's own.
        ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER).load_cert_chain(fresh)
        fresh.replace(_CERT)
    except ssl.SSLError as e:
        _note(f"cert: the converted PEM won't load ({e})")
    except OSError as e:
        _note(f"cert: the converted PEM wasn't saved to {_CERT} ({e!r})")
    else:
        _note("cert: downloaded and saved")
        return True
    with contextlib.suppress(OSError):
        fresh.unlink()
    return False


def _saved_cert_fits() -> bool:
    try:
        from cryptography import x509

        cert = x509.load_pem_x509_certificate(_CERT.read_bytes())
    except (OSError, ValueError, ImportError):
        return False
    return _cert_fault(cert) is None


def ensure_cert() -> Path:
    """Return the chat proxy's certificate file: the saved one, downloaded again each week."""
    try:
        fresh = 0 <= time.time() - _CERT.stat().st_mtime < _CERT_FRESH
    except OSError:
        fresh = False
    if fresh and _saved_cert_fits():
        return _CERT
    if _fetch_cert():
        return _CERT
    if _saved_cert_fits():
        _note("cert: the download failed, so the saved one stands in while it is in date")
        return _CERT
    msg = (
        "Appear Offline couldn't get its certificate. The download from "
        f"{CERT_URL} failed and the saved copy is missing or out of date. "
        "Check your connection and try again."
    )
    raise RuntimeError(msg)


class _Link:
    def __init__(self, to_client: Any, to_riot: Any) -> None:
        self.to_client = to_client
        self.to_riot = to_riot
        self.version: str | None = None
        self.listed = False
        self.shown = False
        # Your last broadcast presence as the client wrote it, and the
        # VALORANT block from it.
        self.presence: str | None = None
        self.private: dict[str, Any] | None = None
        self.private_at = 0.0

    def saw(self, stanza: str) -> None:
        self.presence = stanza
        private = _private_of(stanza)
        if private:
            self.private, self.private_at = private, time.time()

    async def tell(self, data: bytes, what: str) -> None:
        try:
            self.to_client.write(data)
            await self.to_client.drain()
        except OSError as e:
            _note(f"chat: the {what} didn't reach the client ({e!r})")


class _Proxy:
    def __init__(self) -> None:
        self.starting = threading.Lock()
        self.up = False
        self.config_port: int | None = None
        self.chat_port: int | None = None
        # Riot's chat server, learned from the config as it passes through.
        # Once this account's own host is looked up it stays.
        self.riot_host: str | None = None
        self.riot_port = 5223
        self.host_pinned = False
        self.loop: asyncio.AbstractEventLoop | None = None
        self.look = _FIRST_LOOK
        self.links: list[_Link] = []
        self.later: set[asyncio.Task[None]] = set()

    def start(self) -> None:
        with self.starting:
            if self.up:
                return
            cert = ensure_cert()
            with contextlib.suppress(OSError):
                _LOG.parent.mkdir(parents=True, exist_ok=True)
                _LOG.write_text("", encoding="utf-8")
            saved = _saved_look()
            self.look = _FIRST_LOOK if saved == "online" else saved
            self._serve_chat(cert)
            server = ThreadingHTTPServer(("127.0.0.1", 0), _ConfigServer)
            self.config_port = server.server_address[1]
            threading.Thread(
                target=server.serve_forever, name="offline-config", daemon=True
            ).start()
            self.up = True
            _note(f"proxy up, config on {self.config_port}, chat on {self.chat_port}", loud=True)

    def _serve_chat(self, cert: Path) -> None:
        tls = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        tls.load_cert_chain(cert)
        loop = self.loop = asyncio.new_event_loop()
        opened = threading.Event()

        async def listen() -> None:
            server = await asyncio.start_server(self._link, "127.0.0.1", 0, ssl=tls)
            self.chat_port = server.sockets[0].getsockname()[1]
            opened.set()
            await server.serve_forever()

        threading.Thread(
            target=loop.run_until_complete, args=(listen(),), name="offline-chat", daemon=True
        ).start()
        if not opened.wait(10):
            msg = "Appear Offline couldn't open its chat port on 127.0.0.1."
            raise RuntimeError(msg)

    def _soon(self, work: Any) -> None:
        task = asyncio.create_task(work)
        self.later.add(task)
        task.add_done_callback(self.later.discard)

    async def _link(self, from_client: Any, to_client: Any) -> None:
        host, port = self.riot_host, self.riot_port
        if not host:
            _note("chat: the client came before its config named a chat host", loud=True)
            to_client.close()
            return
        try:
            from_riot, to_riot = await asyncio.open_connection(
                host, port, ssl=ssl.create_default_context(), server_hostname=host
            )
        except (OSError, ValueError) as e:
            _note(f"chat: Riot's {host}:{port} didn't answer ({e!r})")
            to_client.close()
            return
        _note(f"chat: passing the client through to {host}:{port}", loud=True)
        link = _Link(to_client, to_riot)
        self.links.append(link)

        async def then_close(pump: Any, other: Any) -> None:
            # Either side hanging up ends the other, or a closed client would
            # leave a session open on Riot's chat server with its last
            # presence, and Riot hanging up would go unnoticed by the client.
            try:
                await pump
            finally:
                other.close()

        try:
            ended = await asyncio.gather(
                then_close(self._outbound(from_client, link), to_riot),
                then_close(self._inbound(from_riot, link), to_client),
                return_exceptions=True,
            )
        finally:
            with contextlib.suppress(ValueError):
                self.links.remove(link)
        faults = [repr(e) for e in ended if isinstance(e, Exception)]
        _note("chat: the connection ended" + (f" with {faults}" if faults else ""))
        for stream in (to_client, to_riot):
            try:
                stream.close()
            except Exception as e:
                _note(f"chat: a stream wouldn't close ({e!r})")

    async def _outbound(self, reader: Any, link: _Link) -> None:
        text = codecs.getincrementaldecoder("utf-8")()
        held = ""
        while data := await reader.read(65536):
            chunk = text.decode(data)
            if _STAND_IN_JID in chunk:
                await self._command(chunk, link)
                chunk = drop_stand_in(chunk)
            if link.version is None and (version := _client_version(chunk)):
                link.version = version
                _note(f"c2s: the client is VALORANT {version}")
                if link.shown:
                    await self._show_stand_in(link)
            hiding = self.look != "online"
            sent, held = hide_presences(
                held + chunk, self.look if hiding else _FIRST_LOOK, link.saw, rewrite=hiding
            )
            if sent:
                link.to_riot.write(sent.encode("utf-8"))
                await link.to_riot.drain()
            if link.listed and not link.shown:
                await self._show_stand_in(link)
        held += text.decode(b"", final=True)
        if held:
            link.to_riot.write(held.encode("utf-8"))
            await link.to_riot.drain()

    async def _inbound(self, reader: Any, link: _Link) -> None:
        while data := await reader.read(65536):
            if not link.listed:
                listed = add_stand_in(data)
                if listed is not None:
                    data, link.listed = listed, True
                    _note("s2c: the stand-in is at the top of the friend list", loud=True)
                    self._soon(self._welcome(link))
            link.to_client.write(data)
            await link.to_client.drain()

    async def _show_stand_in(self, link: _Link) -> None:
        link.shown = True
        await link.tell(_stand_in_presence(link.version, self.look), "stand-in's presence")

    async def _welcome(self, link: _Link) -> None:
        await asyncio.sleep(6)
        await link.tell(
            _from_stand_in(
                f"Valorant Overseer is on, and your friends see you as {self.look.upper()}. "
                "Message me online, offline, away or mobile to change it."
            ),
            "welcome",
        )

    async def _command(self, chunk: str, link: _Link) -> None:
        body = _BODY.search(chunk)
        if body is None:
            return
        said = body.group(1).strip().lower()
        look = next(
            (look for look in ("offline", "away", "mobile", "online") if look in said), None
        )
        if look:
            self.set_look(look)
            return
        if "status" in said:
            reply = f"Your friends see you as {self.look.upper()}."
        elif "help" in said:
            reply = (
                "Send online, offline, away or mobile to change how you show, or status to check."
            )
        else:
            reply = "I only know online, offline, away, mobile, status and help."
        await link.tell(_from_stand_in(reply), f"reply to {said!r}")

    def set_look(self, look: str) -> dict[str, Any]:
        look = (look or "").strip().lower()
        if look not in LOOKS:
            return {
                "ok": False,
                "message": f"There's no '{look}'. Pick online, offline, away or mobile.",
            }
        self.look = look
        _save_look(look)
        _note(f"look: now {look}")
        if self.loop is not None:
            for link in list(self.links):
                asyncio.run_coroutine_threadsafe(self._announce(link), self.loop)
        return {"ok": True, "status": look, "enabled": look != "online"}

    def switch(self, *, enabled: bool) -> dict[str, Any]:
        if not enabled:
            return self.set_look("online")
        if self.look != "online":
            return self.set_look(self.look)
        saved = _saved_look()
        return self.set_look(_FIRST_LOOK if saved == "online" else saved)

    async def _announce(self, link: _Link) -> None:
        try:
            if link.presence:
                again = (
                    link.presence if self.look == "online" else _disguise(link.presence, self.look)
                )
                link.to_riot.write(again.encode("utf-8"))
                await link.to_riot.drain()
        except OSError as e:
            _note(f"chat: your presence as {self.look} didn't reach Riot ({e!r})")
        if link.shown:
            await link.tell(_stand_in_presence(link.version, self.look), "stand-in's presence")
        await link.tell(_from_stand_in(_told(self.look)), "change of look")

    def redirect(self, config: Any, auth: str | None) -> Any:
        if not isinstance(config, dict):
            return config
        host = config.get("chat.host")
        if isinstance(host, str):
            if not self.host_pinned:
                self.riot_host = host
            config["chat.host"] = CHAT_DOMAIN
            _note(f"config: chat goes to {CHAT_DOMAIN} in place of {host}")
        if isinstance(config.get("chat.port"), int):
            self.riot_port, config["chat.port"] = config["chat.port"], self.chat_port
        affinities = config.get("chat.affinities")
        if isinstance(affinities, dict):
            if config.get("chat.affinity.enabled") and auth and not self.host_pinned:
                own = _own_chat_host(affinities, auth)
                if own:
                    self.riot_host, self.host_pinned = own, True
            config["chat.affinities"] = dict.fromkeys(affinities, CHAT_DOMAIN)
        return config


def _own_chat_host(affinities: dict[str, Any], auth: str) -> str | None:
    # The answer is a bare JWT whose payload names the account's affinity.
    # It is a signed token for this account, so it is never logged.
    try:
        token = _http().get(CHAT_AFFINITY, headers={"Authorization": auth}, timeout=15).text
        claims = token.split(".")[1]
        affinity = json.loads(base64.urlsafe_b64decode(claims + "=" * (-len(claims) % 4)))[
            "affinity"
        ]
        host = affinities.get(affinity)
    except (requests.RequestException, IndexError, KeyError, ValueError, TypeError) as e:
        _note(f"config: this account's chat host wasn't found ({e!r}), so the default stands")
        return None
    _note(f"config: this account's chat is {affinity} on {host}")
    return host if isinstance(host, str) else None


# What the Riot Client sends that Riot's config server needs to see too.
_PASSED_ON = ("Authorization", "X-Riot-Entitlements-JWT", "User-Agent")


class _ConfigServer(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *_args: Any) -> None:
        """Quiet, as engine.log says what matters."""

    def do_GET(self) -> None:
        # Pasted onto Riot's host, a target like @other.host/ would send the
        # account's tokens to another server.
        if not self.path.startswith("/"):
            self.send_error(400)
            return
        passed = {name: self.headers[name] for name in _PASSED_ON if name in self.headers}
        _note(f"config: {self.path[:80]} asked for")
        try:
            riot = _http().get(RIOT_CONFIG + self.path, headers=passed, timeout=20)
        except requests.RequestException as e:
            _note(f"config: Riot didn't answer for {self.path[:80]} ({e!r})")
            self.send_error(502)
            return
        kind = riot.headers.get("Content-Type", "application/json")
        body = riot.content
        if "json" in kind.lower():
            try:
                redirected = _proxy.redirect(riot.json(), passed.get("Authorization"))
                body = json.dumps(redirected).encode("utf-8")
            except (ValueError, TypeError) as e:
                _note(f"config: {self.path[:80]} went through as it was ({e!r})")
        self.send_response(riot.status_code)
        self.send_header("Content-Type", kind)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


_proxy = _Proxy()


def _launch_here(look: str | None) -> dict[str, Any]:
    _note(f"launch: asked for, as {look!r}", loud=True)
    client = _riot_client()
    if not client:
        return {"ok": False, "message": "Couldn't find the Riot Client. Is VALORANT installed?"}
    look = (look or "").strip().lower()
    wanted = look in LOOKS and look != "online"
    if wanted:
        _save_look(look)
    # Broad, as the answer goes back to the window and a missing certificate,
    # a port that won't open and a bad PEM all fail differently.
    try:
        _proxy.start()
    except Exception as e:
        _note(f"launch: the proxy didn't start ({e!r})")
        return {"ok": False, "message": str(e)}
    if wanted and look != _proxy.look:
        _proxy.set_look(look)
    _note(f"launch: closing Riot, then starting {client}", loud=True)
    _close_riot()
    # Riot's processes take a moment to let go of their files.
    time.sleep(3.0)
    hidden = subprocess.STARTUPINFO()
    hidden.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    hidden.wShowWindow = subprocess.SW_HIDE
    argv = [
        client,
        f"--client-config-url=http://127.0.0.1:{_proxy.config_port}",
        "--launch-product=valorant",
        "--launch-patchline=live",
    ]
    try:
        subprocess.Popen(argv, **_quiet(startupinfo=hidden))
    except (OSError, ValueError) as e:
        _note(f"launch: {client} didn't start ({e!r})")
        return {"ok": False, "message": f"Couldn't start the Riot Client: {e}"}
    _note("launch: started the Riot Client with " + " ".join(argv[1:]))
    return {
        "ok": True,
        "message": "Starting VALORANT with Appear Offline. Sign in as usual, and your friends "
        f"will see you as {_proxy.look}.",
    }


def _switch_here(*, enabled: bool) -> dict[str, Any]:
    if not _proxy.up:
        return {"ok": False, "message": "Appear Offline isn't running."}
    return _proxy.switch(enabled=enabled)


def _status_here() -> dict[str, Any]:
    return {
        "running": _proxy.up,
        "status": _proxy.look,
        "enabled": _proxy.look != "online",
        "connected": bool(_proxy.links),
    }


def _private_here() -> dict[str, Any] | None:
    links = [link for link in list(_proxy.links) if link.private]
    newest = max(links, key=lambda link: link.private_at, default=None)
    return dict(newest.private or {}) if newest else None


# The helper, a process of its own that holds the proxy. Its file names the
# port it answers on and the key a question needs.


def _helper() -> dict[str, Any] | None:
    try:
        found = json.loads(_HELPER_FILE.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None
    fits = (
        isinstance(found, dict)
        and found.get("speaks") == _HELPER_SPEAKS
        and isinstance(found.get("port"), int)
        and found.get("key")
    )
    return found if fits else None


def _forget_helper(key: str | None = None) -> None:
    current = _helper()
    if key and current and current.get("key") != key:
        return
    with contextlib.suppress(OSError):
        _HELPER_FILE.unlink()


def _write_helper(port: int, key: str) -> None:
    _HELPER_FILE.parent.mkdir(parents=True, exist_ok=True)
    fresh = _HELPER_FILE.with_name(f".helper-{os.getpid()}-{uuid.uuid4().hex}.tmp")
    try:
        fresh.write_text(
            json.dumps({"speaks": _HELPER_SPEAKS, "pid": os.getpid(), "port": port, "key": key}),
            encoding="utf-8",
        )
        fresh.replace(_HELPER_FILE)
    finally:
        with contextlib.suppress(OSError):
            fresh.unlink()


def _ask_helper(what: str, given: dict[str, Any] | None = None, wait: float = 3.0) -> Any:
    found = _helper()
    if not found:
        return None
    line = http.client.HTTPConnection("127.0.0.1", found["port"], timeout=wait)
    try:
        line.request(
            "POST",
            "/",
            body=json.dumps({"do": what, **(given or {})}).encode("utf-8"),
            headers={"Content-Type": "application/json", "X-Overseer-Key": found["key"]},
        )
        answer = json.loads(line.getresponse().read().decode("utf-8"))
    except ConnectionRefusedError:
        # Nothing listens there, so the helper that wrote the file was ended
        # without cleaning up. Windows takes two seconds to refuse, and the
        # status is asked for with every board.
        _forget_helper(found["key"])
        return None
    except (OSError, http.client.HTTPException, ValueError):
        return None
    finally:
        line.close()
    return answer if isinstance(answer, dict) else None


def _helper_python() -> str:
    windowless = Path(sys.executable).with_name("pythonw.exe")
    return str(windowless) if windowless.is_file() else sys.executable


def _run_self(flag: str, **more: Any) -> None:
    here = Path(__file__).resolve()
    subprocess.Popen(
        [_helper_python(), str(here), flag],
        cwd=str(here.parent),
        **_quiet(close_fds=True, **more),
    )


def _start_helper() -> int:
    try:
        _run_self("--helper", env={**os.environ, "OVERSEER_OFFLINE_HELPER": "1"})
    except (OSError, ValueError) as e:
        _note(f"helper: it didn't start ({e})", loud=True)
        return 1
    return 0


def _riot_open() -> bool:
    # Broad, as the watch thread's one call that can fail, and the helper
    # never ends if that thread dies. Not knowing counts as open, so the
    # helper stays when in doubt.
    try:
        names = process_names()
    except Exception as e:
        _note(f"helper: the process list failed ({e!r}), so Riot counts as open")
        return True
    return any(exe.lower() in names for exe in _RIOT_EXES)


def _watch_riot(server: ThreadingHTTPServer) -> None:
    began = time.monotonic()
    seen = False
    closed_at: float | None = None
    while True:
        time.sleep(10)
        if _riot_open():
            seen, closed_at = True, None
            continue
        now = time.monotonic()
        if not seen or now - began < 90:
            continue
        closed_at = closed_at or now
        if now - closed_at >= 45:
            _note("helper: Riot and VALORANT are closed, so the helper ends")
            server.shutdown()
            return


_DOINGS: dict[str, Callable[[dict[str, Any]], dict[str, Any]]] = {
    "status": lambda _given: _status_here(),
    "launch": lambda given: _launch_here(given.get("status")),
    "switch": lambda given: _switch_here(enabled=bool(given.get("enabled", True))),
    "presence": lambda _given: {"private": _private_here()},
}


def _helper_main() -> int:
    key = secrets.token_urlsafe(32)

    class Questions(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def log_message(self, *_args: Any) -> None:
            """Quiet, as engine.log says what matters."""

        def answer(self, code: int, body: dict[str, Any]) -> None:
            data = json.dumps(body).encode("utf-8")
            self.send_response(code)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)

        def do_POST(self) -> None:
            sent = str(self.headers.get("X-Overseer-Key") or "")
            if self.path != "/" or not secrets.compare_digest(sent.encode(), key.encode()):
                self.answer(403, {"ok": False, "message": "Not for you."})
                return
            try:
                size = min(int(self.headers.get("Content-Length") or 0), 1 << 20)
                asked = json.loads(self.rfile.read(size).decode("utf-8"))
                doing = _DOINGS.get(str(asked.get("do")))
                if doing is None:
                    result = {"ok": False, "message": "The helper doesn't know that."}
                else:
                    result = doing(asked)
            except Exception as e:
                _note(f"helper: a question failed ({e!r})")
                result = {"ok": False, "message": str(e)}
            self.answer(200, result)

    server = ThreadingHTTPServer(("127.0.0.1", 0), Questions)
    _write_helper(server.server_address[1], key)
    _note(f"helper: up as {os.getpid()} on {server.server_address[1]}")
    threading.Thread(target=_watch_riot, args=(server,), name="offline-watch", daemon=True).start()
    try:
        server.serve_forever(poll_interval=0.5)
    finally:
        server.server_close()
        _forget_helper(key)
    return 0


def _helper_up() -> bool:
    if _ask_helper("status") is not None:
        return True
    with _HELPER_STARTING:
        if _ask_helper("status") is not None:
            return True
        try:
            _run_self("--start-helper")
        except (OSError, ValueError) as e:
            _note(f"helper: it couldn't be started ({e})", loud=True)
            return False
        until = time.monotonic() + 12
        while time.monotonic() < until:
            time.sleep(0.1)
            if _ask_helper("status", wait=1.0) is not None:
                return True
    return False


def launch(look: str | None = None) -> dict[str, Any]:
    """Restart the Riot Client through the chat proxy, showing friends `look`."""
    if _IS_HELPER:
        return _launch_here(look)
    if not _helper_up():
        return {"ok": False, "message": "Appear Offline couldn't start its helper."}
    return _ask_helper("launch", {"status": look}, wait=60.0) or {
        "ok": False,
        "message": "Appear Offline's helper stopped part way.",
    }


def set_enabled(*, enabled: bool) -> dict[str, Any]:
    """Hide you from friends, or show you again."""
    asked = None if _IS_HELPER else _ask_helper("switch", {"enabled": enabled})
    return asked if asked is not None else _switch_here(enabled=enabled)


def status() -> dict[str, Any]:
    """Return whether the proxy is running, carrying chat and hiding you."""
    asked = None if _IS_HELPER else _ask_helper("status")
    return asked if asked is not None else _status_here()


def captured_presence_private() -> dict[str, Any] | None:
    """Return your own VALORANT presence as the proxy last saw it, or None."""
    if _IS_HELPER:
        return _private_here()
    asked = _ask_helper("presence")
    private = asked.get("private") if asked else _private_here()
    return private if isinstance(private, dict) else None


def _check_stream() -> None:
    games = (
        "<games><valorant><st>in game</st><p>YWJj</p></valorant>"
        "<league_of_legends><st>online</st></league_of_legends>"
        "<keystone><st>online</st></keystone></games>"
    )
    mine = f'<presence from="x"><show>chat</show><status>hi</status>{games}</presence>'
    sent, held = hide_presences(mine)
    check(held == "", held)
    check("<show>offline</show>" in sent and "<status" not in sent, sent)
    check(not any(f"<{g}" in sent for g in ("valorant", "keystone", "league_of_legends")), sent)

    room = f"<presence to='room@muc' from='x'><show>chat</show>{games}</presence>"
    check(hide_presences(room) == (room, ""), "a room's presence changed")

    # Cut mid-stanza, nothing goes until the rest arrives, and then it is
    # disguised as one.
    first, rest = mine[:40], mine[40:]
    sent, held = hide_presences(first)
    check(sent == "" and held == first, (sent, held))
    sent, held = hide_presences(held + rest)
    check(held == "" and "<valorant" not in sent and "<keystone" not in sent, sent)
    sent, held = hide_presences("<iq/><pres")
    check((sent, held) == ("<iq/>", "<pres"), (sent, held))

    sent, held = hide_presences('<presence type="unavailable"/>')
    check(held == "" and sent.startswith("<presence"), sent)

    seen: list[str] = []
    hide_presences(mine, seen=seen.append)
    hide_presences(room, seen=seen.append)
    check(len(seen) == 1 and seen[0] == mine, seen)
    for look in ("away", "mobile"):
        sent, _ = hide_presences(mine, look)
        check(f"<show>{look}</show>" in sent and "<valorant" not in sent, (look, sent))
    sent, _ = hide_presences(mine, rewrite=False)
    check(sent == mine, "online changed a presence")


def _check_stand_in() -> None:
    roster = (
        b"<iq type='result'><query xmlns='jabber:iq:riotgames:roster'>"
        b"<item jid='real@pvp.net'/></query></iq>"
    )
    listed = present(add_stand_in(roster))
    check(listed.index(b"Valorant Overseer Active") < listed.index(b"real@pvp.net"))
    check(add_stand_in(b"<iq><nothing/></iq>") is None)

    to_it = f"<message to='{_STAND_IN_JID}' type='chat'><body>offline</body></message>"
    real = "<message to='someone@eu1.pvp.net' type='chat'><body>hi</body></message>"
    check(drop_stand_in(to_it) == "")
    check(drop_stand_in(to_it + real) == real)
    check(drop_stand_in(f"<iq to='{_STAND_IN_JID}' id='1'><q/></iq>{real}") == real)
    check(drop_stand_in(real + "<message to='x'") == real + "<message to='x'")

    blob = base64.b64encode(
        json.dumps({"partyPresenceData": {"partyClientVersion": "release-10.11"}}).encode()
    ).decode()
    check(
        _client_version(f"<presence><valorant><p>{blob}</p></valorant></presence>")
        == "release-10.11"
    )
    check(_client_version(_stand_in_presence("release-10.11", "away").decode()) == "release-10.11")
    check(_client_version("<presence><show>chat</show></presence>") is None)


def _check_cert() -> None:
    import tempfile
    from datetime import timedelta
    from unittest import mock

    from cryptography import x509
    from cryptography.hazmat.primitives import hashes
    from cryptography.hazmat.primitives.asymmetric import ec
    from cryptography.hazmat.primitives.serialization import Encoding, NoEncryption, pkcs12
    from cryptography.x509.oid import NameOID

    def made(name: str, *, began: int = -1, ends: int = 30) -> tuple[bytes, bytes]:
        # A certificate for `name` as a pfx and as PEM, in date from `began`
        # days from now to `ends` days from now.
        key = ec.generate_private_key(ec.SECP256R1())
        who = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, name)])
        now = datetime.now(UTC)
        cert = (
            x509.CertificateBuilder()
            .subject_name(who)
            .issuer_name(who)
            .public_key(key.public_key())
            .serial_number(x509.random_serial_number())
            .not_valid_before(now + timedelta(days=began))
            .not_valid_after(now + timedelta(days=ends))
            .add_extension(x509.SubjectAlternativeName([x509.DNSName(name)]), critical=False)
            .sign(key, hashes.SHA256())
        )
        pfx = pkcs12.serialize_key_and_certificates(None, key, cert, None, NoEncryption())
        return pfx, cert.public_bytes(Encoding.PEM)

    with mock.patch(f"{__name__}._note"):
        check(_pem_from(made(CHAT_DOMAIN)[0]) is not None, "the chat domain's own was refused")
        check(_pem_from(made("elsewhere.example")[0]) is None, "another name's was taken")
        check(_pem_from(made(CHAT_DOMAIN, began=-100, ends=-10)[0]) is None, "a lapsed one")
        check(_pem_from(made(CHAT_DOMAIN, began=5)[0]) is None, "one not yet in date")
        check(_pem_from(b"not a pfx") is None)

        with tempfile.TemporaryDirectory() as tmp:
            saved = Path(tmp) / "chat.pem"
            with (
                mock.patch(f"{__name__}._CERT", saved),
                mock.patch(f"{__name__}._fetch_cert", return_value=False),
            ):
                saved.write_bytes(made(CHAT_DOMAIN)[1])
                os.utime(saved, (0, 0))
                check(ensure_cert() == saved)
                saved.write_bytes(made(CHAT_DOMAIN, began=-100, ends=-10)[1])
                try:
                    lapsed: Path | None = ensure_cert()
                except RuntimeError:
                    lapsed = None
                check(lapsed is None, "a saved certificate that ran out was used")


def _check_dead_helper() -> None:
    import socket
    import tempfile
    from unittest import mock

    with socket.socket() as free:
        free.bind(("127.0.0.1", 0))
        port = free.getsockname()[1]
    with tempfile.TemporaryDirectory() as tmp:
        state = Path(tmp) / "helper.json"
        with mock.patch(f"{__name__}._HELPER_FILE", state):
            _write_helper(port, "gone")
            check(_ask_helper("status") is None and not state.exists(), "asked again")
            _write_helper(port, "newer")
            _forget_helper("gone")
            check(state.exists(), "a newer helper's file was deleted")
            state.write_text('{"speaks": 1, "port": 1, "key": "old"}', encoding="utf-8")
            check(_helper() is None, "an older version's helper was taken")


if __name__ == "__main__" and "--start-helper" in sys.argv:
    raise SystemExit(_start_helper())

if __name__ == "__main__" and "--helper" in sys.argv:
    raise SystemExit(_helper_main())

if __name__ == "__main__":
    _check_stream()
    _check_stand_in()
    _check_cert()
    _check_dead_helper()
    print("offline_launch self-check OK")

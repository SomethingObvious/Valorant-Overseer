# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""Every agent's id, role and colour, and a lookup by name or id.

The agents there were when this build was made are written below. One
Riot adds after is learned from valorant-api by `learn`, and kept on disk
so it is still known with no connection.
"""

from __future__ import annotations

import re
from typing import Any

import valapi
from common.check import check
from common.jsonstore import data_path, read_json, write_atomic

DUELIST = "Duelist"
INITIATOR = "Initiator"
CONTROLLER = "Controller"
SENTINEL = "Sentinel"

_AGENTS = {
    "Jett": ("add6443a-41bd-e414-f6ad-e58d267f4e95", DUELIST, "#9ADEFF"),
    "Phoenix": ("eb93336a-449b-9c1b-0a54-a891f7921d69", DUELIST, "#FE8266"),
    "Reyna": ("a3bfb853-43b2-7238-a4f1-ad90e9e46bcc", DUELIST, "#B565B5"),
    "Raze": ("f94c3b30-42be-e959-889c-5aa313dba261", DUELIST, "#FFA400"),
    "Yoru": ("7f94d92c-4234-0a36-9646-3a87eb8b5c89", DUELIST, "#2846C8"),
    "Neon": ("bb2a4828-46eb-8cd1-e765-15848195d751", DUELIST, "#00CFFF"),
    "Iso": ("0e38b510-41a8-5780-5e8f-568b2a4f2d6c", DUELIST, "#574AC2"),
    "Waylay": ("df1cb487-4902-002e-5c17-d28e83e78588", DUELIST, "#82C3EB"),
    "Sova": ("320b2a48-4d9b-a075-30f1-1f93a9b638fa", INITIATOR, "#3BA0E5"),
    "Breach": ("5f8d3a7f-467b-97f3-062c-13acf203c006", INITIATOR, "#C76B3B"),
    "Skye": ("6f2a04ca-43e0-be17-7f36-b3908627744d", INITIATOR, "#C0E69E"),
    "KAY/O": ("601dbbe7-43ce-be57-2a40-4abd24953621", INITIATOR, "#85929C"),
    "Fade": ("dade69b4-4f5a-8528-247b-219e5a1facd6", INITIATOR, "#5C5C5E"),
    "Gekko": ("e370fa57-4757-3604-3648-499e1f642d3f", INITIATOR, "#A8E65E"),
    "Tejo": ("b444168c-4e35-8076-db47-ef9bf368f384", INITIATOR, "#FFB761"),
    "Brimstone": ("9f0d8ba9-4140-b941-57d3-a7ad57c6b417", CONTROLLER, "#D1691F"),
    "Omen": ("8e253930-4c05-31dd-1b6c-968525494517", CONTROLLER, "#47508F"),
    "Viper": ("707eab51-4836-f488-046a-cda6bf494859", CONTROLLER, "#38C659"),
    "Astra": ("41fb69c1-4189-7b37-f117-bcaf1e96f1bf", CONTROLLER, "#712AE8"),
    "Harbor": ("95b78ed7-4637-86d9-7e41-71ba8c293152", CONTROLLER, "#008080"),
    "Clove": ("1dbf2edd-4729-0984-3115-daa5eed44993", CONTROLLER, "#F28FD0"),
    "Miks": ("7c8a4701-4de6-9355-b254-e09bc2a34b72", CONTROLLER, "#6B4BB0"),
    "Sage": ("569fdd95-4d10-43ab-ca70-79becc718b46", SENTINEL, "#26C8AF"),
    "Cypher": ("117ed9e3-49f3-6512-3ccf-0cada7e3823b", SENTINEL, "#E6D9C5"),
    "Killjoy": ("1e58de9c-4950-5125-93e9-a0aee9f98746", SENTINEL, "#FFD91F"),
    "Chamber": ("22697a3d-45bf-8dd7-4fec-84a9e28c69d7", SENTINEL, "#B89A46"),
    "Deadlock": ("cc8b64c8-4b25-4ff9-6e7f-37b4da43d235", SENTINEL, "#6677B0"),
    "Vyse": ("efba5359-4016-a1e5-7626-b1ae76895940", SENTINEL, "#656B8B"),
    "Veto": ("92eeef5d-43b5-1d4a-8d03-b3927a09034b", SENTINEL, "#2E8A96"),
}

AGENTS = [
    {
        "name": name,
        "uuid": uuid,
        "role": role,
        "color": color,
    }
    for name, (uuid, role, color) in _AGENTS.items()
]

AGENT_BY_NAME = {a["name"].lower(): a for a in AGENTS}
AGENT_BY_UUID = {a["uuid"].lower(): a for a in AGENTS}
UUID_TO_NAME = {uuid.lower(): name for name, (uuid, _r, _c) in _AGENTS.items()}

_BUILT_IN = frozenset(UUID_TO_NAME)
# Agents newer than this build, as valorant-api last listed them.
_LEARNED = data_path("learned-agents.json")
_HEX = re.compile(r"^[0-9a-fA-F]{6}")
_GREY = "#9AA1AB"


def _row(agent: Any) -> dict[str, str] | None:
    """One of valorant-api's agents as this table keeps it, or None without a name or id."""
    if not isinstance(agent, dict):
        return None
    name = str(agent.get("displayName") or "").strip()
    uuid = str(agent.get("uuid") or "").strip().lower()
    if not name or not uuid:
        return None
    role = str((agent.get("role") or {}).get("displayName") or "").strip() or "Agent"
    first = str((agent.get("backgroundGradientColors") or [""])[0])
    colour = "#" + first[:6].upper() if _HEX.match(first) else _GREY
    return {"name": name, "uuid": uuid, "role": role, "color": colour}


def _add(row: dict[str, str]) -> bool:
    """Add one agent to every table unless it is known, and say whether it was new."""
    name, uuid = row["name"], row["uuid"]
    if uuid in AGENT_BY_UUID or name.lower() in AGENT_BY_NAME:
        return False
    AGENTS.append(row)
    AGENT_BY_NAME[name.lower()] = row
    AGENT_BY_UUID[uuid] = row
    UUID_TO_NAME[uuid] = name
    return True


def learn() -> list[str]:
    """Add the agents valorant-api lists that this build doesn't, and return their names.

    With no answer, the ones it listed last time are added from disk.
    """
    listed = valapi.get("agents?isPlayableCharacter=true")
    if listed:
        rows = [r for r in map(_row, listed) if r and r["uuid"] not in _BUILT_IN]
        write_atomic(_LEARNED, rows, prefix=".agents-")
    else:
        saved = read_json(_LEARNED, [])
        rows = [r for r in map(_row_kept, saved if isinstance(saved, list) else []) if r]
    return [row["name"] for row in rows if _add(row)]


def _row_kept(row: Any) -> dict[str, str] | None:
    """Read back a row `learn` kept on disk, as valorant-api would have said it."""
    if not isinstance(row, dict):
        return None
    return _row(
        {
            "displayName": row.get("name"),
            "uuid": row.get("uuid"),
            "role": {"displayName": row.get("role")},
            "backgroundGradientColors": [str(row.get("color") or "").lstrip("#")],
        }
    )


def resolve_agent(identifier: str | None) -> dict[str, Any] | None:
    """Look an agent up by display name or UUID, ignoring case."""
    if not identifier:
        return None
    key = identifier.strip().lower()
    return AGENT_BY_NAME.get(key) or AGENT_BY_UUID.get(key)


def _self_check() -> None:
    import tempfile
    from pathlib import Path
    from unittest import mock

    newcomer = {
        "displayName": "Newcomer",
        "uuid": "0000AAAA-1111-2222-3333-444455556666",
        "role": {"displayName": "Initiator"},
        "backgroundGradientColors": ["ff8800ff", "000000ff"],
    }
    known = {"displayName": "Jett", "uuid": _AGENTS["Jett"][0], "role": {"displayName": "Duelist"}}
    with tempfile.TemporaryDirectory() as tmp:
        kept = str(Path(tmp) / "learned-agents.json")
        with (
            mock.patch(f"{__name__}._LEARNED", kept),
            mock.patch.object(valapi, "get", return_value=[known, newcomer, {"uuid": "x"}]),
        ):
            check(learn() == ["Newcomer"], "only the new one, and a nameless one is left out")
            check(learn() == [], "learned once")
        found = resolve_agent("newcomer")
        check(found is not None and found["color"] == "#FF8800" and found["role"] == "Initiator")

        # Offline, the next start learns it from what was kept.
        for table in (AGENT_BY_NAME, AGENT_BY_UUID, UUID_TO_NAME):
            table.pop("newcomer", None)
            table.pop("0000aaaa-1111-2222-3333-444455556666", None)
        AGENTS[:] = [a for a in AGENTS if a["name"] != "Newcomer"]
        with (
            mock.patch(f"{__name__}._LEARNED", kept),
            mock.patch.object(valapi, "get", return_value=None),
        ):
            check(learn() == ["Newcomer"], "from disk with no connection")
    check(
        _row({"displayName": "Odd", "uuid": "u", "backgroundGradientColors": ["zz"]})
        == {
            "name": "Odd",
            "uuid": "u",
            "role": "Agent",
            "color": _GREY,
        }
    )
    print("agents self-check OK (learned once, kept for offline, odd colours)")


if __name__ == "__main__":
    _self_check()

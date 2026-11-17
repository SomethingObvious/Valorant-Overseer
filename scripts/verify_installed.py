# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""Checks the installed packages match requirements.txt's pins exactly."""

from __future__ import annotations

import argparse
import importlib.metadata
import re
import sys
from pathlib import Path

_PIN = re.compile(r"([A-Za-z0-9._\[\]-]+)==([A-Za-z0-9._+!-]+)")


def _key(name: str) -> str:
    return re.sub(r"[-_.]+", "-", name).lower()


def read_lock(path: Path) -> dict[str, str]:
    """Return the pins in a requirements file, by package name."""
    pins: dict[str, str] = {}
    for number, line in enumerate(path.read_text(encoding="utf-8-sig").splitlines(), 1):
        text = line.partition("#")[0].strip()
        if not text or text.startswith("--hash="):
            continue
        text = text.rstrip("\\").strip()
        found = _PIN.fullmatch(text)
        if found is None:
            msg = f"{path}:{number}: {text!r} isn't a name==version pin"
            raise ValueError(msg)
        name, version = found.groups()
        if pins.setdefault(_key(name.partition("[")[0]), version) != version:
            msg = f"{path}:{number}: {name} is pinned twice, to different versions"
            raise ValueError(msg)
    if not pins:
        msg = f"{path} has no pins in it"
        raise ValueError(msg)
    return pins


def installed_versions() -> dict[str, str]:
    """Return every installed package's version, by package name."""
    return {
        _key(dist.metadata["Name"]): dist.version
        for dist in importlib.metadata.distributions()
        if dist.metadata.get("Name")
    }


def compare(pins: dict[str, str], installed: dict[str, str]) -> list[dict[str, str]]:
    """Return each pin the installed packages miss or get wrong."""
    return [
        {"package": name, "expected": wanted, "actual": installed.get(name, "missing")}
        for name, wanted in sorted(pins.items())
        if installed.get(name) != wanted
    ]


def main(argv: list[str] | None = None) -> int:
    """Check the pins and return 0 when they all match, else 1."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--requirements",
        type=Path,
        default=Path(__file__).resolve().parents[1] / "backend" / "requirements.txt",
    )
    args = parser.parse_args(argv)
    try:
        pins = read_lock(args.requirements)
        wrong = compare(pins, installed_versions())
    except (OSError, ValueError) as e:
        print(f"Couldn't check the installed packages: {e}", file=sys.stderr)
        return 1
    if not wrong:
        print(f"all {len(pins)} pinned packages are installed at their pinned versions")
        return 0
    print("Installed packages that don't match requirements.txt:", file=sys.stderr)
    for pin in wrong:
        print(
            f"  {pin['package']} {pin['actual']}, pinned at {pin['expected']}",
            file=sys.stderr,
        )
    return 1


if __name__ == "__main__":
    raise SystemExit(main())

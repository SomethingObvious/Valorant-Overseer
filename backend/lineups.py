# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""Lineups you add yourself, kept per map in `lineups/` at the install root.

Each is where to stand, where it lands, and a short clip of the throw. The
window's Lineups screen reads and writes them through the bridge, and the
command line at the bottom adds them too.

A clip comes from a link through yt-dlp or from a video you recorded, and
ffmpeg cuts it and sets its volume. Neither ships with Overseer. Both are
found on PATH, since yt-dlp needs updating every few weeks to keep up with
the sites it reads, and this build never updates anything itself.
"""

from __future__ import annotations

import hashlib
import os
import re
import secrets
import shutil
import subprocess
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from typing import Any
from urllib.parse import urlparse

import overseerlog
import valapi
from common.check import check
from common.jsonstore import data_path, read_json, write_atomic
from common.png import save_png

LOG = overseerlog.get_logger("lineups")

ROOT = Path(__file__).resolve().parent.parent / "lineups"
_MINIMAPS = Path(data_path("minimaps"))
_ICONS = Path(data_path("abilities"))
# Seconds of video downloaded either side of a clip, so nudging where it
# starts or stops doesn't mean another download.
PAD = 5.0
# Longer than this is a whole video pasted by mistake, not a throw.
LONGEST = 90.0
_SLUG = re.compile(r"^[a-z0-9][a-z0-9-]{0,80}$")
_KEYS = {"Grenade": "C", "Ability1": "Q", "Ability2": "E", "Ultimate": "X"}
_POOL = ThreadPoolExecutor(max_workers=4, thread_name_prefix="lineups")


class LineupError(Exception):
    """A lineup that can't be saved or played, said the way a player needs it."""


def slug(text: str) -> str:
    """Lowercase letters, digits and single hyphens, for a file name."""
    return re.sub(r"[^a-z0-9]+", "-", text.lower()).strip("-")[:80]


def seconds(text: Any) -> float:
    """Return a time as seconds, from 75, "75.5", "1:15" or "1:01:15"."""
    raw = str(text if text is not None else "").strip()
    if not re.fullmatch(r"\d+(?::\d{1,2}){0,2}(?:\.\d+)?", raw):
        msg = f"{raw or 'An empty time'} isn't a time. Write it like 1:15 or 75."
        raise LineupError(msg)
    total = 0.0
    for part in raw.split(":"):
        total = total * 60 + float(part)
    return total


def tools() -> dict[str, bool]:
    """Which of the three programs a clip needs are installed."""
    return {name: bool(_which(exe)) for name, exe in _TOOLS.items()}


_TOOLS = {"ytdlp": "yt-dlp", "ffmpeg": "ffmpeg", "ffplay": "ffplay"}
_GET_THEM = "Update Clip Tools in the window's settings installs it."
_MISSING = {
    "ytdlp": f"yt-dlp isn't installed, so clips can't come from a link. {_GET_THEM}",
    "ffmpeg": f"ffmpeg isn't installed, so clips can't be cut. {_GET_THEM}",
    "ffplay": f"ffplay isn't installed, so clips play without sound. {_GET_THEM}",
}
# Where winget puts the programs it installs: a link, or without Developer
# Mode the package folder itself. These come first, since they are the copies
# Update Clip Tools keeps current and an older one installed some other way
# can sit ahead of them on PATH, which a running backend also wouldn't have
# picked up yet.
_WINGET = Path(os.environ.get("LOCALAPPDATA", "")) / "Microsoft" / "WinGet"


def _which(exe: str) -> str | None:
    linked = _WINGET / "Links" / f"{exe}.exe"
    if linked.is_file():
        return str(linked)
    packages = _WINGET / "Packages"
    found = [*packages.glob(f"*/{exe}.exe"), *packages.glob(f"*/*/bin/{exe}.exe")]
    if found:
        return str(max(found, key=lambda path: path.stat().st_mtime))
    return shutil.which(exe)


def _download_failed(said: list[str]) -> str:
    """Return why yt-dlp failed, and the usual fix, since sites change faster than it does."""
    why = said[-1] if said else "no reason given"
    return (
        f"yt-dlp couldn't download that video: {why}. If this keeps happening, yt-dlp is "
        "probably out of date, and Update Clip Tools in the window's settings updates it."
    )


def _tool(name: str) -> str:
    found = _which(_TOOLS[name])
    if not found:
        raise LineupError(_MISSING[name])
    return found


def _image(url: str | None, path: Path) -> str | None:
    """Return a picture from valorant-api on disk, fetched the first time."""
    if path.exists():
        return str(path)
    if not url:
        return None
    try:
        save_png(url, path)
    except (OSError, ValueError) as e:
        LOG.warning("could not fetch %s to %s: %r", url, path, e)
        return None
    return str(path)


def _where(m: dict[str, Any], at: dict[str, Any]) -> list[float]:
    """Return a spot in the game's world as a point on the minimap, 0 to 1 each way."""
    # valorant-api's own formula: the world's y runs across the minimap and
    # its x runs down.
    x = float(at.get("y") or 0) * float(m["xMultiplier"]) + float(m["xScalarToAdd"])
    y = float(at.get("x") or 0) * float(m["yMultiplier"]) + float(m["yScalarToAdd"])
    return [round(x, 4), round(y, 4)]


def maps() -> list[dict[str, Any]]:
    """Every map with bomb sites: its minimap on disk, its sites, and its callouts."""
    standard = [
        m
        for m in valapi.get("maps") or []
        if m.get("tacticalDescription") and m.get("displayIcon") and m.get("xMultiplier")
    ]

    def one(m: dict[str, Any]) -> dict[str, Any]:
        name = m["displayName"]
        # Where each side starts, so the window can turn the map to put one of
        # them at the bottom.
        spawns = {
            c.get("superRegionName"): _where(m, c.get("location") or {})
            for c in m.get("callouts") or []
            if c.get("regionName") == "Spawn"
        }
        return {
            "name": name,
            "minimap": _image(m.get("displayIcon"), _MINIMAPS / f"{slug(name)}.png"),
            "sites": re.findall(r"[A-C](?=[/ ])", m.get("tacticalDescription") or ""),
            "callouts": [
                {
                    "name": f"{c.get('superRegionName')} {c.get('regionName')}".strip(),
                    "at": _where(m, c.get("location") or {}),
                }
                for c in m.get("callouts") or []
                if c.get("regionName") and "Spawn" not in (c.get("regionName") or "")
            ],
            # Minimap widths to a game unit, which turns an ability's reach
            # into a circle drawn to scale.
            "scale": abs(float(m["xMultiplier"])),
            "attack": spawns.get("Attacker Side"),
            "defend": spawns.get("Defender Side"),
        }

    return sorted(_POOL.map(one, standard), key=lambda m: m["name"])


def agents() -> list[dict[str, Any]]:
    """Every agent and the four abilities a lineup can be for, with their icons on disk."""
    out = []
    for a in valapi.get("agents?isPlayableCharacter=true") or []:
        name = a.get("displayName")
        if not name:
            continue
        kit = [ab for ab in a.get("abilities") or [] if ab.get("slot") in _KEYS]
        kit.sort(key=lambda ab: "CQEX".index(_KEYS[ab["slot"]]))
        out.append(
            {
                "name": name,
                "abilities": [
                    {
                        "slot": ab["slot"],
                        "key": _KEYS[ab["slot"]],
                        "name": ab.get("displayName"),
                        "icon": str(_ICONS / f"{slug(name)}-{ab['slot'].lower()}.png"),
                        "_url": ab.get("displayIcon"),
                    }
                    for ab in kit
                ],
            }
        )
    wanted = [ab for a in out for ab in a["abilities"] if not Path(ab["icon"]).exists()]
    list(_POOL.map(lambda ab: _image(ab["_url"], Path(ab["icon"])), wanted))
    for a in out:
        for ab in a["abilities"]:
            ab.pop("_url")
            if not Path(ab["icon"]).exists():
                ab["icon"] = None
    return sorted(out, key=lambda a: a["name"])


def _folder(map_name: Any) -> Path:
    name = slug(str(map_name or ""))
    if not _SLUG.match(name):
        msg = "A lineup needs a map."
        raise LineupError(msg)
    return ROOT / name


def listing() -> list[dict[str, Any]]:
    """Every saved lineup, with its clip's path when the clip is there."""
    out = []
    for path in sorted(ROOT.glob("*/*.json")):
        lineup = read_json(str(path), None)
        if not isinstance(lineup, dict) or lineup.get("id") != path.stem:
            continue
        out.append(_shown(lineup, path))
    return out


_KINDS = ("rect", "circle", "oval", "cone", "triangle", "text", "textbox", "brush")
# A stroke drawn across the whole map a few times over is still well under this.
_MOST_POINTS = 4000
_COLOUR = re.compile(r"^#[0-9A-Fa-f]{6}$")


def drawings() -> dict[str, list[dict[str, Any]]]:
    """Return the shapes drawn on each map, by map name."""
    out: dict[str, list[dict[str, Any]]] = {}
    for path in sorted(ROOT.glob("*/drawing.json")):
        kept = read_json(str(path), None)
        if isinstance(kept, dict) and kept.get("map") and isinstance(kept.get("shapes"), list):
            out[str(kept["map"])] = kept["shapes"]
    return out


def _shape(raw: Any) -> dict[str, Any]:
    """One shape as it is kept, or why it can't be."""
    if not isinstance(raw, dict) or raw.get("kind") not in _KINDS:
        msg = "A shape is a rectangle, a circle, an oval, a cone, a triangle, text or a stroke."
        raise LineupError(msg)
    colour = str(raw.get("colour") or "")
    if not _COLOUR.match(colour):
        msg = f"{colour or 'An empty colour'} isn't a colour like #FF4655."
        raise LineupError(msg)
    start, end = _point(raw.get("a"), "a shape starts"), _point(raw.get("b"), "a shape ends")
    if start is None or end is None:
        msg = "A shape needs both of its points."
        raise LineupError(msg)
    try:
        spread = round(min(180.0, max(5.0, float(raw.get("spread") or 60))), 1)
    except (TypeError, ValueError) as e:
        msg = "A cone's width is in degrees, like 60."
        raise LineupError(msg) from e
    # A shape kept before shapes had an opacity is a solid one.
    solid = raw.get("opacity")
    try:
        opacity = 1.0 if solid is None else round(min(1.0, max(0.1, float(solid))), 2)
    except (TypeError, ValueError) as e:
        msg = "A shape's opacity is a fraction, like 0.5."
        raise LineupError(msg) from e
    kept = {
        "kind": raw["kind"],
        "colour": colour.upper(),
        "a": start,
        "b": end,
        "spread": spread,
        "opacity": opacity,
    }
    if raw["kind"] == "text":
        words = " ".join(str(raw.get("text") or "").split())[:60]
        if not words:
            msg = "A label on the map needs some words."
            raise LineupError(msg)
        kept["text"] = words
    if raw["kind"] == "textbox":
        # Its lines stay as typed, each with its spaces tidied.
        lines = [" ".join(line.split()) for line in str(raw.get("text") or "").splitlines()]
        kept["text"] = "\n".join(lines).strip("\n")[:400]
    if raw["kind"] == "brush":
        points = _points(raw.get("points"))
        if not 2 <= len(points) <= _MOST_POINTS:
            msg = f"A stroke has 2 to {_MOST_POINTS} points, not {len(points)}."
            raise LineupError(msg)
        try:
            width = round(min(0.06, max(0.001, float(raw.get("width") or 0.008))), 4)
        except (TypeError, ValueError) as e:
            msg = "A stroke's width is a share of the map, like 0.008."
            raise LineupError(msg) from e
        kept["points"], kept["width"] = points, width
    return kept


def save_drawing(map_name: Any, shapes: Any) -> dict[str, Any]:
    """Keep a map's shapes, all of them at once, the way the window has them."""
    folder = _folder(map_name)
    if not isinstance(shapes, list):
        msg = "The shapes didn't come through as a list."
        raise LineupError(msg)
    kept = {"map": str(map_name), "shapes": [_shape(s) for s in shapes]}
    folder.mkdir(parents=True, exist_ok=True)
    if not write_atomic(str(folder / "drawing.json"), kept, prefix=".drawing-"):
        msg = f"Couldn't write the shapes for {map_name}."
        raise LineupError(msg)
    return kept


def _points(raw: Any) -> list[list[float]]:
    try:
        out = [[round(float(x), 4), round(float(y), 4)] for x, y in raw or []]
    except (TypeError, ValueError) as e:
        msg = "Those corners aren't points on the map."
        raise LineupError(msg) from e
    if any(not (0 <= v <= 1) for p in out for v in p):
        msg = "A corner is off the map."
        raise LineupError(msg)
    return out


def _point(raw: Any, what: str) -> list[float] | None:
    if raw is None:
        return None
    points = _points([raw])
    if not points:
        msg = f"Where {what} isn't a point on the map."
        raise LineupError(msg)
    return points[0]


# The best stream up to 1080p, counted on its short side so a short comes at
# its full 1080x1920. Past that the window's frames are no sharper, and 4K
# only makes the download and the decode slower.
_BEST = "bv*[height<=1920][width<=1920]+ba/b[height<=1920][width<=1920]/b"


# How long a download stays in the cache once nothing is using it.
_CACHE_DAYS = 1


def prune_cache(now: float | None = None) -> int:
    """Delete cached downloads more than a day old, and say how many went."""
    cutoff = (now or time.time()) - _CACHE_DAYS * 86400
    gone = 0
    for path in (ROOT / ".cache").glob("*.mp4"):
        try:
            if path.stat().st_mtime < cutoff:
                path.unlink()
                gone += 1
        except OSError:
            continue
    return gone


def _key(source: str) -> str:
    """Return a short name for a link, for the file its download goes in."""
    return hashlib.sha256(source.encode("utf-8")).hexdigest()[:12]


def _cached(source: str, start: float, end: float) -> tuple[Path, float] | None:
    """Return a download of `source` that already covers start to end, and where it begins."""
    key = _key(source)
    for path in (ROOT / ".cache").glob(f"{key}-hd-*.mp4"):
        found = re.fullmatch(rf"{key}-hd-(\d+)-(\d+)", path.stem)
        if found:
            begins, ends = int(found.group(1)) / 10, int(found.group(2)) / 10
            if begins <= start and end <= ends:
                return path, begins
    return None


def fetch(source: str, start: float, end: float) -> tuple[Path, float]:
    """Return the video to cut from, and the time in it where its first frame sits.

    A link downloads only start to end, with `PAD` either side, and a file
    you recorded is read where it is.
    """
    source = source.strip()
    local = Path(source)
    if source and not source.startswith(("http://", "https://")) and local.is_file():
        return local, 0.0
    parsed = urlparse(source)
    if parsed.scheme not in ("http", "https") or not parsed.netloc:
        msg = "Paste a link to the video, or the path of a video file on this PC."
        raise LineupError(msg)
    # The whole video, if it was downloaded to trim. Cutting from it is a
    # couple of seconds, where downloading the clip's part again took ten or so.
    whole = ROOT / ".cache" / f"{_key(source)}-watch-hd.mp4"
    if whole.exists():
        return whole, 0.0
    hit = _cached(source, start, end)
    if hit:
        return hit
    begins, ends = max(0.0, start - PAD), end + PAD
    key = _key(source)
    out = ROOT / ".cache" / f"{key}-hd-{round(begins * 10)}-{round(ends * 10)}.mp4"
    out.parent.mkdir(parents=True, exist_ok=True)
    ffmpeg = _tool("ffmpeg")
    command = [
        _tool("ytdlp"),
        "--no-playlist",
        "--quiet",
        "--no-warnings",
        "--no-progress",
        "--ffmpeg-location",
        str(Path(ffmpeg).parent),
        "-f",
        _BEST,
        "--merge-output-format",
        "mp4",
        # Cut on the exact second, so the clip's times line up with the link's.
        "--download-sections",
        f"*{begins:.2f}-{ends:.2f}",
        "--force-keyframes-at-cuts",
        "-o",
        str(out),
        "--",
        source,
    ]
    done = _run(command, 600)
    if done.returncode != 0 or not out.exists():
        said = (done.stderr or "").strip().splitlines()
        msg = _download_failed(said)
        raise LineupError(msg)
    return out, begins


def _run(command: list[str], timeout: int) -> subprocess.CompletedProcess[str]:
    try:
        return subprocess.run(
            command,
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            timeout=timeout,
            check=False,
            creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
        )
    except subprocess.TimeoutExpired as e:
        msg = f"{Path(command[0]).stem} took over {timeout} seconds and was stopped."
        raise LineupError(msg) from e
    except OSError as e:
        msg = f"{Path(command[0]).stem} couldn't start: {e}"
        raise LineupError(msg) from e


_LENGTHS: dict[str, float] = {}


def _clock(at: float) -> str:
    """Seconds as a clock, 151 as 2:31."""
    minutes, rest = divmod(round(at), 60)
    return f"{minutes}:{rest:02}"


def length(source: Any) -> float:
    """How long a video is in seconds.

    Asked of ffprobe for a file on this PC and of yt-dlp for a link. Kept,
    since a video's length doesn't change.
    """
    source = str(source or "").strip()
    if source in _LENGTHS:
        return _LENGTHS[source]
    local = Path(source)
    if source and not source.startswith(("http://", "https://")) and local.is_file():
        probe = _which("ffprobe") or str(Path(_tool("ffmpeg")).with_name("ffprobe.exe"))
        command = [probe, "-v", "error", "-show_entries", "format=duration"]
        done = _run([*command, "-of", "default=nw=1:nk=1", str(local)], 30)
    else:
        parsed = urlparse(source)
        if parsed.scheme not in ("http", "https") or not parsed.netloc:
            msg = "Paste a link to the video, or the path of a video file on this PC."
            raise LineupError(msg)
        command = [_tool("ytdlp"), "--no-playlist", "--quiet", "--no-warnings"]
        done = _run([*command, "--skip-download", "--print", "duration", "--", source], 60)
    try:
        found = float((done.stdout or "").strip().splitlines()[-1])
    except (IndexError, ValueError) as e:
        said = (done.stderr or "").strip().splitlines()
        msg = f"Couldn't tell how long that video is: {said[-1] if said else 'no length given'}"
        raise LineupError(msg) from e
    _LENGTHS[source] = found
    return found


def _times(clip: dict[str, Any]) -> tuple[float, float, int]:
    # No start is the start of the video, and no end is the end of it.
    given = str(clip.get("from") or "").strip()
    start = seconds(given) if given else 0.0
    given = str(clip.get("to") or "").strip()
    end = seconds(given) if given else length(clip.get("source"))
    if not given and end - start > LONGEST:
        msg = (
            f"That video runs {_clock(end)}. Drag the trim bar or write the times "
            f"to keep {round(LONGEST)} seconds of it or less."
        )
        raise LineupError(msg)
    if end <= start:
        msg = "The clip has to end after it starts."
        raise LineupError(msg)
    if end - start > LONGEST:
        msg = f"Keep a clip under {round(LONGEST)} seconds. It's a throw, not the video."
        raise LineupError(msg)
    raw = clip.get("volume")
    try:
        volume = 100 if raw is None else round(float(raw))
    except (TypeError, ValueError) as e:
        msg = "The volume is a percentage, like 60."
        raise LineupError(msg) from e
    return start, end, max(0, min(200, volume))


def cut(clip: dict[str, Any], out: Path) -> None:
    """Cuts `clip` from its source into `out`, at its volume."""
    start, end, volume = _times(clip)
    source, begins = fetch(str(clip.get("source") or ""), start, end)
    sound = (
        ["-af", f"volume={volume / 100:.2f}", "-c:a", "aac", "-b:a", "128k"] if volume else ["-an"]
    )
    part = out.with_name(out.stem + ".part.mp4")
    command = [
        _tool("ffmpeg"),
        "-y",
        "-v",
        "error",
        "-ss",
        f"{start - begins:.3f}",
        "-i",
        str(source),
        "-t",
        f"{end - start:.3f}",
        # The source's own size and close to its quality, since the point of
        # the clip is seeing exactly where the crosshair sits.
        "-c:v",
        "libx264",
        # veryfast cuts a 7 second 1080p clip in about 2 seconds, half what fast
        # took, and at this quality its file comes out smaller too.
        "-preset",
        "veryfast",
        "-crf",
        "17",
        *sound,
        "-movflags",
        "+faststart",
        str(part),
    ]
    done = _run(command, 300)
    if done.returncode != 0 or not part.exists():
        said = (done.stderr or "").strip().splitlines()
        msg = f"ffmpeg couldn't cut the clip: {said[-1] if said else 'no reason given'}"
        raise LineupError(msg)
    part.replace(out)


# The pictures a lineup can carry, and what they may start as. Each is kept
# as a PNG, since that is what the window reads.
_MOST_PICTURES = 8
_PICTURE_TYPES = (".png", ".jpg", ".jpeg", ".webp", ".bmp", ".gif")
# Wider than this is only memory, since the window shows them smaller.
_PICTURE_WIDTH = 1600
_LONGEST_DESCRIPTION = 4000


def _pictures(raw: Any, folder: Path, lineup_id: str, kept: list[str]) -> list[str]:
    """Return the pictures a lineup keeps, as file names in its folder.

    One it has already stays. One anywhere else on this PC is copied in as a
    PNG, made one by ffmpeg when it isn't one and shrunk to `_PICTURE_WIDTH`
    across. A picture it had and no longer lists is deleted.
    """
    names: list[str] = []
    for entry in raw[:_MOST_PICTURES] if isinstance(raw, list) else []:
        path = Path(str(entry or "").strip())
        if path.name in kept and (folder / path.name).exists():
            names.append(path.name)
            continue
        if not path.is_file() or path.suffix.lower() not in _PICTURE_TYPES:
            msg = f"Couldn't find a picture at {path}. Use a PNG or JPEG file on this PC."
            raise LineupError(msg)
        name = f"{lineup_id}-pic-{secrets.token_hex(3)}.png"
        out = folder / name
        folder.mkdir(parents=True, exist_ok=True)
        if _which(_TOOLS["ffmpeg"]):
            command = [
                _tool("ffmpeg"),
                "-y",
                "-v",
                "error",
                "-i",
                str(path),
                "-frames:v",
                "1",
                "-vf",
                f"scale='min({_PICTURE_WIDTH},iw)':-2",
                str(out),
            ]
            done = _run(command, 60)
            if done.returncode != 0 or not out.exists():
                msg = f"ffmpeg couldn't read the picture {path.name}."
                raise LineupError(msg)
        elif path.suffix.lower() == ".png":
            shutil.copyfile(path, out)
        else:
            msg = "ffmpeg isn't installed, so only PNG pictures can be added."
            raise LineupError(msg)
        names.append(name)
    for gone in set(kept) - set(names):
        (folder / gone).unlink(missing_ok=True)
    return names


def _shown(lineup: dict[str, Any], path: Path) -> dict[str, Any]:
    """Return a saved lineup as the window takes it.

    Its clip's file and its pictures go in as paths, each only when it is
    there.
    """
    shown = dict(lineup)
    clip = shown.get("clip") or {}
    video = path.with_suffix(".mp4")
    if clip and video.exists():
        shown["clip"] = {**clip, "file": str(video)}
    pictures = [path.parent / name for name in shown.get("images") or []]
    shown["images"] = [str(p) for p in pictures if p.exists()]
    return shown


def save(raw: Any, clip: Any = None) -> dict[str, Any]:
    """Save a lineup, cutting its clip first when one comes with it."""
    if not isinstance(raw, dict):
        msg = "That lineup is empty."
        raise LineupError(msg)
    folder = _folder(raw.get("map"))
    title = str(raw.get("title") or "").strip()
    agent = str(raw.get("agent") or "").strip()
    if not title or not agent:
        msg = "A lineup needs a title and an agent."
        raise LineupError(msg)
    lineup_id = str(raw.get("id") or "")
    if not _SLUG.match(lineup_id):
        lineup_id = f"{slug(agent)}-{slug(title)[:40]}-{secrets.token_hex(2)}"
    side = raw.get("side") if raw.get("side") in ("attack", "defense") else None
    lineup: dict[str, Any] = {
        "id": lineup_id,
        "map": str(raw["map"]),
        "agent": agent,
        "ability": str(raw.get("ability") or "") or None,
        "side": side,
        "site": raw.get("site") if raw.get("site") in ("A", "B", "C") else None,
        "title": title,
        "notes": str(raw.get("notes") or "").strip() or None,
        "description": str(raw.get("description") or "").strip()[:_LONGEST_DESCRIPTION] or None,
        "stand": _point(raw.get("stand"), "you stand"),
        "land": _point(raw.get("land"), "it lands"),
    }
    path = folder / f"{lineup_id}.json"
    kept = read_json(str(path), None) if path.exists() else None
    old_clip = (kept or {}).get("clip") if isinstance(kept, dict) else None
    if isinstance(clip, dict) and str(clip.get("source") or "").strip():
        start, end, volume = _times(clip)
        wanted = {"source": str(clip["source"]).strip(), "from": start, "to": end, "volume": volume}
        video = path.with_suffix(".mp4")
        if wanted != old_clip or not video.exists():
            folder.mkdir(parents=True, exist_ok=True)
            cut(wanted, video)
            # The whole video was only for trimming. The clip is cut now.
            (ROOT / ".cache" / f"{_key(str(wanted['source']))}-watch-hd.mp4").unlink(
                missing_ok=True
            )
        lineup["clip"] = wanted
    elif old_clip:
        lineup["clip"] = old_clip
    old_pictures = (kept or {}).get("images") if isinstance(kept, dict) else None
    lineup["images"] = _pictures(raw.get("images"), folder, lineup_id, list(old_pictures or []))
    folder.mkdir(parents=True, exist_ok=True)
    if not write_atomic(str(path), lineup, prefix=".lineup-"):
        msg = f"Couldn't write {path}."
        raise LineupError(msg)
    return _shown(lineup, path)


def delete(map_name: Any, lineup_id: Any) -> dict[str, Any]:
    """Remove a lineup, its clip and its pictures."""
    folder = _folder(map_name)
    name = str(lineup_id or "")
    if not _SLUG.match(name):
        msg = "That lineup doesn't exist."
        raise LineupError(msg)
    for suffix in (".json", ".mp4"):
        (folder / f"{name}{suffix}").unlink(missing_ok=True)
    for picture in folder.glob(f"{name}-pic-*.png"):
        picture.unlink(missing_ok=True)
    return {"deleted": name}


def watch(source: Any) -> dict[str, str]:
    """Return a file the window can play.

    A video on this PC as it is, or for a link a copy of the whole video at the
    best quality up to 1080p, downloaded once so the window can seek to any
    second of it straight away.
    """
    source = str(source or "").strip()
    local = Path(source)
    if source and not source.startswith(("http://", "https://")) and local.is_file():
        return {"source": source, "file": str(local.resolve())}
    parsed = urlparse(source)
    if parsed.scheme not in ("http", "https") or not parsed.netloc:
        msg = "Paste a link to the video, or the path of a video file on this PC."
        raise LineupError(msg)
    # ponytail: the whole video at up to 1080p, so an hour-long one is a few GB.
    # Fetch a window around the trim instead if long sources turn up.
    out = ROOT / ".cache" / f"{_key(source)}-watch-hd.mp4"
    if not out.exists():
        out.parent.mkdir(parents=True, exist_ok=True)
        command = [
            _tool("ytdlp"),
            "--no-playlist",
            "--quiet",
            "--no-warnings",
            "--no-progress",
            "--ffmpeg-location",
            str(Path(_tool("ffmpeg")).parent),
            "-f",
            _BEST,
            "--merge-output-format",
            "mp4",
            "-o",
            str(out),
            "--",
            source,
        ]
        done = _run(command, 900)
        if done.returncode != 0 or not out.exists():
            said = (done.stderr or "").strip().splitlines()
            msg = _download_failed(said)
            raise LineupError(msg)
    return {"source": source, "file": str(out)}


def overview() -> dict[str, Any]:
    """Everything the Lineups screen draws from."""
    return {
        "maps": maps(),
        "agents": agents(),
        "lineups": listing(),
        "drawings": drawings(),
        "tools": tools(),
    }


def _self_check() -> None:
    import tempfile
    from unittest import mock

    check(slug("A Main  Molly!") == "a-main-molly")
    check((seconds("75"), seconds("1:15"), seconds("1:01:15.5")) == (75, 75, 3675.5))
    for bad in ("", "-3", "1:75x", "abc"):
        try:
            seconds(bad)
        except LineupError:
            continue
        raise AssertionError(bad)

    # Ascent's own numbers put its A site's callout on the right of the map.
    ascent = {
        "xMultiplier": 7e-05,
        "yMultiplier": -7e-05,
        "xScalarToAdd": 0.813895,
        "yScalarToAdd": 0.573242,
    }
    check(_where(ascent, {"x": 6153.585, "y": -6626.2114}) == [0.3501, 0.1425])

    with (
        tempfile.TemporaryDirectory() as tmp,
        mock.patch(f"{__name__}.ROOT", Path(tmp)),
    ):
        runs: list[list[str]] = []

        def fake_run(command: list[str], _timeout: int) -> subprocess.CompletedProcess[str]:
            runs.append(command)
            # Each tool writes the file it was told to.
            Path(command[command.index("-o") + 1] if "-o" in command else command[-1]).write_bytes(
                b"x"
            )
            return subprocess.CompletedProcess(command, 0, "", "")

        with (
            mock.patch(f"{__name__}._run", side_effect=fake_run),
            mock.patch(f"{__name__}.shutil.which", side_effect=lambda exe: f"C:/tools/{exe}.exe"),
        ):
            saved = save(
                {
                    "map": "Ascent",
                    "agent": "Brimstone",
                    "title": "A Main Molly",
                    "stand": [0.4, 0.6],
                    "land": [0.5, 0.3],
                },
                {
                    "source": "https://localhost/v?id=1",
                    "from": "1:10",
                    "to": "1:17",
                    "volume": 60,
                },
            )
            download, trim = runs
            # The link goes after "--", so it can never be read as an option,
            # and only the clip with five seconds either side comes down.
            check(download[-2:] == ["--", "https://localhost/v?id=1"], download)
            check("*65.00-82.00" in download, download)
            # The cut starts five seconds into that download, at 60% volume.
            check(trim[trim.index("-ss") + 1] == "5.000" and "volume=0.60" in trim, trim)
            check(Path(saved["clip"]["file"]).exists(), saved)

            # Saving again with the same clip cuts nothing, and a nudge inside
            # the download cuts again without downloading.
            runs.clear()
            again = save(
                {**saved, "title": "A Main Molly"}, {**saved["clip"], "from": "1:10", "to": "1:17"}
            )
            check(again["id"] == saved["id"] and runs == [], runs)
            save(saved, {**saved["clip"], "from": "1:11", "to": "1:17"})
            check([Path(r[0]).stem for r in runs] == ["ffmpeg"], runs)

            for bad in ("file:///C:/Windows/win.ini", "-o evil", "javascript:alert(1)"):
                try:
                    fetch(bad, 1, 2)
                except LineupError:
                    continue
                raise AssertionError(bad)

        # The whole video for trimming goes once the clip is cut, and anything
        # in the cache a day old goes when the backend starts.
        cache = ROOT / ".cache"
        cache.mkdir(parents=True, exist_ok=True)
        fresh, stale = cache / "fresh.mp4", cache / "stale.mp4"
        fresh.write_bytes(b"x")
        stale.write_bytes(b"x")
        os.utime(stale, (time.time() - 3 * 86400, time.time() - 3 * 86400))
        check(prune_cache() == 1 and fresh.exists() and not stale.exists())
        fresh.unlink()

        # A picture comes in as a PNG in the lineup's folder, stays when the
        # next save lists it, goes when one doesn't, and a long description
        # is kept to its limit.
        source = Path(tmp) / "shot.png"
        source.write_bytes(b"x")
        with (
            mock.patch(f"{__name__}._run", side_effect=fake_run),
            mock.patch(f"{__name__}.shutil.which", side_effect=lambda exe: f"C:/tools/{exe}.exe"),
        ):
            pictured = save({**saved, "images": [str(source)], "description": "y" * 5000})
        (picture,) = pictured["images"]
        check(Path(picture).parent == ROOT / "ascent" and Path(picture).exists(), pictured)
        check(len(pictured["description"]) == _LONGEST_DESCRIPTION)
        check(save({**pictured})["images"] == [picture])
        check(save({**pictured, "images": []})["images"] == [] and not Path(picture).exists())
        try:
            save({**pictured, "images": [str(Path(tmp) / "missing.png")]})
        except LineupError:
            pass
        else:
            msg = "a picture that isn't there was kept"
            raise AssertionError(msg)

        check([lineup["id"] for lineup in listing()] == [saved["id"]])
        delete("Ascent", saved["id"])
        check(listing() == [])

        # Shapes keep their kind, colour and points, a cone's width is held
        # to what can be drawn, and anything else is refused with why.
        circle: dict[str, Any] = {
            "kind": "circle",
            "colour": "#ff4655",
            "a": [0.5, 0.5],
            "b": [0.6, 0.5],
        }
        cone = {
            "kind": "cone",
            "colour": "#18E5A7",
            "a": [0.2, 0.2],
            "b": [0.3, 0.3],
            "spread": 400,
            "opacity": 0.456,
        }
        faint = {**circle, "opacity": 0}
        label = {**circle, "kind": "text", "text": "  Stack\n here  "}
        boxed = {**circle, "kind": "textbox", "text": "Smoke  first\n\nthen   go"}
        stroke = {**circle, "kind": "brush", "points": [[0.1, 0.1], [0.2, 0.15]], "width": 1}
        save_drawing("Ascent", [circle, cone, faint, label, boxed, stroke])
        drawn = drawings()["Ascent"]
        check(
            [(s["kind"], s["colour"], s["spread"], s["opacity"]) for s in drawn]
            == [
                ("circle", "#FF4655", 60.0, 1.0),
                ("cone", "#18E5A7", 180.0, 0.46),
                ("circle", "#FF4655", 60.0, 0.1),
                ("text", "#FF4655", 60.0, 1.0),
                ("textbox", "#FF4655", 60.0, 1.0),
                ("brush", "#FF4655", 60.0, 1.0),
            ],
            drawn,
        )
        check(drawn[4]["text"] == "Smoke first\n\nthen go", drawn[4])
        check(drawn[5]["points"] == [[0.1, 0.1], [0.2, 0.15]] and drawn[5]["width"] == 0.06)
        check(drawn[3]["text"] == "Stack here" and "text" not in drawn[0], drawn)
        check(listing() == [], "a drawing is not a lineup")
        for wrong in (
            {**circle, "kind": "star"},
            {**circle, "colour": "red"},
            {**circle, "b": [2, 0]},
            {**circle, "opacity": "half"},
            {**label, "text": "   "},
            {**stroke, "points": [[0.1, 0.1]]},
            {**stroke, "points": [[0.1, 0.1], [1.5, 0.1]]},
        ):
            try:
                save_drawing("Ascent", [wrong])
            except LineupError:
                continue
            raise AssertionError(wrong)
    # Empty times are the whole video, measured once, and a whole video
    # past the limit says how long it runs.
    _LENGTHS["short.mp4"] = 12.5
    _LENGTHS["long.mp4"] = 151.0
    check(_times({"source": "short.mp4", "from": "", "to": None}) == (0.0, 12.5, 100))
    check(_times({"source": "short.mp4", "from": "0:02.5"})[:2] == (2.5, 12.5))
    check(length("short.mp4") == 12.5)
    said = ""
    try:
        _times({"source": "long.mp4"})
    except LineupError as e:
        said = str(e)
    check("2:31" in said, f"a whole video past the limit said {said!r}")
    _LENGTHS.clear()
    # A link is downloaded to watch once, and a file on this PC is watched as it is.
    with (
        tempfile.TemporaryDirectory() as tmp,
        mock.patch(f"{__name__}.ROOT", Path(tmp)),
    ):
        fetched: list[list[str]] = []

        def fake_fetch(command: list[str], _timeout: int) -> subprocess.CompletedProcess[str]:
            fetched.append(command)
            Path(command[command.index("-o") + 1]).write_bytes(b"x")
            return subprocess.CompletedProcess(command, 0, "", "")

        with (
            mock.patch(f"{__name__}._run", side_effect=fake_fetch),
            mock.patch(f"{__name__}.shutil.which", side_effect=lambda exe: f"C:/tools/{exe}.exe"),
        ):
            link = "https://localhost/v?id=2"
            copy, cached = watch(link)["file"], watch(link)["file"]
            check(copy == cached == str(ROOT / ".cache" / f"{_key(link)}-watch-hd.mp4"))
            check(len(fetched) == 1, fetched)
            check(fetched[0][-2:] == ["--", link], fetched)
            mine = ROOT / "mine.mp4"
            mine.write_bytes(b"x")
            check(watch(str(mine)) == {"source": str(mine), "file": str(mine.resolve())})
            check(len(fetched) == 1, "a file on this PC was downloaded")
            # Saving cuts from the video already there, without downloading.
            check(fetch(link, 3, 9) == (Path(copy), 0.0))
            check(len(fetched) == 1, "saving downloaded the clip again")
    print(
        "lineups self-check OK "
        "(times, map points, download window, cut, cache, pictures, shapes, watch)"
    )


if __name__ == "__main__":
    _self_check()

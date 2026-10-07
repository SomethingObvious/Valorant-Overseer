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

import base64
import contextlib
import hashlib
import json
import os
import re
import secrets
import shutil
import subprocess
import time
import unicodedata
import zlib
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from typing import TYPE_CHECKING, Any
from urllib.parse import urlparse

import overseerlog
import valapi
from common.check import check
from common.jsonstore import data_path, read_json, write_atomic
from common.png import save_png

if TYPE_CHECKING:
    from collections.abc import Callable

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
    except (TypeError, ValueError, OverflowError) as e:
        msg = "The volume is a percentage, like 60."
        raise LineupError(msg) from e
    return start, end, max(0, min(200, volume))


def cut(clip: dict[str, Any], out: Path, *, small: bool = False) -> None:
    """Cuts `clip` from its source into `out`, at its volume, and at 720p when `small`."""
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
        # 8-bit 4:2:0, since a 10-bit or 4:4:4 source cut as it is plays in
        # neither Windows' engine nor a GPU's decoder.
        "-pix_fmt",
        "yuv420p",
        # On a slow PC the short side stops at 720, which takes about a quarter
        # less of a graphics chip the game shares to play.
        *(
            ["-vf", "scale=w='if(gte(iw,ih),-2,min(iw,720))':h='if(gte(iw,ih),min(ih,720),-2)'"]
            if small
            else []
        ),
        # The window shows 30 frames a second, so a 60 fps clip only doubled
        # what it decoded. Slowed to a quarter, it shows 7.5 a second.
        "-fpsmax",
        "30",
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
    try:
        _patiently(lambda: part.replace(out))
    except PermissionError as e:
        part.unlink(missing_ok=True)
        msg = "Couldn't replace the old clip, as another program has it open. Close it, then save."
        raise LineupError(msg) from e


def _patiently(action: Callable[[], object]) -> None:
    """Do something to a file, waiting a moment while a player lets go of it.

    Windows' media engine closes a clip a little after it is told to stop.
    """
    for wait in (0.2, 0.4, 0.8):
        try:
            action()
        except PermissionError:
            time.sleep(wait)
        else:
            return
    action()


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


def save(raw: Any, clip: Any = None, *, small: bool = False) -> dict[str, Any]:
    """Save a lineup, cutting its clip first when one comes with it, at 720p when `small`."""
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
        # A bent wall's eight bends are the most any ability has.
        "points": _points(raw.get("points"))[:8],
    }
    path = folder / f"{lineup_id}.json"
    kept = read_json(str(path), None) if path.exists() else None
    old_clip = (kept or {}).get("clip") if isinstance(kept, dict) else None
    video = path.with_suffix(".mp4")
    if isinstance(clip, dict) and str(clip.get("source") or "").strip():
        start, end, volume = _times(clip)
        wanted = {"source": str(clip["source"]).strip(), "from": start, "to": end, "volume": volume}
        if wanted != old_clip or not video.exists():
            folder.mkdir(parents=True, exist_ok=True)
            cut(wanted, video, small=small)
            # The whole video was only for trimming. The clip is cut now. A
            # cut from it still running keeps it, and a day's pruning takes it.
            with contextlib.suppress(PermissionError):
                (ROOT / ".cache" / f"{_key(str(wanted['source']))}-watch-hd.mp4").unlink(
                    missing_ok=True
                )
        lineup["clip"] = wanted
    elif isinstance(clip, dict):
        # A clip with no source is one taken off, its file with it.
        _patiently(lambda: video.unlink(missing_ok=True))
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
    try:
        _patiently(lambda: (folder / f"{name}.mp4").unlink(missing_ok=True))
    except PermissionError as e:
        msg = "Couldn't delete the clip, as another program has it open. Close it and try again."
        raise LineupError(msg) from e
    (folder / f"{name}.json").unlink(missing_ok=True)
    for picture in folder.glob(f"{name}-pic-*.png"):
        picture.unlink(missing_ok=True)
    return {"deleted": name}


# A code is OVL1, the lineups as zlib'd JSON in base32, then a 9. Base32 is
# only letters and 2 to 7, so chat markdown can't eat any of it and a
# double-click picks the whole thing, and 9 can only be the end, so a code
# that got cut short says so.
_PREFIX, _END = "OVL1", "9"
_CODE = re.compile(rf"{_PREFIX}([A-Z2-7]+){_END}", re.IGNORECASE)
_SHARED = (
    "map",
    "agent",
    "ability",
    "side",
    "site",
    "title",
    "notes",
    "description",
    "stand",
    "land",
    "points",
)
# A map's lineups come to about 700 characters. These are far past any real
# code, and only stop a hostile one filling the disk or the memory.
_MOST_SHARED = 100
_MOST_UNPACKED = 256 * 1024
_LONGEST_TITLE = 120
_LONGEST_NOTES = 300
# A pasted clip is only cut from sites people post lineups on, since anything
# else has yt-dlp and ffmpeg fetching whatever a stranger's code points at.
_CLIP_SITES = (
    "youtube.com",
    "youtu.be",
    "twitch.tv",
    "medal.tv",
    "streamable.com",
    "x.com",
    "twitter.com",
    "tiktok.com",
)


# Clips for pasted lineups are cut one at a time on a thread of their own,
# so a code answers at once and its downloads never hold up the window's
# other requests. A real map's worth is under ten.
_CUTTER = ThreadPoolExecutor(max_workers=1, thread_name_prefix="lineup-cuts")
_MOST_CUT = 20


def _counted(n: int) -> str:
    return f"{n} lineup" if n == 1 else f"{n} lineups"


def _pack(lineups: list[Any]) -> str:
    """Return `lineups` as a code, the way `_unpack` reads one."""
    text = json.dumps(lineups, separators=(",", ":"), ensure_ascii=False)
    body = base64.b32encode(zlib.compress(text.encode(), 9)).decode().rstrip("=")
    return f"{_PREFIX}{body}{_END}"


def export_code(map_name: Any = None, ids: Any = None) -> dict[str, Any]:
    """Return a code for a map's lineups, or for those with these ids.

    It carries each clip's link and times and not the clip, so it is short
    enough for a chat message. A clip from a file on this PC stays out, since
    its path means nothing on another PC and names your folders.
    """
    wanted = {str(i) for i in ids} if isinstance(ids, list) else set()
    folder = slug(str(map_name or ""))
    out = []
    left = 0
    for path in sorted(ROOT.glob("*/*.json")):
        if path.stem not in wanted and path.parent.name != folder:
            continue
        lineup = read_json(str(path), None)
        # A map's drawing.json sits beside its lineups, and isn't one.
        if not isinstance(lineup, dict) or lineup.get("id") != path.stem:
            continue
        kept = {k: lineup[k] for k in _SHARED if lineup.get(k) not in (None, "", [])}
        clip = lineup.get("clip") or {}
        if _clip_site(str(clip.get("source") or "")):
            kept["clip"] = {k: clip[k] for k in ("source", "from", "to", "volume") if k in clip}
        elif clip:
            left += 1
        out.append(kept)
    if not out:
        msg = "There are no lineups here to share yet."
        raise LineupError(msg)
    code = _pack(out)
    said = [
        (
            f"Copied the code for {_counted(len(out))}, {len(code)} characters. "
            "Whoever you send it to copies it and picks Share, then Paste Code."
        )
    ]
    if left:
        clips = "clip stays" if left == 1 else "clips stay"
        said.append(f"{left} {clips} out, as they aren't from a video site.")
    return {"code": code, "said": " ".join(said)}


def _unpack(text: str) -> list[Any]:
    # Chats wrap long lines and some slip in invisible formatting characters,
    # so neither counts as part of the code.
    flat = "".join(c for c in text if not c.isspace() and unicodedata.category(c) != "Cf")
    found = _CODE.search(flat)
    if not found:
        msg = (
            "That code is cut short. Copy all of it, up to the 9 at the end."
            if _PREFIX.lower() in flat.lower()
            else "There's no lineup code on the clipboard. Copy one first."
        )
        raise LineupError(msg)
    body = found[1].upper()
    damaged = "That code is damaged. Copy it again from where it was sent."
    try:
        unpack = zlib.decompressobj()
        raw = unpack.decompress(base64.b32decode(body + "=" * (-len(body) % 8)), _MOST_UNPACKED)
        whole = unpack.eof and not unpack.unconsumed_tail
        shared = json.loads(raw) if whole else None
    except (ValueError, zlib.error, RecursionError) as e:
        raise LineupError(damaged) from e
    if not isinstance(shared, list) or len(shared) > _MOST_SHARED:
        raise LineupError(damaged)
    return shared


def _same(lineup: dict[str, Any]) -> str:
    """Return what makes two lineups the same one, whoever saved it."""
    keys = ("map", "agent", "title", "stand", "land")
    return json.dumps([str(lineup.get(k)).lower() for k in keys])


def _clip_site(source: str) -> bool:
    try:
        parsed = urlparse(source)
        host = (parsed.hostname or "").lower()
    except ValueError:
        return False
    return parsed.scheme in ("http", "https") and any(
        host == site or host.endswith(f".{site}") for site in _CLIP_SITES
    )


def _arrival(
    entry: Any, known: dict[str, str], kits: dict[str, Any]
) -> tuple[dict[str, Any], dict[str, Any] | None]:
    """Return a pasted entry as a lineup to save and the clip to cut for it.

    It is checked against the game's maps, agents and abilities, and its clip
    is only kept when it comes from a video site with both its times.
    """
    if not isinstance(entry, dict):
        msg = "That isn't a lineup."
        raise LineupError(msg)
    kit = kits.get(str(entry.get("agent") or "").lower())
    map_name = known.get(str(entry.get("map") or "").lower())
    if not kit or not map_name:
        msg = "That lineup is for a map or an agent the game doesn't have."
        raise LineupError(msg)
    raw = {k: entry.get(k) for k in _SHARED}
    raw["map"], raw["agent"] = map_name, kit["name"]
    ability = str(raw["ability"] or "")
    raw["ability"] = ability if ability in {ab["name"] for ab in kit["abilities"]} else None
    raw["title"] = str(raw["title"] or "")[:_LONGEST_TITLE]
    raw["notes"] = str(raw["notes"] or "")[:_LONGEST_NOTES]
    clip = entry.get("clip")
    if not isinstance(clip, dict) or "from" not in clip or "to" not in clip:
        return raw, None
    if not _clip_site(str(clip.get("source") or "")):
        return raw, None
    start, end, volume = _times(clip)
    return raw, {"source": str(clip["source"]).strip(), "from": start, "to": end, "volume": volume}


def _cut_later(path: Path, wanted: dict[str, Any], *, small: bool) -> None:
    """Cut a pasted lineup's clip on the cutter's thread.

    The lineup already keeps the clip's link and times, so one that fails is
    left for Edit then Save to try again, and one deleted meanwhile is skipped.
    """

    def run() -> None:
        if not path.exists():
            return
        try:
            cut(wanted, path.with_suffix(".mp4"), small=small)
        except (LineupError, OSError) as e:
            LOG.warning("couldn't cut the pasted clip for %s: %s", path.stem, e)
        if not path.exists():
            path.with_suffix(".mp4").unlink(missing_ok=True)

    _CUTTER.submit(run)


def import_code(text: Any, *, small: bool = False) -> dict[str, Any]:
    """Save the lineups in a code from `export_code`, and start cutting their clips.

    Each is checked the way the form checks one, and one you already have is
    left alone. It answers once the lineups are saved, and their clips are cut
    one at a time after, up to `_MOST_CUT` of them.
    """
    shared = _unpack(str(text or ""))
    known = {m["name"].lower(): m["name"] for m in maps()}
    kits = {a["name"].lower(): a for a in agents()}
    if not known or not kits:
        msg = "Overseer couldn't load the maps and agents to check that code against. Try again."
        raise LineupError(msg)
    have = {_same(lineup) for lineup in listing()}
    added: list[dict[str, Any]] = []
    already = unreadable = cutting = 0
    for entry in shared:
        try:
            raw, clip = _arrival(entry, known, kits)
            if _same(raw) in have:
                already += 1
                continue
            saved = save(raw)
        except (LineupError, TypeError, ValueError, OverflowError, RecursionError):
            unreadable += 1
            continue
        have.add(_same(raw))
        path = _folder(saved["map"]) / f"{saved['id']}.json"
        stored = read_json(str(path), None)
        if clip and cutting < _MOST_CUT and isinstance(stored, dict):
            stored["clip"] = clip
            write_atomic(str(path), stored, prefix=".lineup-")
            _cut_later(path, clip, small=small)
            saved = _shown(stored, path)
            cutting += 1
        added.append(saved)
    if not added and not already:
        msg = "None of the lineups in that code are ones Overseer can read."
        raise LineupError(msg)
    places = {lineup["map"] for lineup in added}
    if not added:
        said = ["You already have every lineup in that code."]
    elif len(places) == 1:
        said = [f"Added {_counted(len(added))} to {next(iter(places))}."]
    else:
        said = [f"Added {_counted(len(added))}."]
    if added and already:
        said.append(f"Left out {already} you already had.")
    if unreadable:
        said.append(f"Left out {unreadable} that couldn't be read.")
    if cutting == 1:
        said.append("Its clip is being cut in the background, which takes a few seconds.")
    elif cutting:
        said.append(f"Their {cutting} clips are being cut in the background, a few seconds each.")
    return {
        "added": added,
        "map": next(iter(places)) if len(places) == 1 else None,
        "cutting": cutting,
        "said": " ".join(said),
    }


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


def _check_codes(tmp: Path, fake_run: Any, runs: list[list[str]]) -> None:
    from unittest import mock

    def cut_all() -> None:
        # The cutter takes one job at a time, so one queued behind them waits.
        _CUTTER.submit(lambda: None).result()

    recording = tmp / "mine.mp4"
    recording.write_bytes(b"x")
    game = {
        "maps": [{"name": "Ascent"}],
        "agents": [{"name": "Brimstone", "abilities": [{"name": "Incendiary"}]}],
    }
    with (
        mock.patch(f"{__name__}._run", side_effect=fake_run),
        mock.patch(f"{__name__}.shutil.which", side_effect=lambda exe: f"C:/tools/{exe}.exe"),
        mock.patch(f"{__name__}.maps", return_value=game["maps"]),
        mock.patch(f"{__name__}.agents", return_value=game["agents"]),
        # The one site clips may come from here, so no outside host is named.
        mock.patch(f"{__name__}._CLIP_SITES", ("localhost",)),
    ):
        linked = {"source": "https://localhost/watch?v=abc", "from": "5", "to": "9"}
        first = save(
            {
                "map": "Ascent",
                "agent": "Brimstone",
                "ability": "Incendiary",
                "title": "B Short",
                "stand": [0.1, 0.2],
                "land": [0.3, 0.4],
            },
            linked,
        )
        recorded = {"map": "Ascent", "agent": "Brimstone", "title": "Mine", "stand": [0.5, 0.5]}
        mine = save(recorded, {"source": str(recording), "from": "1", "to": "3"})
        # A drawing sits beside the lineups and must not go out as one.
        save_drawing(
            "Ascent", [{"kind": "circle", "colour": "#ff4655", "a": [0.5, 0.5], "b": [0.6, 0.5]}]
        )

        # The code has no ids or pictures, and a clip from a file on this PC
        # stays out. Pasted with the message around it and wrapped, it reads.
        made = export_code("Ascent")
        code = made["code"]
        check("for 2 lineups" in made["said"] and "1 clip stays out" in made["said"], made)
        shared = _unpack(f"here you go\n{code[:40]}\n{code[40:]} have fun")
        check([s.get("clip", {}).get("source") for s in shared] == [linked["source"], None])
        check(not any("id" in s or "images" in s for s in shared), shared)
        check(_unpack(code.lower()) == shared)
        broken = code[:30] + ("B" if code[30] == "A" else "A") + code[31:]
        for bad, said in ((code[:-20], "cut short"), (broken, "damaged"), ("hi", "no lineup")):
            try:
                _unpack(bad)
            except LineupError as e:
                check(said in str(e), str(e))
                continue
            raise AssertionError(bad)

        check(import_code(code)["said"] == "You already have every lineup in that code.")
        delete("Ascent", first["id"])
        runs.clear()
        back = import_code(code)
        check(
            back["said"] == "Added 1 lineup to Ascent. Left out 1 you already had. "
            "Its clip is being cut in the background, which takes a few seconds.",
            back,
        )
        (arrived,) = back["added"]
        # It answers before the clip is cut, with the clip's link to cut from.
        check(arrived["id"] != first["id"] and back["cutting"] == 1, back)
        cut_all()
        cut_now = next(item for item in listing() if item["id"] == arrived["id"])
        check(Path(cut_now["clip"]["file"]).exists(), cut_now)
        # Cut from the part of the video the first save downloaded.
        check([Path(r[0]).stem for r in runs] == ["ffmpeg"], runs)

        # An id, a picture, a local clip or one from a site nobody posts
        # lineups on all stay out, and nothing is fetched for them. Odd values
        # count as unreadable rather than stopping the rest.
        hostile = [
            {
                "map": "ascent",
                "agent": "brimstone",
                "title": "x" * 999,
                "id": mine["id"],
                "images": [str(recording)],
                "stand": [0.9, 0.9],
                "clip": {"source": str(recording), "from": 1, "to": 2},
            },
            {"map": "Nowhere", "agent": "Brimstone", "title": "Lost"},
            {
                "map": "Ascent",
                "agent": "Brimstone",
                "title": "Router",
                "stand": [0.2, 0.2],
                "clip": {"source": "http://127.0.0.1:8080/v", "from": 1, "to": 2},
            },
            {
                "map": "Ascent",
                "agent": "Brimstone",
                "title": "Odd",
                "ability": [],
                "stand": [0.3, 0.3],
            },
            {
                "map": "Ascent",
                "agent": "Brimstone",
                "title": "Loud",
                "stand": [0.4, 0.4],
                "clip": {"source": "https://localhost/v", "from": 1, "to": 2, "volume": 1e400},
            },
            {
                "map": "Ascent",
                "agent": "Brimstone",
                "title": "Bad link",
                "stand": [0.6, 0.6],
                "clip": {"source": "https://localhost:[/v", "from": 1, "to": 2},
            },
            "not a lineup",
        ]
        runs.clear()
        took = import_code(_pack(hostile))
        cut_all()
        check(runs == [] and "Left out 3 that couldn't be read." in took["said"], took)
        long_one, router, odd, bad_link = took["added"]
        check(len(long_one["title"]) == _LONGEST_TITLE and long_one["images"] == [], long_one)
        check(long_one["id"] != mine["id"] and not long_one.get("clip") and not router.get("clip"))
        check(odd["ability"] is None and not bad_link.get("clip"), (odd, bad_link))
        check(Path(mine["clip"]["file"]).exists())
        bomb = zlib.compress(b"[" + b"0," * 5_000_000 + b"0]")
        deep = zlib.compress(b"[" * 3000 + b"]" * 3000)
        for hostile_code in (bomb, deep):
            try:
                _unpack(f"{_PREFIX}{base64.b32encode(hostile_code).decode().rstrip('=')}{_END}")
            except LineupError:
                continue
            msg = "a code that unpacks to 10 MB, or nests 3000 deep, was read"
            raise AssertionError(msg)

    # Without the tools the lineup still comes in, keeping the clip's link and
    # times so saving it later cuts the clip.
    delete("Ascent", arrived["id"])
    with (
        mock.patch(f"{__name__}.shutil.which", return_value=None),
        mock.patch(f"{__name__}._which", return_value=None),
        mock.patch(f"{__name__}.maps", return_value=game["maps"]),
        mock.patch(f"{__name__}.agents", return_value=game["agents"]),
        # The one site clips may come from here, so no outside host is named.
        mock.patch(f"{__name__}._CLIP_SITES", ("localhost",)),
    ):
        late = import_code(code)
        cut_all()
    (waiting,) = late["added"]
    stored = next(item for item in listing() if item["id"] == waiting["id"])
    check("being cut" in late["said"] and "file" not in stored["clip"], stored)
    check(stored["clip"]["source"] == linked["source"] and stored["clip"]["to"] == 9.0)

    # A clip with no source takes the clip off, file and all, where no clip
    # at all leaves it be.
    with (
        mock.patch(f"{__name__}._run", side_effect=fake_run),
        mock.patch(f"{__name__}.shutil.which", side_effect=lambda exe: f"C:/tools/{exe}.exe"),
    ):
        clipped = save(
            {"map": "Ascent", "agent": "Brimstone", "title": "Takes Off", "stand": [0.7, 0.7]},
            {"source": "https://localhost/v?id=9", "from": "1", "to": "2"},
        )
    video = Path(clipped["clip"]["file"])
    check(save(clipped)["clip"]["file"] == str(video) and video.exists())
    gone = save(clipped, {"source": ""})
    check("clip" not in gone and not video.exists(), gone)
    # A slow PC's clip is scaled down, and only then.
    with (
        mock.patch(f"{__name__}._run", side_effect=fake_run),
        mock.patch(f"{__name__}.shutil.which", side_effect=lambda exe: f"C:/tools/{exe}.exe"),
    ):
        for small in (False, True):
            runs.clear()
            save(
                {
                    "map": "Ascent",
                    "agent": "Brimstone",
                    "title": f"Small {small}",
                    "stand": [0.7, 0.2],
                },
                {"source": "https://localhost/v?id=8", "from": "1", "to": "2"},
                small=small,
            )
            scaled = [r for r in runs if Path(r[0]).stem == "ffmpeg" and "-vf" in r]
            check(bool(scaled) == small, runs)
    for lineup in listing():
        delete(lineup["map"], lineup["id"])


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
        _check_codes(Path(tmp), fake_run, runs)

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
        "(times, map points, download window, cut, cache, pictures, codes, shapes, watch)"
    )


if __name__ == "__main__":
    _self_check()

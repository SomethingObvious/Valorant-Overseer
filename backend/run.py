# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""The backend's launcher, which the window starts for itself."""

from __future__ import annotations

import _winapi
import contextlib
import ctypes
import faulthandler
import hashlib
import importlib.util
import itertools
import json
import msvcrt
import os
import re
import secrets
import signal
import socket
import string
import subprocess
import sys
import time
from collections import deque
from ctypes import wintypes
from dataclasses import dataclass
from pathlib import Path
from typing import IO, NoReturn

from common import env_number, load_env
from common.check import check
from common.system import NETSTAT, POWERSHELL, TASKKILL, image_of, parents

BACKEND = Path(__file__).resolve().parent
ROOT = BACKEND.parent
OVERSEER_DIR = ROOT / ".overseer"
_RUNTIME_STATE = OVERSEER_DIR / "runtime-state.json"
_CONSOLE_LOG = OVERSEER_DIR / "backend-console.log"
_INSIDE = (str(ROOT).rstrip("\\") + os.sep).lower()
_ASCII_LOWER = str.maketrans(string.ascii_uppercase, string.ascii_lowercase)

try:
    import overseerlog

    LOG = overseerlog.get_logger("launcher")
except ImportError:
    import logging

    LOG = logging.getLogger("launcher")
    LOG.addHandler(logging.NullHandler())


for _stream in (sys.stdout, sys.stderr):
    _reconfigure = getattr(_stream, "reconfigure", None)
    if _reconfigure is not None:
        _reconfigure(encoding="utf-8", errors="replace")

# What a ctypes call into kernel32 or user32 can raise: a missing export, an
# argument ctypes cannot convert, or a Windows error.
_CTYPES_ERRORS = (AttributeError, OSError, ctypes.ArgumentError)

try:
    _k32 = ctypes.windll.kernel32
    _k32.CreateMutexW.restype = wintypes.HANDLE
    _k32.CreateMutexW.argtypes = (wintypes.LPVOID, wintypes.BOOL, wintypes.LPCWSTR)
    _k32.WaitForSingleObject.restype = wintypes.DWORD
    _k32.WaitForSingleObject.argtypes = (wintypes.HANDLE, wintypes.DWORD)
    _k32.ReleaseMutex.argtypes = (wintypes.HANDLE,)
    _k32.CloseHandle.argtypes = (wintypes.HANDLE,)
except _CTYPES_ERRORS as e:
    LOG.warning("could not set up kernel32: %r", e)


def _tell(message: str) -> None:
    if "--prod" not in sys.argv:
        return
    try:
        ctypes.windll.user32.MessageBoxW(None, message, "Valorant Overseer", 0x10)
    except Exception as e:
        LOG.warning("the dialog didn't show: %r", e)


def die(code: str, msg: str) -> NoReturn:
    """Log `msg` under `code`, show it, and end the launch."""
    LOG.error("%s %s", code, msg)
    _tell(f"{code}: {msg}\n\nDetails: {OVERSEER_DIR / 'launcher.log'}")
    sys.exit(1)


def resolve_python() -> str:
    """Return the Python to run the backend with."""
    own = ROOT / ".venv" / "Scripts" / "python.exe"
    if own.exists():
        return str(own)
    if importlib.util.find_spec("websockets") is not None:
        return sys.executable
    die("VG-PY-001", "There is no Python environment in .venv. Run install.bat to set one up.")


_RUNTIME_OK = OVERSEER_DIR / "runtime-ok"


def _runtime_stamp(py: str) -> str:
    site = Path(py).parent.parent / "Lib" / "site-packages"
    try:
        names = sorted(n.name for n in site.iterdir() if n.name.endswith(".dist-info"))
        pins = (BACKEND / "requirements.txt").read_bytes()
    except OSError:
        return ""
    digest = hashlib.sha256(pins)
    for part in (py, *names):
        digest.update(part.encode("utf-8") + b"\0")
    return digest.hexdigest()


def forget_runtime() -> None:
    """Make the next launch check the packages again, after a backend that didn't start."""
    with contextlib.suppress(OSError):
        _RUNTIME_OK.unlink(missing_ok=True)


def validate_runtime(py: str) -> None:
    """Check the runtime's packages match the pins before anything starts."""
    stamp = _runtime_stamp(py)
    try:
        if stamp and _RUNTIME_OK.read_text(encoding="utf-8") == stamp:
            return
    except (OSError, ValueError):
        # Unreadable, or not UTF-8 after an edit. Either way it gets rewritten.
        pass
    _check_runtime(py)
    if stamp:
        with contextlib.suppress(OSError):
            OVERSEER_DIR.mkdir(exist_ok=True)
            _RUNTIME_OK.write_text(stamp, encoding="utf-8")


_CHECK_WAIT = 120


def _check_runtime(py: str) -> None:
    pins = BACKEND / "requirements.txt"
    repair = "Run install.bat to repair them. Your settings and data are kept."
    checks: tuple[tuple[Path, list[str], str], ...] = (
        (
            ROOT / "scripts" / "verify_installed.py",
            ["--requirements", str(pins)],
            "The installed packages don't match this release.",
        ),
        (
            ROOT / "scripts" / "import_smoke.py",
            [],
            "Some installed packages are broken or missing.",
        ),
    )
    for script, args, fault in checks:
        if not script.exists() or (args and not pins.exists()):
            continue
        try:
            ran = subprocess.run(
                [py, str(script), *args],
                capture_output=True,
                text=True,
                check=False,
                timeout=_CHECK_WAIT,
            )
        except subprocess.TimeoutExpired:
            # An antivirus scan holding the imports is slow, not broken, and
            # a check that never ends must not keep the app from starting.
            LOG.warning("%s took over %ss, so it was skipped", script.name, _CHECK_WAIT)
            continue
        if ran.returncode != 0:
            LOG.error("VG-DEPS-001 %s failed:\n%s", script.name, (ran.stderr or ran.stdout).strip())
            die("VG-DEPS-001", f"{fault} {repair}")


@dataclass
class _State:
    instance_lock: int | None = None


_STATE = _State()


def _path_fingerprint() -> str:
    # Has to agree with Get-FolderId in scripts/common.ps1, which lowers
    # ASCII letters alone, or the two sides name different mutexes.
    folded = str(ROOT).rstrip("\\").translate(_ASCII_LOWER)
    return hashlib.sha256(folded.encode("utf-8")).hexdigest()[:16].upper()


def _mutex_name(purpose: str) -> str:
    return rf"Local\Overseer-{purpose}-{_path_fingerprint()}"


def _end(pid: int, *, force: bool = True) -> None:
    try:
        subprocess.run(
            [TASKKILL, "/PID", str(pid), "/T", *(["/F"] if force else [])],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
            timeout=15,
        )
    except (OSError, subprocess.SubprocessError) as e:
        LOG.warning("ending %s failed: %r", pid, e)


def _parent_map() -> dict[int, int]:
    try:
        return parents()
    except OSError as e:
        LOG.warning("the process list failed: %r", e)
        return {}


def _mine(parent_of: dict[int, int]) -> set[int]:
    mine: set[int] = set()
    pid = os.getpid()
    while pid and pid not in mine and len(mine) < 5:
        mine.add(pid)
        pid = parent_of.get(pid, 0)
    return mine


def _python_commands() -> dict[int, str]:
    # A Toolhelp snapshot has no command lines, so this one question goes to WMI.
    query = (
        "[Console]::OutputEncoding = [Text.Encoding]::UTF8; "
        "Get-CimInstance Win32_Process -Filter \"Name like 'py%'\" | "
        "ForEach-Object { '{0} {1}' -f $_.ProcessId, $_.CommandLine }"
    )
    try:
        out = subprocess.run(
            [POWERSHELL, "-NoProfile", "-Command", query],
            capture_output=True,
            timeout=20,
            check=False,
        ).stdout.decode("utf-8", "replace")
    except (OSError, subprocess.SubprocessError) as e:
        LOG.warning("the running Pythons weren't listed: %r", e)
        return {}
    commands = {}
    for line in out.splitlines():
        pid, _, command = line.strip().partition(" ")
        if pid.isdigit():
            commands[int(pid)] = command.lower()
    return commands


def _ours(pid: int, parent_of: dict[int, int], commands: dict[int, str]) -> bool:
    # Two above, as the venv's python.exe starts the real interpreter, which
    # runs from wherever Python is installed.
    for _ in range(3):
        if not pid:
            return False
        if _INSIDE in f"{image_of(pid)} {commands.get(pid, '')}".lower():
            return True
        pid = parent_of.get(pid, 0)
    return False


def _recorded_pid() -> int:
    try:
        state = json.loads(_RUNTIME_STATE.read_text(encoding="utf-8"))
        return max(int(state.get("pid", 0)), 0)
    except FileNotFoundError:
        return 0
    except (OSError, ValueError, TypeError, AttributeError) as e:
        LOG.warning("runtime-state.json couldn't be read: %r", e)
        return 0


def _kill_leftover_instances() -> bool:
    parent_of = _parent_map()
    commands = _python_commands()
    script = str(BACKEND / "run.py").lower()
    leftover = {pid for pid, command in commands.items() if script in command}
    recorded = _recorded_pid()
    if recorded and _ours(recorded, parent_of, commands):
        leftover.add(recorded)
    leftover -= _mine(parent_of)
    for pid in leftover:
        LOG.info("ending pid %s, a launch left over from before", pid)
        _end(pid)
    return bool(leftover)


def acquire_instance_lock() -> bool:
    """Take the mutex that makes this launch the app."""
    kernel = ctypes.windll.kernel32
    try:
        handle = kernel.CreateMutexW(None, 0, _mutex_name("App"))
        # 0 and 0x80 are a mutex this launch now holds, the second because
        # the launch that held it died.
        wait = kernel.WaitForSingleObject(handle, 0) if handle else None
        if wait not in (None, 0, 0x80) and _kill_leftover_instances():
            wait = kernel.WaitForSingleObject(handle, 3000)
    except Exception:
        LOG.exception("could not create the app instance mutex")
        return False
    if not handle:
        LOG.error("could not create the app instance mutex: CreateMutexW failed")
        return False
    if wait not in (0, 0x80):
        kernel.CloseHandle(handle)
        return False
    _STATE.instance_lock = handle
    return True


def release_instance_lock() -> None:
    """Let go of the app mutex, if this launch holds it."""
    if not _STATE.instance_lock:
        return
    try:
        ctypes.windll.kernel32.ReleaseMutex(_STATE.instance_lock)
        ctypes.windll.kernel32.CloseHandle(_STATE.instance_lock)
    finally:
        _STATE.instance_lock = None


def write_runtime_state(ws_port: int) -> None:
    """Write this launch's pid and port, for maintenance to find the app by."""
    state = {"pid": os.getpid(), "wsPort": int(ws_port), "startedAt": int(time.time())}
    fresh = _RUNTIME_STATE.with_suffix(".tmp")
    try:
        OVERSEER_DIR.mkdir(exist_ok=True)
        fresh.write_text(json.dumps(state), encoding="utf-8")
        fresh.replace(_RUNTIME_STATE)
    except OSError:
        LOG.warning(
            "runtime-state.json wasn't written, which only maintenance misses", exc_info=True
        )


def clear_runtime_state() -> None:
    """Delete this launch's runtime state, once it stops."""
    with contextlib.suppress(OSError):
        _RUNTIME_STATE.unlink(missing_ok=True)


def _port_free(port: int) -> bool:
    try:
        probe = socket.create_server(("127.0.0.1", port))
    except OSError:
        return False
    probe.close()
    return True


def _port_holders(port: int) -> set[int]:
    try:
        out = subprocess.run(
            [NETSTAT, "-ano"], capture_output=True, timeout=15, check=False
        ).stdout.decode("ascii", "replace")
    except (OSError, subprocess.SubprocessError) as e:
        LOG.warning("netstat failed: %r", e)
        return set()
    # Picked out by the empty far end, as Windows translates the word LISTENING.
    listening = re.compile(
        rf"^\s*TCP\s+\S+:{port}\s+(?:0\.0\.0\.0|\[::\]):0\s+\S+\s+(\d+)\s*$", re.MULTILINE
    )
    return {int(pid) for pid in listening.findall(out)} - {0, 4, os.getpid()}


def _kill_our_stale(port: int) -> bool:
    holders = _port_holders(port)
    if not holders:
        return False
    parent_of, commands = _parent_map(), _python_commands()
    ended = False
    for pid in holders:
        if not _ours(pid, parent_of, commands):
            continue
        LOG.info("asking pid %s, a backend of ours left on port %s, to close", pid, port)
        _end(pid, force=False)
        ended = True
        until = time.monotonic() + 3
        while not _port_free(port) and time.monotonic() < until:
            time.sleep(0.15)
        if not _port_free(port):
            LOG.warning("pid %s didn't close when asked, so it is forced", pid)
            _end(pid)
    return ended


def choose_port(preferred: str | int, label: str) -> int:
    """Return a free port for `label`, `preferred` when it can be had."""
    preferred = int(preferred)
    if _port_free(preferred):
        return preferred
    if _kill_our_stale(preferred):
        for _ in range(20):
            if _port_free(preferred):
                return preferred
            time.sleep(0.25)
    holder = next((image_of(pid) or f"PID {pid}" for pid in _port_holders(preferred)), "")
    for alt in range(preferred + 1, preferred + 21):
        if _port_free(alt):
            LOG.warning(
                "VG-PORT-001 port %s (%s) busy (%s); using alternate %s",
                preferred,
                label,
                holder,
                alt,
            )
            return alt
    die(
        "VG-PORT-001",
        f"Ports {preferred}-{preferred + 20} ({label}) are all in use "
        f"(first held by {holder or 'another program'}). Close it, or set "
        f"WS_PORT in backend\\.env to a free port.",
    )


def wait_bridge(launch_id: str, timeout: float) -> bool:
    """Wait for the backend to publish .overseer/bridge.json for this launch."""
    # The file survives from the previous run, so it only counts once it names
    # this launch. The launch id is what matches: the venv's python.exe
    # re-execs, so the pid handed to Popen is not the pid the backend reports.
    bridge = OVERSEER_DIR / "bridge.json"
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            data = json.loads(bridge.read_text(encoding="utf-8"))
            if int(data.get("wsPort", 0)) > 0 and data.get("launchId") == launch_id:
                return True
        except (OSError, ValueError, TypeError, AttributeError):
            pass
        time.sleep(0.6)
    LOG.warning("the backend didn't write %s within %ss", bridge, int(timeout))
    return False


def _rotate(path: Path, keep: int = 5, over: int = 2 * 1024 * 1024) -> None:
    names = [path, *(path.with_name(f"{path.name}.{n}") for n in range(1, keep + 1))]
    try:
        if path.stat().st_size <= over:
            return
        for newer, older in reversed(list(itertools.pairwise(names))):
            if newer.exists():
                newer.replace(older)
    except OSError:
        pass


def backend_output(*, prod: bool) -> IO[str] | None:
    """Open the backend's console log, rotated, for a launch with --prod."""
    if not prod:
        return None
    try:
        OVERSEER_DIR.mkdir(exist_ok=True)
        _rotate(_CONSOLE_LOG)
        # Line buffered, so the last lines before a crash make it to disk.
        return _CONSOLE_LOG.open("a", encoding="utf-8", errors="replace", buffering=1)
    except OSError:
        return None


def tail_backend_log(lines: int = 12) -> str:
    """Return the backend console log's last `lines` lines, for an error message."""
    try:
        with _CONSOLE_LOG.open(encoding="utf-8", errors="replace") as log:
            last = deque(log, maxlen=lines)
    except OSError:
        return ""
    return "".join(last).rstrip("\n")


def stop(proc: subprocess.Popen[bytes]) -> None:
    """Stop the backend, politely first and then its whole process tree by force."""
    if proc.poll() is not None:
        return
    with contextlib.suppress(OSError, ValueError):
        proc.send_signal(signal.CTRL_BREAK_EVENT)
    with contextlib.suppress(subprocess.TimeoutExpired):
        proc.wait(timeout=2)
        return
    LOG.warning("pid %s ignored a graceful shutdown, so its process tree is forced", proc.pid)
    _end(proc.pid)
    with contextlib.suppress(subprocess.TimeoutExpired):
        proc.wait(timeout=1.5)


def first_to_end(*pids: int) -> int:
    """Wait for any of `pids` to end and return which, by its place in the call."""
    synchronize, inherit, wait_all = 0x00100000, False, False
    handles = []
    try:
        for at, pid in enumerate(pids):
            try:
                handles.append(_winapi.OpenProcess(synchronize, inherit, pid))
            except OSError:
                return at
        return _winapi.WaitForMultipleObjects(handles, wait_all, _winapi.INFINITE)
    finally:
        for handle in handles:
            _winapi.CloseHandle(handle)


def wait_window(parent: int | None) -> None:
    """Wait for the window after --parent to close, or until Ctrl+C without one."""
    if parent is None:
        # Not Event().wait(), which Ctrl+C can't interrupt on Windows.
        while True:
            time.sleep(3600)
    first_to_end(parent)


def _write_to(log: IO[str]) -> None:
    sys.stdout.flush()
    sys.stderr.flush()
    for std, fd in ((-11, 1), (-12, 2)):
        os.dup2(log.fileno(), fd)
        ctypes.windll.kernel32.SetStdHandle(std, msvcrt.get_osfhandle(fd))
    sys.stdout = sys.stderr = log


def serves_here(py: str) -> bool:
    """Whether `py` is the Python running this, so the backend can run in this process."""
    return Path(py).resolve() == Path(sys.executable).resolve()


def serve_here(py: str, ws_port: int, launch_id: str, log: IO[str] | None) -> None:
    """Start the backend in this process, or stop with why it didn't start."""
    os.environ["WS_PORT"] = str(ws_port)
    os.environ["OVERSEER_LAUNCH_ID"] = launch_id
    os.chdir(BACKEND)
    if log is not None:
        _write_to(log)
    # Nothing outside this process is left to notice a native crash, so it at
    # least leaves a stack in the log. The stamp goes until the bridge is up,
    # so a crash on the way there gets the packages checked next launch.
    faulthandler.enable(sys.stderr)
    stamp: bytes | None = None
    with contextlib.suppress(OSError):
        stamp = _RUNTIME_OK.read_bytes()
    forget_runtime()
    try:
        import app

        app.start()
    except (Exception, SystemExit):
        LOG.exception("VG-BACKEND-001 backend did not start")
        _check_runtime(py)
        die("VG-BACKEND-001", "The backend didn't start.")
    if stamp is not None:
        with contextlib.suppress(OSError):
            _RUNTIME_OK.write_bytes(stamp)


def parent_pid() -> int | None:
    """Return the pid after --parent, if one was given."""
    try:
        return int(sys.argv[sys.argv.index("--parent") + 1])
    except (ValueError, IndexError):
        return None


def main() -> None:
    """Run the backend until it stops or the window after --parent closes."""
    load_env(ROOT / ".env", BACKEND / ".env")
    parent = parent_pid()
    if not acquire_instance_lock():
        LOG.info("a backend is already being served, so this launch leaves it")
        return
    py = resolve_python()
    validate_runtime(py)
    proc = None
    served = False
    log = backend_output(prod="--prod" in sys.argv)
    details = (
        "See .overseer\\backend-console.log for details."
        if log is not None
        else "Its output is above."
    )
    try:
        ws_port = choose_port(
            int(env_number("WS_PORT", 7878, low=1, high=65535)), "WebSocket bridge"
        )
        env = os.environ.copy()
        env["WS_PORT"] = str(ws_port)
        launch_id = secrets.token_hex(8)
        env["OVERSEER_LAUNCH_ID"] = launch_id
        LOG.info("starting stack: ws=%s", ws_port)
        write_runtime_state(ws_port)
        if serves_here(py):
            serve_here(py, ws_port, launch_id, log)
            served = True
            wait_window(parent)
            LOG.info("the window that started this backend has closed, so it is shutting down")
            return
        proc = subprocess.Popen(
            [py, "app.py"],
            cwd=str(BACKEND),
            env=env,
            stdout=log,
            stderr=subprocess.STDOUT if log is not None else None,
            creationflags=subprocess.CREATE_NEW_PROCESS_GROUP,
        )
        if not wait_bridge(launch_id, 40):
            forget_runtime()
            LOG.error("VG-BACKEND-001 backend did not start; last output:\n%s", tail_backend_log())
            die("VG-BACKEND-001", f"The backend didn't start. {details}")
        if first_to_end(proc.pid, *([parent] if parent is not None else [])) == 1:
            LOG.info("the window that started this backend has closed, so it is shutting down")
            return
        proc.wait()
        forget_runtime()
        LOG.error(
            "VG-BACKEND-001 backend exited (code %s); last output:\n%s",
            proc.returncode,
            tail_backend_log(),
        )
        die(
            "VG-BACKEND-001",
            f"The backend stopped unexpectedly (exit {proc.returncode}). {details}",
        )
    finally:
        if proc is not None:
            stop(proc)
        if log is not None:
            with contextlib.suppress(OSError):
                log.close()
        release_instance_lock()
        clear_runtime_state()
        if served:
            os._exit(0)


def _report_crash() -> None:
    import traceback

    trace = traceback.format_exc()
    print(trace, file=sys.stderr)
    crash = OVERSEER_DIR / "crash.log"
    try:
        OVERSEER_DIR.mkdir(exist_ok=True)
        _rotate(crash)
        stamp = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
        with crash.open("a", encoding="utf-8") as log:
            log.write(f"\n--- {stamp} ---\n{trace}")
    except OSError as e:
        LOG.warning("%s wasn't written: %r", crash, e)
    last = trace.strip().rpartition("\n")[2]
    _tell(f"Valorant Overseer stopped with an error.\n\n{last}\n\nDetails were saved to:\n{crash}")


if __name__ == "__main__" and "--self-check" in sys.argv:
    import tempfile

    _real_dir = OVERSEER_DIR

    def _bridge(**fields: object) -> None:
        (OVERSEER_DIR / "bridge.json").write_text(json.dumps(fields), encoding="utf-8")

    with tempfile.TemporaryDirectory() as _tmp:
        OVERSEER_DIR = Path(_tmp)

        _bridge(wsPort=7878, pid=4242, launchId="an-older-launch")
        check(not wait_bridge("mine", 1.2), "a stale bridge was accepted")

        _bridge(wsPort=0, pid=999, launchId="mine")
        check(not wait_bridge("mine", 1.2), "a portless bridge was accepted")

        _site = Path(_tmp) / "venv" / "Lib" / "site-packages"
        _site.mkdir(parents=True)
        _py = str(Path(_tmp) / "venv" / "Scripts" / "python.exe")
        (_site / "requests-2.34.2.dist-info").mkdir()
        _before = _runtime_stamp(_py)
        check(_before and _before == _runtime_stamp(_py), "the same runtime, the same stamp")
        (_site / "requests-2.34.2.dist-info").rename(_site / "requests-2.35.0.dist-info")
        check(_runtime_stamp(_py) != _before, "an upgraded package went unnoticed")
        check(_runtime_stamp(str(Path(_tmp) / "nowhere" / "python.exe")) == "")

    OVERSEER_DIR = _real_dir

    _kept = None
    _bridge_path = OVERSEER_DIR / "bridge.json"
    if _bridge_path.exists():
        _kept = _bridge_path.read_bytes()
    _sock = socket.socket()
    _sock.bind(("127.0.0.1", 0))
    _spare = _sock.getsockname()[1]
    _sock.close()
    _launch = secrets.token_hex(8)
    _env = dict(os.environ)
    _env["WS_PORT"] = str(_spare)
    _env["OVERSEER_LAUNCH_ID"] = _launch
    # A real backend reads the local client and talks to Riot unless told
    # otherwise, and a test has no business doing that.
    _env["DATA_SOURCE"] = "demo"
    _proc = subprocess.Popen(
        [resolve_python(), "app.py"],
        cwd=str(BACKEND),
        env=_env,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.STDOUT,
    )
    try:
        check(
            wait_bridge(_launch, 45),
            "the launcher did not recognise a backend it started itself",
        )
    finally:
        _proc.terminate()
        try:
            _proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            _proc.kill()
        if _kept is not None:
            _bridge_path.write_bytes(_kept)
        elif _bridge_path.exists():
            _bridge_path.unlink()

    print("run self-check OK (a launch recognises its own backend)")
    raise SystemExit(0)

if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        pass
    except SystemExit:
        raise
    except Exception:
        _report_crash()
        sys.exit(1)

# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""The backend's launcher, which the window starts for itself.

It checks the install, keeps one backend running at a time, and stops it when
the window that started it closes.
"""

from __future__ import annotations

import contextlib
import ctypes
import hashlib
import json
import os
import secrets
import signal
import socket
import subprocess
import sys
import time
from ctypes import wintypes
from dataclasses import dataclass
from pathlib import Path
from typing import IO, NoReturn

from common import load_env
from common.check import check
from common.system import NETSTAT, POWERSHELL, TASKKILL

BACKEND = Path(__file__).resolve().parent
ROOT = BACKEND.parent
OVERSEER_DIR = ROOT / ".overseer"

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
    _k32.OpenProcess.restype = wintypes.HANDLE
    _k32.OpenProcess.argtypes = (wintypes.DWORD, wintypes.BOOL, wintypes.DWORD)
    _k32.QueryFullProcessImageNameW.argtypes = (
        wintypes.HANDLE,
        wintypes.DWORD,
        wintypes.LPWSTR,
        ctypes.POINTER(wintypes.DWORD),
    )
except _CTYPES_ERRORS as e:
    LOG.warning("could not set up kernel32: %r", e)


def die(code: str, msg: str) -> NoReturn:
    """Stop the launch with an error code, a dialog and the reason, and exit."""
    LOG.error("%s %s", code, msg)
    _fatal_dialog(f"{code}: {msg}")
    sys.exit(1)


def _fatal_dialog(message: str) -> None:
    if "--prod" not in sys.argv:
        return
    try:
        ctypes.windll.user32.MessageBoxW(
            None,
            f"{message}\n\nDetails: {OVERSEER_DIR / 'launcher.log'}",
            "Valorant Overseer",
            0x10,
        )
    except Exception as e:
        # Broad because this runs on the way out of die(), and a failure here
        # must not hide the error being reported.
        LOG.warning("could not show the fatal dialog: %r", e)


def venv_python() -> Path:
    """Return where this install's own Python would be."""
    return ROOT / ".venv" / "Scripts" / "python.exe"


def resolve_python() -> str:
    """Return the Python to run the backend with.

    This install's own, or the one running this script when it has the backend's
    packages, or else stop with how to fix it.
    """
    py = venv_python()
    if py.exists():
        return str(py)
    if (
        subprocess.run(
            [sys.executable, "-c", "import websockets"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
        ).returncode
        == 0
    ):
        return sys.executable
    die(
        "VG-PY-001",
        "There is no Python environment in .venv. Run install.bat to set one up.",
    )


def validate_runtime(py: str) -> None:
    """Check the runtime's packages match the pins before anything starts."""
    exact = ROOT / "scripts" / "verify_installed.py"
    requirements = BACKEND / "requirements.txt"
    if exact.exists() and requirements.exists():
        r = subprocess.run(
            [py, str(exact), "--requirements", str(requirements)],
            capture_output=True,
            text=True,
            check=False,
        )
        if r.returncode != 0:
            LOG.error(
                "VG-DEPS-001 exact dependency check failed:\n%s", (r.stderr or r.stdout).strip()
            )
            die(
                "VG-DEPS-001",
                "The installed packages don't match this release. "
                "Run install.bat to repair them. Your settings and data are kept.",
            )
    smoke = ROOT / "scripts" / "import_smoke.py"
    if not smoke.exists():
        return
    r = subprocess.run([py, str(smoke)], capture_output=True, text=True, check=False)
    if r.returncode != 0:
        LOG.error("VG-DEPS-001 import smoke failed:\n%s", r.stderr.strip())
        die(
            "VG-DEPS-001",
            "Some installed packages are broken or missing. "
            "Run install.bat to repair them. Your settings and data are kept.",
        )


@dataclass
class _State:
    """The mutex that says this launch is the app, once it holds it."""

    instance_lock: int | None = None


_STATE = _State()


def _path_fingerprint() -> str:
    # Must match Get-PathFingerprint in scripts/common.ps1, which lowercases
    # ASCII only, or the two sides name different mutexes.
    base = str(ROOT).rstrip("\\")
    lowered = "".join(chr(ord(c) + 32) if "A" <= c <= "Z" else c for c in base)
    return hashlib.sha256(lowered.encode("utf-8")).hexdigest()[:16].upper()


def _mutex_name(purpose: str) -> str:
    return rf"Local\Overseer-{purpose}-{_path_fingerprint()}"


def _my_process_tree() -> set[int]:
    mine = {os.getpid()}
    pid = os.getpid()
    for _ in range(4):
        _, _, ppid = _proc_info(pid)
        if not ppid:
            break
        mine.add(ppid)
        pid = ppid
    return mine


def _kill_leftover_instances() -> bool:
    mine = _my_process_tree()
    pids = set()
    try:
        state = json.loads((OVERSEER_DIR / "runtime-state.json").read_text(encoding="utf-8"))
        pid = int(state.get("pid", 0))
        if pid > 0 and pid not in mine and _is_ours(pid):
            pids.add(pid)
    except FileNotFoundError:
        pass
    except (OSError, ValueError, TypeError, AttributeError) as e:
        LOG.warning("unreadable runtime-state.json: %r", e)
    try:
        env = os.environ.copy()
        env["VS_MATCH"] = (str(BACKEND) + os.sep + "run.py").lower()
        out = subprocess.run(
            [
                POWERSHELL,
                "-NoProfile",
                "-Command",
                (
                    "$m = $env:VS_MATCH; Get-CimInstance Win32_Process -Filter "
                    "\"Name like 'py%'\" | Where-Object { $_.CommandLine -and "
                    "$_.CommandLine.ToLower().Contains($m) } | "
                    "ForEach-Object { $_.ProcessId }"
                ),
            ],
            capture_output=True,
            text=True,
            timeout=20,
            env=env,
            check=False,
        ).stdout
        for tok in out.split():
            if tok.isdigit() and int(tok) not in mine:
                pids.add(int(tok))
    # text=True decodes with the console code page, and output it can't
    # decode leaves stdout as None, which the parsing below trips on.
    except (OSError, subprocess.SubprocessError, AttributeError, TypeError) as e:
        LOG.warning("could not list leftover instances: %r", e)
    if not pids:
        return False
    for pid in pids:
        LOG.info("killing leftover instance pid=%s to take over", pid)
        subprocess.run(
            [TASKKILL, "/PID", str(pid), "/T", "/F"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
        )
    return True


def acquire_instance_lock() -> bool:
    """Take the mutex that makes this launch the app.

    A launch left over from before is stopped first. False when another launch
    still holds it, or the mutex couldn't be made.
    """
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
    try:
        OVERSEER_DIR.mkdir(exist_ok=True)
        path = OVERSEER_DIR / "runtime-state.json"
        temp = path.with_suffix(".tmp")
        temp.write_text(
            json.dumps(
                {
                    "pid": os.getpid(),
                    "wsPort": int(ws_port),
                    "startedAt": int(time.time()),
                }
            ),
            encoding="utf-8",
        )
        temp.replace(path)
    except OSError:
        LOG.warning("couldn't write runtime-state.json (non-fatal)", exc_info=True)


def clear_runtime_state() -> None:
    """Delete this launch's runtime state, once it stops."""
    with contextlib.suppress(OSError):
        (OVERSEER_DIR / "runtime-state.json").unlink(missing_ok=True)


def _pid_exe(pid: int) -> str:
    try:
        k32 = ctypes.windll.kernel32
        h = k32.OpenProcess(0x1000, 0, pid)
        if not h:
            return ""
        try:
            buf = ctypes.create_unicode_buffer(1024)
            size = ctypes.c_ulong(1024)
            if k32.QueryFullProcessImageNameW(h, 0, buf, ctypes.byref(size)):
                return buf.value
            return ""
        finally:
            k32.CloseHandle(h)
    except _CTYPES_ERRORS as e:
        LOG.warning("could not read the image name of pid %s: %r", pid, e)
        return ""


def _proc_info(pid: int) -> tuple[str, str, int]:
    try:
        lines = subprocess.run(
            [
                POWERSHELL,
                "-NoProfile",
                "-Command",
                (
                    f"$p = Get-CimInstance Win32_Process -Filter 'ProcessId={pid}'; "
                    "$p.ExecutablePath; $p.CommandLine; $p.ParentProcessId"
                ),
            ],
            capture_output=True,
            text=True,
            timeout=20,
            check=False,
        ).stdout.splitlines()
        lines += ["", "", ""]
        exe, cmd, ppid = lines[0].strip(), lines[1].strip(), lines[2].strip()
        return exe, cmd, int(ppid) if ppid.isdigit() else 0
    # text=True decodes with the console code page, and output it can't
    # decode leaves stdout as None, which the parsing below trips on.
    except (OSError, subprocess.SubprocessError, AttributeError, TypeError) as e:
        LOG.warning("could not look up pid %s: %r", pid, e)
        return "", "", 0


def _is_ours(pid: int) -> bool:
    prefix = (str(ROOT).rstrip("\\") + os.sep).lower()
    env = os.environ.copy()
    env["VS_PREFIX"] = prefix
    ps = (
        f"$cur = {pid}; foreach ($hop in 1..3) {{ "
        '$p = Get-CimInstance Win32_Process -Filter "ProcessId=$cur"; '
        "if (-not $p) { break }; "
        '$hay = ("$($p.ExecutablePath) $($p.CommandLine)").ToLower(); '
        "if ($hay.Contains($env:VS_PREFIX)) { 'VS_OURS'; break }; "
        "if (-not $p.ParentProcessId) { break }; "
        "$cur = $p.ParentProcessId }"
    )
    try:
        out = subprocess.run(
            [POWERSHELL, "-NoProfile", "-Command", ps],
            capture_output=True,
            text=True,
            timeout=20,
            env=env,
            check=False,
        ).stdout
    # text=True decodes with the console code page, and output it can't
    # decode leaves stdout as None, which the parsing below trips on.
    except (OSError, subprocess.SubprocessError, AttributeError, TypeError) as e:
        LOG.warning("could not tell whether pid %s is ours: %r", pid, e)
        return False
    else:
        return "VS_OURS" in out


def _port_free(port: int) -> bool:
    try:
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
            s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 0)
            s.bind(("127.0.0.1", int(port)))
    except OSError:
        return False
    else:
        return True


def _port_pids(port: int) -> set[int]:
    out = ""
    for proto in ("TCP", "TCPv6"):
        try:
            out += subprocess.run(
                [NETSTAT, "-ano", "-p", proto],
                capture_output=True,
                text=True,
                timeout=15,
                check=False,
            ).stdout
        # text=True decodes with the console code page, and output it can't
        # decode leaves stdout as None, which the parsing below trips on.
        except (OSError, subprocess.SubprocessError, AttributeError, TypeError) as e:
            LOG.warning("netstat -p %s failed: %r", proto, e)
    me = os.getpid()
    pids = set()
    for line in out.splitlines():
        parts = line.split()
        if (
            len(parts) >= 5
            and parts[1].endswith(f":{port}")
            and parts[2] in ("0.0.0.0:0", "[::]:0")
        ):
            try:
                pid = int(parts[4])
            except ValueError:
                continue
            if pid not in (0, 4, me):
                pids.add(pid)
    return pids


def _kill_our_stale(port: int) -> bool:
    killed = False
    root = str(ROOT).lower()
    prefix = root.rstrip("\\") + os.sep
    for pid in _port_pids(port):
        exe = _pid_exe(pid).lower()
        if exe == root or exe.startswith(prefix) or _is_ours(pid):
            LOG.info("closing our stale instance pid=%s on port %s", pid, port)
            subprocess.run(
                [TASKKILL, "/PID", str(pid), "/T"],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                check=False,
            )
            killed = True
            deadline = time.monotonic() + 3
            while time.monotonic() < deadline and not _port_free(port):
                time.sleep(0.15)
            if not _port_free(port):
                LOG.warning("stale pid %s ignored graceful shutdown; forcing it", pid)
                subprocess.run(
                    [TASKKILL, "/PID", str(pid), "/T", "/F"],
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                    check=False,
                )
    return killed


def choose_port(preferred: str | int, label: str) -> int:
    """Return a free port for `label`, `preferred` when it can be had.

    A stale backend of ours holding it is stopped first. Otherwise one of the
    twenty above it, with a warning naming what holds it, or stop when every one
    is taken.
    """
    preferred = int(preferred)
    if _port_free(preferred):
        return preferred
    if _kill_our_stale(preferred):
        for _ in range(20):
            if _port_free(preferred):
                return preferred
            time.sleep(0.25)
    holder = ""
    for pid in _port_pids(preferred):
        holder = _pid_exe(pid) or f"PID {pid}"
        break
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


def wait_bridge(launch_id: str, pid: int, timeout: float) -> bool:
    """Wait for the backend to publish .overseer/bridge.json for this launch."""
    # The file survives from the previous run, so it only counts once it names
    # this launch. The launch id is what matches: the venv's python.exe
    # re-execs, so the pid handed to Popen is not the pid the backend reports.
    # The pid still matches a backend too old to write a launch id.
    bridge = OVERSEER_DIR / "bridge.json"
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            data = json.loads(bridge.read_text(encoding="utf-8"))
            if int(data.get("wsPort", 0)) > 0 and (
                (launch_id and data.get("launchId") == launch_id) or int(data.get("pid", 0)) == pid
            ):
                return True
        except (OSError, ValueError, TypeError, AttributeError):
            # Missing, or not this launch's yet. Keep polling.
            pass
        time.sleep(0.6)
    LOG.warning("the backend didn't write %s within %ss", bridge, int(timeout))
    return False


def _rotate(path: Path, max_bytes: int = 2 * 1024 * 1024, backups: int = 5) -> None:
    try:
        if path.exists() and path.stat().st_size > max_bytes:
            for i in range(backups - 1, 0, -1):
                src = path.with_suffix(path.suffix + f".{i}")
                if src.exists():
                    src.replace(path.with_suffix(path.suffix + f".{i + 1}"))
            path.replace(path.with_suffix(path.suffix + ".1"))
    except OSError:
        pass


def backend_output(*, prod: bool) -> IO[str] | None:
    """Open the backend's console log, rotated, for a launch with --prod.

    Without --prod, or when it won't open, the backend prints here instead.
    """
    if not prod:
        return None
    try:
        OVERSEER_DIR.mkdir(exist_ok=True)
        log = OVERSEER_DIR / "backend-console.log"
        _rotate(log)
        return log.open("a", encoding="utf-8", errors="replace")
    except OSError:
        return None


def tail_backend_log(lines: int = 12) -> str:
    """Return the backend console log's last `lines` lines, for an error message."""
    try:
        text = (OVERSEER_DIR / "backend-console.log").read_text(encoding="utf-8", errors="replace")
        return "\n".join(text.splitlines()[-lines:])
    except OSError:
        return ""


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
    subprocess.run(
        [TASKKILL, "/PID", str(proc.pid), "/T", "/F"],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        check=False,
    )
    with contextlib.suppress(subprocess.TimeoutExpired):
        proc.wait(timeout=1.5)


def parent_alive(pid: int) -> bool:
    """Whether a process is still running, so the backend can follow its window."""
    synchronize, wait_timeout = 0x00100000, 0x102
    handle = ctypes.windll.kernel32.OpenProcess(synchronize, 0, pid)
    if not handle:
        return False
    try:
        return bool(ctypes.windll.kernel32.WaitForSingleObject(handle, 0) == wait_timeout)
    finally:
        ctypes.windll.kernel32.CloseHandle(handle)


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
        # Another backend is serving already, and the window will find it.
        LOG.info("a backend is already being served, so this launch leaves it")
        return
    py = resolve_python()
    validate_runtime(py)
    proc = None
    log = backend_output(prod="--prod" in sys.argv)
    details = (
        "See .overseer\\backend-console.log for details."
        if log is not None
        else "Its output is above."
    )
    try:
        ws_port = choose_port(os.environ.get("WS_PORT", "7878"), "WebSocket bridge")
        env = os.environ.copy()
        env["WS_PORT"] = str(ws_port)
        # Stamped into bridge.json so this launch can recognise its own.
        launch_id = secrets.token_hex(8)
        env["OVERSEER_LAUNCH_ID"] = launch_id
        LOG.info("starting stack: ws=%s", ws_port)
        write_runtime_state(ws_port)
        # Its own process group, so stop() can send it CTRL_BREAK alone.
        proc = subprocess.Popen(
            [py, "app.py"],
            cwd=str(BACKEND),
            env=env,
            stdout=log,
            stderr=subprocess.STDOUT if log is not None else None,
            creationflags=subprocess.CREATE_NEW_PROCESS_GROUP,
        )
        if not wait_bridge(launch_id, proc.pid, 40):
            LOG.error("VG-BACKEND-001 backend did not start; last output:\n%s", tail_backend_log())
            die("VG-BACKEND-001", f"The backend didn't start. {details}")
        while proc.poll() is None:
            time.sleep(0.5)
            if parent is not None and not parent_alive(parent):
                LOG.info("the window that started this backend has closed, so it is shutting down")
                return
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


def _report_crash() -> None:
    import traceback

    tb = traceback.format_exc()
    print(tb, file=sys.stderr)
    log = OVERSEER_DIR / "crash.log"
    try:
        log.parent.mkdir(exist_ok=True)
        _rotate(log)
        with log.open("a", encoding="utf-8") as fh:
            fh.write(f"\n--- {time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())} ---\n{tb}")
    except OSError as e:
        LOG.warning("could not write %s: %r", log, e)
    if "--prod" in sys.argv:
        try:
            last = tb.strip().splitlines()[-1]
            ctypes.windll.user32.MessageBoxW(
                None,
                f"Valorant Overseer stopped with an error.\n\n{last}\n\n"
                f"Details were saved to:\n{log}",
                "Valorant Overseer",
                0x10,
            )
        except Exception as e:
            # The crash is already on stderr and in crash.log. Nothing here
            # may raise over it.
            LOG.warning("could not show the crash dialog: %r", e)


if __name__ == "__main__" and "--self-check" in sys.argv:
    # The launcher decides whether anything runs at all, so its wait for the
    # bridge is checked against fixtures and against a real backend.
    import tempfile

    _real_dir = OVERSEER_DIR

    def _bridge(**fields: object) -> None:
        (OVERSEER_DIR / "bridge.json").write_text(json.dumps(fields), encoding="utf-8")

    with tempfile.TemporaryDirectory() as _tmp:
        OVERSEER_DIR = Path(_tmp)

        # A file left by the previous launch must not count as this one.
        _bridge(wsPort=7878, pid=4242, launchId="an-older-launch")
        check(not wait_bridge("mine", 999, 1.2), "a stale bridge was accepted")

        # The launch id is the answer, whatever the pid turned out to be.
        _bridge(wsPort=7878, pid=4242, launchId="mine")
        check(wait_bridge("mine", 999, 1.2), "the launch id was not matched")

        # And a backend too old to write one still matches on pid.
        _bridge(wsPort=7878, pid=999)
        check(wait_bridge("mine", 999, 1.2), "the pid fallback was not matched")

        # A bridge with no port is not a bridge.
        _bridge(wsPort=0, pid=999, launchId="mine")
        check(not wait_bridge("mine", 999, 1.2), "a portless bridge was accepted")

    OVERSEER_DIR = _real_dir

    # Only a real python, actually spawned, re-execs into a second process,
    # which is what breaks matching on pid. No fixture can stand in for it.
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
            wait_bridge(_launch, _proc.pid, 45),
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

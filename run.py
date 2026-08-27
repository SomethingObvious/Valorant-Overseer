from __future__ import annotations

import ctypes
import hashlib
import json
import os
import re
import secrets
import shutil
import signal
import socket
import subprocess
import sys
import time
from ctypes import wintypes
from pathlib import Path
from typing import IO, TYPE_CHECKING, NoReturn

if TYPE_CHECKING:
    from collections.abc import Iterable

ROOT = Path(__file__).resolve().parent
BACKEND = ROOT / "backend"
OVERSEER_DIR = ROOT / ".overseer"

sys.path.insert(0, str(BACKEND))
from common import load_env

try:
    import overseerlog

    LOG = overseerlog.get_logger("launcher")
except Exception:
    import logging

    LOG = logging.getLogger("launcher")
    LOG.addHandler(logging.NullHandler())


for _stream in (sys.stdout, sys.stderr):
    _reconfigure = getattr(_stream, "reconfigure", None)
    if _reconfigure is not None:
        _reconfigure(encoding="utf-8", errors="replace")

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
    _k32.GetStdHandle.restype = wintypes.HANDLE
    _k32.SetConsoleMode.argtypes = (wintypes.HANDLE, wintypes.DWORD)
    # ENABLE_PROCESSED_OUTPUT | ENABLE_WRAP_AT_EOL | ENABLE_VIRTUAL_TERMINAL_PROCESSING.
    # Without the last bit the ANSI colours below print as literal escapes.
    _k32.SetConsoleMode(_k32.GetStdHandle(-11), 7)
except Exception:
    pass

C_RED = "\033[38;5;203m"
C_TEAL = "\033[38;5;43m"
C_DIM = "\033[2m"
C_OK = "\033[38;5;78m"
C_WARN = "\033[38;5;214m"
C_END = "\033[0m"

ATTACHED = os.environ.get("VS_ATTACHED_CLI", "").strip() == "1"


def say(msg: str, color: str = C_TEAL) -> None:
    if ATTACHED:
        LOG.info("%s", msg)
        return
    print(f"{color}» {msg}{C_END}", flush=True)


def warn(msg: str) -> None:
    if ATTACHED:
        LOG.warning("%s", msg)
        return
    print(f"{C_WARN}! {msg}{C_END}", flush=True)


def die(code: str, msg: str) -> NoReturn:
    LOG.error("%s %s", code, msg)
    _fatal_dialog(f"{code}: {msg}")
    warn(f"{code}: {msg}")
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
    except Exception:
        pass


def venv_python() -> Path:
    return ROOT / ".venv" / "Scripts" / "python.exe"


def resolve_python() -> str:
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
        "No Python environment found (.venv is missing). "
        "Run install.bat to set up Valorant Overseer.",
    )


def validate_runtime(py: str) -> None:
    if os.environ.get("VS_PREVALIDATED", "").strip() == "1":
        return
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
                "Installed package versions do not match this release. "
                "Run install.bat to repair (your settings and data are kept).",
            )
    smoke = ROOT / "scripts" / "import_smoke.py"
    if not smoke.exists():
        return
    r = subprocess.run([py, str(smoke)], capture_output=True, text=True, check=False)
    if r.returncode != 0:
        LOG.error("VG-DEPS-001 import smoke failed:\n%s", r.stderr.strip())
        die(
            "VG-DEPS-001",
            "Installed packages are broken or missing. "
            "Run install.bat to repair (your settings and data are kept).",
        )


_INSTANCE_LOCK = None


def _path_fingerprint() -> str:
    base = str(Path(str(Path(__file__).resolve())).parent).rstrip("\\")
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
    except Exception:
        pass
    try:
        env = os.environ.copy()
        env["VS_MATCH"] = (str(ROOT).rstrip("\\") + os.sep + "run.py").lower()
        out = subprocess.run(
            [
                "powershell",
                "-NoProfile",
                "-Command",
                "$m = $env:VS_MATCH; Get-CimInstance Win32_Process -Filter "
                "\"Name like 'py%'\" | Where-Object { $_.CommandLine -and "
                "$_.CommandLine.ToLower().Contains($m) } | "
                "ForEach-Object { $_.ProcessId }",
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
    except Exception:
        pass
    if not pids:
        return False
    for pid in pids:
        say(f"A previous Valorant Overseer (PID {pid}) is still closing, taking over.", C_DIM)
        LOG.info("killing leftover instance pid=%s to take over", pid)
        subprocess.run(
            ["taskkill", "/PID", str(pid), "/T", "/F"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
        )
    return True


def acquire_instance_lock() -> bool:
    global _INSTANCE_LOCK
    try:
        handle = ctypes.windll.kernel32.CreateMutexW(None, False, _mutex_name("App"))
        if not handle:
            raise OSError("CreateMutexW failed")
        wait = ctypes.windll.kernel32.WaitForSingleObject(handle, 0)
        if wait not in (0, 0x80):
            if _kill_leftover_instances():
                wait = ctypes.windll.kernel32.WaitForSingleObject(handle, 3000)
            if wait not in (0, 0x80):
                ctypes.windll.kernel32.CloseHandle(handle)
                return False
        _INSTANCE_LOCK = handle
        return True
    except Exception:
        LOG.exception("could not create the app instance mutex")
        return False


def release_instance_lock() -> None:
    global _INSTANCE_LOCK
    if not _INSTANCE_LOCK:
        return
    try:
        ctypes.windll.kernel32.ReleaseMutex(_INSTANCE_LOCK)
        ctypes.windll.kernel32.CloseHandle(_INSTANCE_LOCK)
    finally:
        _INSTANCE_LOCK = None


def write_runtime_state(ws_port: int) -> None:
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
        Path(temp).replace(path)
    except OSError:
        LOG.warning("couldn't write runtime-state.json (non-fatal)", exc_info=True)


def clear_runtime_state() -> None:
    try:
        (OVERSEER_DIR / "runtime-state.json").unlink(missing_ok=True)
    except OSError:
        pass


def _pid_exe(pid: int) -> str:
    try:
        k32 = ctypes.windll.kernel32
        h = k32.OpenProcess(0x1000, False, pid)
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
    except Exception:
        return ""


def _proc_info(pid: int) -> tuple[str, str, int]:
    try:
        lines = subprocess.run(
            [
                "powershell",
                "-NoProfile",
                "-Command",
                f"$p = Get-CimInstance Win32_Process -Filter 'ProcessId={pid}'; "
                "$p.ExecutablePath; $p.CommandLine; $p.ParentProcessId",
            ],
            capture_output=True,
            text=True,
            timeout=20,
            check=False,
        ).stdout.splitlines()
        lines += ["", "", ""]
        exe, cmd, ppid = lines[0].strip(), lines[1].strip(), lines[2].strip()
        return exe, cmd, int(ppid) if ppid.isdigit() else 0
    except Exception:
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
            ["powershell", "-NoProfile", "-Command", ps],
            capture_output=True,
            text=True,
            timeout=20,
            env=env,
            check=False,
        ).stdout
        return "VS_OURS" in out
    except Exception:
        return False


def _port_free(port: int) -> bool:
    try:
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
            s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 0)
            s.bind(("127.0.0.1", int(port)))
        return True
    except OSError:
        return False


def _port_pids(port: int) -> set[int]:
    out = ""
    for proto in ("TCP", "TCPv6"):
        try:
            out += subprocess.run(
                ["netstat", "-ano", "-p", proto],
                capture_output=True,
                text=True,
                timeout=15,
                check=False,
            ).stdout
        except Exception:
            pass
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
            say(
                f"Port {port} is held by a previous Valorant Overseer instance "
                f"(PID {pid}), closing it.",
                C_DIM,
            )
            LOG.info("closing our stale instance pid=%s on port %s", pid, port)
            subprocess.run(
                ["taskkill", "/PID", str(pid), "/T"],
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
                    ["taskkill", "/PID", str(pid), "/T", "/F"],
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                    check=False,
                )
    return killed


def choose_port(preferred: str | int, label: str) -> int:
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
            warn(
                f"Port {preferred} ({label}) is in use by "
                f"{holder or 'another program'}, using port {alt} instead."
            )
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
    """Wait for the backend to publish .overseer/bridge.json for this launch.

    The file survives from the previous run, so its mere existence proves
    nothing: it has to be matched to the launch that just happened.

    That match used to be on pid, and on Windows it could never succeed. The
    venv's python.exe re-execs into a second process, so the pid handed to
    Popen and the pid the backend reports are different numbers, and the wait
    ran to its full forty seconds every single time before declaring a healthy
    backend dead. The launch id is passed in, so it says what pid meant to.
    """
    bridge = OVERSEER_DIR / "bridge.json"
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            data = json.loads(bridge.read_text(encoding="utf-8"))
            if int(data.get("wsPort", 0)) > 0 and (
                (launch_id and data.get("launchId") == launch_id) or int(data.get("pid", 0)) == pid
            ):
                return True
        except Exception:
            pass
        time.sleep(0.6)
    warn(f"Backend did not publish {bridge} within {int(timeout)}s.")
    return False


def node_cmd() -> str:
    executable = shutil.which("node.exe")
    if executable is None:
        warn("Node.js not found on PATH. Install Node.js 18.17+ from https://nodejs.org and retry.")
        sys.exit(1)
    try:
        version = (
            subprocess.run(
                [executable, "--version"], capture_output=True, text=True, timeout=10, check=True
            )
            .stdout.strip()
            .lstrip("v")
        )
        parts = tuple(int(re.sub(r"\D.*$", "", part) or 0) for part in version.split(".")[:3])
        if parts < (18, 17, 0):
            raise ValueError(version)
    except Exception:
        die(
            "VG-TUI-001",
            "The scoreboard requires Node.js 18.17 or newer. Upgrade Node.js and retry.",
        )
    return executable


def _rotate(path: Path, max_bytes: int = 2 * 1024 * 1024, backups: int = 5) -> None:
    try:
        if path.exists() and path.stat().st_size > max_bytes:
            for i in range(backups - 1, 0, -1):
                src = path.with_suffix(path.suffix + f".{i}")
                if src.exists():
                    Path(src).replace(path.with_suffix(path.suffix + f".{i + 1}"))
            Path(path).replace(path.with_suffix(path.suffix + ".1"))
    except OSError:
        pass


def backend_output(prod: bool) -> IO[str] | None:
    if not prod:
        return None
    try:
        OVERSEER_DIR.mkdir(exist_ok=True)
        log = OVERSEER_DIR / "backend-console.log"
        _rotate(log)
        return Path(log).open("a", encoding="utf-8", errors="replace")
    except OSError:
        return None


def tail_backend_log(lines: int = 12) -> str:
    try:
        text = (OVERSEER_DIR / "backend-console.log").read_text(encoding="utf-8", errors="replace")
        return "\n".join(text.splitlines()[-lines:])
    except OSError:
        return ""


TUI_BUNDLE = ROOT / "tui" / "dist" / "overseer.js"


def tui_args() -> list[str]:
    if not TUI_BUNDLE.exists():
        die(
            "VG-TUI-001",
            "tui\\dist\\overseer.js is missing. The release ships it prebuilt; "
            "from a source tree run: cd tui && npm install && npm run build",
        )
    node = node_cmd()
    extra = [a for a in sys.argv[1:] if a not in ("--cli", "--no-cli", "--prod")]
    return [node, str(TUI_BUNDLE), "--root", str(ROOT), *extra]


def run_cli() -> None:
    py = resolve_python()
    validate_runtime(py)
    say("Launching the scoreboard…", C_OK)
    subprocess.run(tui_args(), check=False)


def spawn_cli_window(py: str) -> subprocess.Popen[bytes] | None:
    try:
        args = tui_args()
        # Attached, the scoreboard takes over the console start.bat already
        # opened; detached, it needs one of its own.
        if ATTACHED:
            proc = subprocess.Popen(args)
        else:
            proc = subprocess.Popen(args, creationflags=subprocess.CREATE_NEW_CONSOLE)
        say("Scoreboard opened in a separate window.", C_OK)
        return proc
    except Exception as e:
        warn(f"Couldn't open the scoreboard ({e}). Run it manually with: python run.py --cli")
        return None


_CTRL_KILL_PIDS: list[int] = []
_CTRL_HANDLER_REF = None
_CLOSING = False


def _install_console_close_handler() -> None:
    if not ATTACHED:
        return
    global _CTRL_HANDLER_REF
    try:
        handler_routine = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.DWORD)

        def _handler(event: int) -> bool:
            if event in (2, 5, 6):
                global _CLOSING
                _CLOSING = True
                for pid in _CTRL_KILL_PIDS:
                    subprocess.run(
                        ["taskkill", "/PID", str(pid), "/T", "/F"],
                        stdout=subprocess.DEVNULL,
                        stderr=subprocess.DEVNULL,
                        check=False,
                    )
                try:
                    release_instance_lock()
                    clear_runtime_state()
                except Exception:
                    pass
            return False

        _CTRL_HANDLER_REF = handler_routine(_handler)
        ctypes.windll.kernel32.SetConsoleCtrlHandler(_CTRL_HANDLER_REF, True)
    except Exception:
        LOG.debug("couldn't install console close handler", exc_info=True)


def shutdown(
    procs: list[subprocess.Popen[bytes]],
    grouped: Iterable[subprocess.Popen[bytes]] = (),
) -> None:
    alive = [p for p in procs if p.poll() is None]
    grouped = set(grouped)
    for p in alive:
        try:
            if p in grouped:
                p.send_signal(signal.CTRL_BREAK_EVENT)
            else:
                subprocess.run(
                    ["taskkill", "/PID", str(p.pid), "/T"],
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                    check=False,
                )
        except Exception:
            LOG.debug("graceful stop failed for pid %s", p.pid, exc_info=True)

    deadline = time.monotonic() + 2
    for p in alive:
        try:
            p.wait(timeout=max(0.05, deadline - time.monotonic()))
        except subprocess.TimeoutExpired:
            pass

    stubborn = [p for p in alive if p.poll() is None]
    for p in stubborn:
        LOG.warning("pid %s ignored graceful shutdown; forcing its process tree", p.pid)
        try:
            subprocess.run(
                ["taskkill", "/PID", str(p.pid), "/T", "/F"],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                check=False,
            )
        except Exception:
            pass
    deadline = time.monotonic() + 1.5
    for p in stubborn:
        try:
            p.wait(timeout=max(0.05, deadline - time.monotonic()))
        except subprocess.TimeoutExpired:
            pass


def main() -> None:
    load_env(ROOT / ".env", BACKEND / ".env")

    if "--cli" in sys.argv:
        run_cli()
        return

    with_cli = "--no-cli" not in sys.argv
    prod = "--prod" in sys.argv

    if not acquire_instance_lock():
        LOG.info("second instance blocked")
        say("Valorant Overseer is already running or being installed/updated.", C_WARN)
        _fatal_dialog(
            "Valorant Overseer is already running or maintenance is in progress.\n\n"
            "Close the app or wait for install/update to finish, then try again."
        )
        return

    if not ATTACHED:
        print(f"{C_RED}{'=' * 58}{C_END}")
        print(f"{C_RED}  OVERSEER{C_END}  {C_DIM}live scoreboard · read-only{C_END}")
        print(f"{C_RED}{'=' * 58}{C_END}")

    source = os.environ.get("DATA_SOURCE", "auto")
    say("Live scoreboard reads your LOCAL VALORANT client, open the game and")
    say("join Agent Select / a match to see real ranks, names & parties.")
    say(f"With the game closed it waits, and shows nothing invented.  (DATA_SOURCE={source})")
    if os.environ.get("RIOT_API_KEY", "").strip():
        say("RIOT_API_KEY found (used by the legacy match-history endpoint).", C_OK)

    py = resolve_python()
    validate_runtime(py)

    procs = []
    roles = {}
    grouped = set()
    backend_log_fh = None
    try:
        if with_cli:
            cli_proc = spawn_cli_window(py)
            if cli_proc is not None:
                procs.append(cli_proc)
                roles[cli_proc] = "scoreboard"

        ws_port = choose_port(os.environ.get("WS_PORT", "7878"), "WebSocket bridge")

        child_env = os.environ.copy()
        child_env["WS_PORT"] = str(ws_port)
        # Stamped into bridge.json so this launch can recognise its own.
        launch_id = secrets.token_hex(8)
        child_env["OVERSEER_LAUNCH_ID"] = launch_id

        LOG.info("starting stack: ws=%s", ws_port)

        backend_log_fh = backend_output(prod)
        write_runtime_state(ws_port)
        say(f"Starting backend → ws://127.0.0.1:{ws_port}")
        # Its own process group, so shutdown() can send CTRL_BREAK to the
        # backend without it reaching this console as well.
        backend_proc = subprocess.Popen(
            [py, "app.py"],
            cwd=str(BACKEND),
            env=child_env,
            stdout=backend_log_fh,
            stderr=subprocess.STDOUT if backend_log_fh is not None else None,
            creationflags=subprocess.CREATE_NEW_PROCESS_GROUP,
        )
        procs.append(backend_proc)
        roles[backend_proc] = "backend"
        grouped.add(backend_proc)
        _CTRL_KILL_PIDS.append(backend_proc.pid)
        _install_console_close_handler()

        if not wait_bridge(launch_id, backend_proc.pid, 40):
            tail = tail_backend_log()
            LOG.error("VG-BACKEND-001 backend did not become healthy; last output:\n%s", tail)
            die(
                "VG-BACKEND-001",
                "The backend did not start. See .overseer\\backend-console.log for details.",
            )

        if not ATTACHED:
            print(f"\n{C_OK}Terminal scoreboard running. Press Ctrl+C to stop.{C_END}\n")
        stop = False
        while not stop:
            time.sleep(0.5)
            for p in procs:
                if p.poll() is None:
                    continue
                role = roles.get(p, "child")
                if role == "backend":
                    if _CLOSING or (ATTACHED and p.returncode in (0xC000013A, -1073741510)):
                        LOG.info("backend exited with console-close status; shutting down")
                        stop = True
                        break
                    tail = tail_backend_log()
                    LOG.error(
                        "VG-BACKEND-001 backend exited (code %s); last output:\n%s",
                        p.returncode,
                        tail,
                    )
                    die(
                        "VG-BACKEND-001",
                        f"The backend stopped unexpectedly (exit {p.returncode}). "
                        "See .overseer\\backend-console.log for details.",
                    )
                if role == "scoreboard":
                    say("Scoreboard closed, shutting down.", C_WARN)
                    LOG.info("scoreboard window closed; shutting down")
                    stop = True
                    break
    except KeyboardInterrupt:
        print()
        say("Shutting down…", C_WARN)
    finally:
        shutdown(procs, grouped)
        if backend_log_fh is not None:
            try:
                backend_log_fh.close()
            except OSError:
                pass
        release_instance_lock()
        clear_runtime_state()
        say("Bye.", C_DIM)


def _report_crash() -> None:
    import traceback

    tb = traceback.format_exc()
    print(tb, file=sys.stderr)
    log = OVERSEER_DIR / "crash.log"
    try:
        log.parent.mkdir(exist_ok=True)
        _rotate(log)
        with Path(log).open("a", encoding="utf-8") as fh:
            fh.write(f"\n--- {time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())} ---\n{tb}")
    except Exception:
        pass
    if "--prod" in sys.argv:
        try:
            last = tb.strip().splitlines()[-1]
            ctypes.windll.user32.MessageBoxW(
                None,
                f"Valorant Overseer couldn't start.\n\n{last}\n\nDetails were saved to:\n{log}",
                "Valorant Overseer",
                0x10,
            )
        except Exception:
            pass


if __name__ == "__main__" and "--self-check" in sys.argv:
    # Nothing here has ever tested the launcher, and the launcher is the part
    # that decides whether anything runs at all. It spent four launches in a
    # row declaring a healthy backend dead, forty seconds at a time, and every
    # other check in this repository passed while it did.
    import tempfile

    _real_dir = OVERSEER_DIR

    def _bridge(**fields: object) -> None:
        (OVERSEER_DIR / "bridge.json").write_text(json.dumps(fields), encoding="utf-8")

    # Two of the cases below are meant to time out, and the timeout says so on
    # the way past. Quiet it, so a passing check does not print warnings.
    _say_warn = warn

    def _quiet(msg: str) -> None:
        return

    warn = _quiet
    with tempfile.TemporaryDirectory() as _tmp:
        OVERSEER_DIR = Path(_tmp)

        # A file left by the previous launch must not count as this one.
        _bridge(wsPort=7878, pid=4242, launchId="an-older-launch")
        assert not wait_bridge("mine", 999, 1.2), "a stale bridge was accepted"

        # The launch id is the answer, whatever the pid turned out to be.
        _bridge(wsPort=7878, pid=4242, launchId="mine")
        assert wait_bridge("mine", 999, 1.2), "the launch id was not matched"

        # And a backend too old to write one still matches on pid.
        _bridge(wsPort=7878, pid=999)
        assert wait_bridge("mine", 999, 1.2), "the pid fallback was not matched"

        # A bridge with no port is not a bridge.
        _bridge(wsPort=0, pid=999, launchId="mine")
        assert not wait_bridge("mine", 999, 1.2), "a portless bridge was accepted"

    warn = _say_warn
    OVERSEER_DIR = _real_dir

    # The one that matters. Matching on pid failed only because the real
    # interpreter re-execs into a second process, which no hand written
    # fixture reproduces: it has to be the actual python, actually spawned.
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
    _env["DISCORD_RPC"] = "false"
    # This spawns a real backend. Without this it reads the local client and
    # talks to Riot, which a test has no business doing. Under the gate it
    # inherited demo mode by luck; run by hand it did not.
    _env["DATA_SOURCE"] = "demo"
    _proc = subprocess.Popen(
        [resolve_python(), "app.py"],
        cwd=str(BACKEND),
        env=_env,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.STDOUT,
    )
    try:
        assert wait_bridge(_launch, _proc.pid, 45), (
            "the launcher did not recognise a backend it started itself"
        )
    finally:
        _proc.terminate()
        try:
            _proc.wait(timeout=10)
        except Exception:
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

# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""Windows' own programs, by their full paths, and what is running on it."""

import ctypes
import functools
import os
from collections.abc import Iterator
from ctypes import wintypes
from pathlib import Path
from typing import Any

_SYSTEM = Path(os.environ.get("SYSTEMROOT", r"C:\Windows")) / "System32"

TASKKILL = str(_SYSTEM / "taskkill.exe")
NETSTAT = str(_SYSTEM / "netstat.exe")
POWERSHELL = str(_SYSTEM / "WindowsPowerShell" / "v1.0" / "powershell.exe")

# TH32CS_SNAPPROCESS
_ALL_PROCESSES = 0x2
_LOOK_AT = 0x1000


class _Process(ctypes.Structure):
    _fields_ = (
        ("size", wintypes.DWORD),
        ("usage", wintypes.DWORD),
        ("pid", wintypes.DWORD),
        ("heap", ctypes.c_size_t),
        ("module", wintypes.DWORD),
        ("threads", wintypes.DWORD),
        ("parent", wintypes.DWORD),
        ("priority", wintypes.LONG),
        ("flags", wintypes.DWORD),
        ("exe", wintypes.WCHAR * 260),
    )


@functools.cache
def _kernel() -> Any:
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    each = ctypes.POINTER(_Process)
    size = ctypes.POINTER(wintypes.DWORD)
    for name, args, gives in (
        ("CreateToolhelp32Snapshot", (wintypes.DWORD, wintypes.DWORD), wintypes.HANDLE),
        ("Process32FirstW", (wintypes.HANDLE, each), wintypes.BOOL),
        ("Process32NextW", (wintypes.HANDLE, each), wintypes.BOOL),
        ("OpenProcess", (wintypes.DWORD, wintypes.BOOL, wintypes.DWORD), wintypes.HANDLE),
        (
            "QueryFullProcessImageNameW",
            (wintypes.HANDLE, wintypes.DWORD, wintypes.LPWSTR, size),
            wintypes.BOOL,
        ),
        ("CloseHandle", (wintypes.HANDLE,), wintypes.BOOL),
    ):
        call = getattr(kernel, name)
        call.argtypes, call.restype = args, gives
    return kernel


def _processes() -> Iterator[_Process]:
    kernel = _kernel()
    snap = kernel.CreateToolhelp32Snapshot(_ALL_PROCESSES, 0)
    if snap in (None, ctypes.c_void_p(-1).value):
        raise OSError(ctypes.get_last_error(), "Windows wouldn't list the processes")
    try:
        one = _Process(size=ctypes.sizeof(_Process))
        more = kernel.Process32FirstW(snap, ctypes.byref(one))
        while more:
            yield one
            more = kernel.Process32NextW(snap, ctypes.byref(one))
    finally:
        kernel.CloseHandle(snap)


def process_names() -> set[str]:
    """Return the name of every running process, in lower case."""
    return {str(p.exe).lower() for p in _processes()}


def parents() -> dict[int, int]:
    """Return each running process's parent, by pid."""
    return {int(p.pid): int(p.parent) for p in _processes()}


def image_of(pid: int) -> str:
    """Return the full path of the program `pid` runs, or "" when it can't be told."""
    kernel = _kernel()
    handle = kernel.OpenProcess(_LOOK_AT, wintypes.BOOL(0), pid)
    if not handle:
        return ""
    try:
        path = ctypes.create_unicode_buffer(32768)
        size = wintypes.DWORD(len(path))
        return path.value if kernel.QueryFullProcessImageNameW(handle, 0, path, size) else ""
    finally:
        kernel.CloseHandle(handle)

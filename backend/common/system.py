# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""Windows' own programs, by their full paths, and what is running on it.

Named by full path rather than looked up on PATH, so nothing earlier on PATH
can stand in for one.
"""

import os
from pathlib import Path

_SYSTEM = Path(os.environ.get("SYSTEMROOT", r"C:\Windows")) / "System32"

TASKKILL = str(_SYSTEM / "taskkill.exe")
NETSTAT = str(_SYSTEM / "netstat.exe")
POWERSHELL = str(_SYSTEM / "WindowsPowerShell" / "v1.0" / "powershell.exe")


def process_names() -> set[str]:
    """Return the name of every running process, in lower case."""
    import ctypes
    from ctypes import wintypes

    class PROCESSENTRY32W(ctypes.Structure):
        _fields_ = [
            ("dwSize", wintypes.DWORD),
            ("cntUsage", wintypes.DWORD),
            ("th32ProcessID", wintypes.DWORD),
            ("th32DefaultHeapID", ctypes.c_size_t),
            ("th32ModuleID", wintypes.DWORD),
            ("cntThreads", wintypes.DWORD),
            ("th32ParentProcessID", wintypes.DWORD),
            ("pcPriClassBase", wintypes.LONG),
            ("dwFlags", wintypes.DWORD),
            ("szExeFile", wintypes.WCHAR * 260),
        ]

    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    snapshot_fn = kernel32.CreateToolhelp32Snapshot
    snapshot_fn.argtypes = (wintypes.DWORD, wintypes.DWORD)
    snapshot_fn.restype = wintypes.HANDLE
    first_fn = kernel32.Process32FirstW
    first_fn.argtypes = (wintypes.HANDLE, ctypes.POINTER(PROCESSENTRY32W))
    first_fn.restype = wintypes.BOOL
    next_fn = kernel32.Process32NextW
    next_fn.argtypes = (wintypes.HANDLE, ctypes.POINTER(PROCESSENTRY32W))
    next_fn.restype = wintypes.BOOL
    close_fn = kernel32.CloseHandle
    close_fn.argtypes = (wintypes.HANDLE,)
    close_fn.restype = wintypes.BOOL

    snapshot = snapshot_fn(0x00000002, 0)
    invalid = ctypes.c_void_p(-1).value
    if snapshot in (None, invalid):
        raise OSError(ctypes.get_last_error(), "CreateToolhelp32Snapshot failed")
    names = set()
    try:
        entry = PROCESSENTRY32W()
        entry.dwSize = ctypes.sizeof(entry)
        if first_fn(snapshot, ctypes.byref(entry)):
            while True:
                names.add(str(entry.szExeFile).lower())
                if not next_fn(snapshot, ctypes.byref(entry)):
                    break
    finally:
        close_fn(snapshot)
    return names

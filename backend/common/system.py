# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""Windows' own programs, by their full paths.

Named by full path rather than looked up on PATH, so nothing earlier on PATH
can stand in for one.
"""

import os
from pathlib import Path

_SYSTEM = Path(os.environ.get("SYSTEMROOT", r"C:\Windows")) / "System32"

TASKKILL = str(_SYSTEM / "taskkill.exe")
NETSTAT = str(_SYSTEM / "netstat.exe")
POWERSHELL = str(_SYSTEM / "WindowsPowerShell" / "v1.0" / "powershell.exe")

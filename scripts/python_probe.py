# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""Prints what a Python interpreter is, as JSON, for the installer to read."""

import json
import platform
import struct
import sys

print(
    json.dumps(
        {
            "implementation": platform.python_implementation(),
            "version": "{}.{}.{}".format(*sys.version_info[:3]),
            "machine": platform.machine(),
            "bits": struct.calcsize("P") * 8,
            "executable": sys.executable,
            "prefix": sys.prefix,
            "basePrefix": sys.base_prefix,
            "isVenv": sys.prefix != sys.base_prefix,
        }
    )
)

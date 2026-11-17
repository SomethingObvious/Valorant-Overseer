# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""Prints what a Python interpreter is, as one line of JSON, for the installer."""

import json
import platform
import sys

facts = {
    "kind": platform.python_implementation(),
    "version": platform.python_version(),
    "machine": platform.machine(),
    "bits": 64 if sys.maxsize > 2**32 else 32,
    "exe": sys.executable,
    "home": sys.prefix,
    "base": sys.base_prefix,
}
print(json.dumps(facts))

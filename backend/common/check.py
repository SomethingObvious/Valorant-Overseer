# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""The test the module self-checks make, kept under ``python -O``.

An ``assert`` statement vanishes when Python runs optimised, which would leave
a self-check that passes by checking nothing.
"""


def check(ok: object, why: object = "") -> None:
    """Fail the self-check with ``why`` unless ``ok`` is true."""
    if not ok:
        raise AssertionError(why)


def present[T](value: T | None, why: object = "") -> T:
    """Fail the self-check with ``why`` if ``value`` is None, else give it back."""
    if value is None:
        raise AssertionError(why)
    return value

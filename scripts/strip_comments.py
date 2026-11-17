# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""Strips comments and docstrings from Python files for the release build."""

from __future__ import annotations

import ast
import io
import re
import sys
import tokenize
from pathlib import Path
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from collections.abc import Iterator


def _bare_strings(tree: ast.AST) -> Iterator[tuple[ast.Expr, bool]]:
    for node in ast.walk(tree):
        for field in ("body", "orelse", "finalbody"):
            block = getattr(node, field, None)
            if not isinstance(block, list):
                continue
            strings = [
                s
                for s in block
                if isinstance(s, ast.Expr)
                and isinstance(s.value, ast.Constant)
                and isinstance(s.value.value, str)
            ]
            # A block of nothing but strings needs a pass where the first one
            # was, except a whole file, which can be empty.
            emptied = bool(strings) and len(strings) == len(block)
            emptied = emptied and not isinstance(node, ast.Module)
            for n, stmt in enumerate(strings):
                yield stmt, emptied and n == 0


def strip_source(src: str) -> str:
    """Return `src` without its comments and docstrings."""
    lines = src.splitlines()
    gone: set[int] = set()
    instead: dict[int, str] = {}
    for stmt, emptied in _bare_strings(ast.parse(src)):
        first = stmt.lineno
        if lines[first - 1][: stmt.col_offset].strip():
            continue
        gone.update(range(first, (stmt.end_lineno or first) + 1))
        if emptied:
            instead[first] = " " * stmt.col_offset + "pass"
    cut = {
        tok.start[0]: tok.start[1]
        for tok in tokenize.generate_tokens(io.StringIO(src).readline)
        if tok.type == tokenize.COMMENT
    }
    kept = []
    for number, line in enumerate(lines, 1):
        if number in instead:
            kept.append(instead[number])
        elif number not in gone:
            text = line[: cut.get(number, len(line))].rstrip()
            if text or number not in cut:
                kept.append(text)
    return re.sub(r"\n{4,}", "\n\n\n", "\n".join(kept)).strip("\n") + "\n"


def strip_file(path: Path, *, check: bool = False) -> tuple[int, int]:
    """Strip one file, or with `check` only fail when it would change."""
    before = path.read_text(encoding="utf-8")
    after = strip_source(before)
    compile(after, str(path), "exec")
    if check:
        if after != before:
            msg = "comments or docstrings remain"
            raise ValueError(msg)
        return len(before), len(before)
    path.write_text(after, encoding="utf-8", newline="\n")
    return len(before), len(after)


def main(root: str, *, check: bool = False) -> int:
    """Strip every Python file under `root`, and return the exit code."""
    files = sorted(p for p in Path(root).rglob("*.py") if "__pycache__" not in p.parts)
    sizes = []
    for path in files:
        try:
            sizes.append(strip_file(path, check=check))
        except (OSError, SyntaxError, ValueError) as e:
            print(f"FAIL {path.relative_to(root)}: {e}", file=sys.stderr)
            return 1
    before, after = sum(b for b, _ in sizes), sum(a for _, a in sizes)
    if check:
        print(f"verified {len(sizes)} Python files contain no comments or docstrings")
    else:
        smaller = 100 * (before - after) / (before or 1)
        print(f"stripped {len(sizes)} Python files, {before:,} to {after:,} bytes", end="")
        print(f" ({smaller:.1f}% smaller)")
    return 0


if __name__ == "__main__":
    flags = sys.argv[1:]
    roots = [a for a in flags if a != "--check"]
    sys.exit(main(roots[0] if roots else ".", check="--check" in flags))

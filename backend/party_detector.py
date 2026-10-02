# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""Guessing who queued together, from how often two accounts share a side."""

from __future__ import annotations

import math
from typing import Any

from common.check import check

# --- guessing a stack from history -----------------------------------------
#
# Riot tells you the party of anyone whose presence you can see, which means
# yourself and your friends. For everyone else the lobby says nothing, so the
# only evidence is how often two accounts have turned up on the same side.
#
# Fix one player in a ten player lobby. The other occupies one of the nine
# remaining slots, four of which are on that first player's team, so two
# strangers share a side four times in nine. Queueing together makes it happen
# every time. The question is whether a run of same side meetings is short
# enough to be luck, which is what the binomial tail below answers.
SAME_TEAM_ODDS = 4 / 9

# Three meetings is the fewest that can say anything at all: all three on the
# same side lands at p = 0.09. Two can never clear any threshold.
MIN_SHARED = 3

# The chance of calling a stack that is not there, for the whole lobby. A lobby
# has twenty pairs to test, so one in ten per pair would make two false calls a
# lobby, and a false call says something untrue about a real person.
# likely_stacks divides this between the pairs.
FALSE_CALL_BUDGET = 0.05


def _tail(hits: int, trials: int, odds: float) -> float:
    """P(X >= hits) for X binomial over `trials` at `odds`. The luck of a run."""
    return sum(
        math.comb(trials, i) * odds**i * (1 - odds) ** (trials - i) for i in range(hits, trials + 1)
    )


def _pair_history(
    rosters: list[dict[str, Any]], a: str, b: str, skip: str | None
) -> tuple[int, int]:
    """How many lobbies held both accounts, and how many put them on one side."""
    shared = same = 0
    for roster in rosters:
        if roster.get("matchId") == skip:
            continue
        teams = roster.get("teams") or {}
        side_a, side_b = teams.get(a), teams.get(b)
        if not side_a or not side_b:
            continue
        shared += 1
        if side_a == side_b:
            same += 1
    return shared, same


def likely_stacks(
    board: dict[str, Any], rosters: list[dict[str, Any]]
) -> dict[str, dict[str, Any]]:
    """Guess who queued together, keyed by puuid."""
    # Only teammates in the current lobby are compared. A stack is always on
    # one side, and comparing across the map would turn two opponents who keep
    # meeting into a party.
    sides: dict[str, list[str]] = {}
    for player in board.get("players") or []:
        if not isinstance(player, dict):
            continue
        puuid, team = player.get("puuid"), player.get("team")
        if puuid and team:
            sides.setdefault(team, []).append(puuid)

    skip = board.get("matchId")

    # Count the pairs first so the budget can be split between them.
    tests = sum(len(m) * (len(m) - 1) // 2 for m in sides.values())
    limit = FALSE_CALL_BUDGET / max(1, tests)

    flagged: list[tuple[str, str, float, int, int]] = []
    for members in sides.values():
        for i, a in enumerate(members):
            for b in members[i + 1 :]:
                shared, same = _pair_history(rosters, a, b, skip)
                if shared < MIN_SHARED or same < MIN_SHARED:
                    continue
                p = _tail(same, shared, SAME_TEAM_ODDS)
                if p <= limit:
                    flagged.append((a, b, p, shared, same))

    # A pair that shares an account with another pair is one stack, not two:
    # if A queues with B and B queues with C, the three of them are a trio.
    groups: list[dict[str, Any]] = []
    for a, b, p, shared, same in flagged:
        touching = [g for g in groups if a in g["members"] or b in g["members"]]
        if touching:
            target = touching[0]
            for other in touching[1:]:
                target["members"] |= other["members"]
                target["p"] = max(target["p"], other["p"])
                target["shared"] = max(target["shared"], other["shared"])
                target["same"] = max(target["same"], other["same"])
                groups.remove(other)
        else:
            target = {"members": set(), "p": 0.0, "shared": 0, "same": 0}
            groups.append(target)
        target["members"] |= {a, b}
        # A stack is only as certain as its least certain link.
        target["p"] = max(target["p"], p)
        target["shared"] = max(target["shared"], shared)
        target["same"] = max(target["same"], same)

    out: dict[str, dict[str, Any]] = {}
    for index, group in enumerate(groups):
        members = sorted(group["members"])
        if len(members) < 2:
            continue
        for puuid in members:
            out[puuid] = {
                "id": f"guess-{index}",
                "size": len(members),
                "confidence": round(100 * (1 - float(group["p"]))),
                "members": members,
                "shared": int(group["shared"]),
                "same": int(group["same"]),
            }
    return out


def _self_check() -> None:
    # Ten past lobbies, mostly strangers who are never seen again, which is what
    # a real log looks like. b0, b1 and b2 are Red in all ten and only just make
    # a trio. b3 is Red in three, nowhere near enough. b4 turns up in nine and is
    # Red in six, which is what luck looks like.
    def past_lobby(n: int) -> dict[str, Any]:
        blue, red = ["me"], ["b0", "b1", "b2"]
        if n < 3:
            red.append("b3")
        if n < 6:
            red.append("b4")
        elif n < 9:
            blue.append("b4")
        blue += [f"blue{n}-{i}" for i in range(5 - len(blue))]
        red += [f"red{n}-{i}" for i in range(5 - len(red))]
        teams = dict.fromkeys(blue, "Blue")
        teams.update(dict.fromkeys(red, "Red"))
        return {"matchId": f"m{n}", "teams": teams}

    rosters = [past_lobby(n) for n in range(10)]

    def board_of(*, b1_swaps: bool = False) -> dict[str, Any]:
        blue = ["me", "a0", "a1", "a2", "a3"]
        red = ["b0", "b1", "b2", "b3", "b4"]
        if b1_swaps:
            red.remove("b1")
            blue.append("b1")
        return {
            "matchId": "now",
            "players": [{"puuid": p, "team": "Blue"} for p in blue]
            + [{"puuid": p, "team": "Red"} for p in red],
        }

    stacks = likely_stacks(board_of(), rosters)

    check(sorted(stacks) == ["b0", "b1", "b2"], stacks)
    check(stacks["b0"]["size"] == 3, stacks["b0"])
    check(stacks["b0"]["id"] == stacks["b2"]["id"], stacks)
    check(stacks["b0"]["confidence"] >= 99, stacks["b0"])

    # Thin evidence and plain luck both stay out of it.
    check("b3" not in stacks, stacks)
    check("b4" not in stacks, stacks)

    # The budget is for the lobby, so it is divided between the twenty pairs in
    # one. Five meetings on the same side is p = 0.017, which reads as certain
    # for a single pair and is nothing of the sort across twenty of them.
    check(likely_stacks(board_of(), rosters[:5]) == {})
    check(likely_stacks(board_of(), rosters[:7]) == {})

    # The current lobby is not evidence for itself.
    only_now = [{"matchId": "now", "teams": {"b0": "Red", "b1": "Red"}}]
    check(likely_stacks(board_of(), only_now) == {})

    # Opponents are never compared, so moving b1 across the map leaves the
    # other two as a pair rather than keeping a trio that cannot exist.
    split = likely_stacks(board_of(b1_swaps=True), rosters)
    check(sorted(split) == ["b0", "b2"], split)

    print("party_detector self-check OK (trio found, thin evidence and luck left alone)")


if __name__ == "__main__":
    _self_check()

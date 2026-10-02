# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""A player's career, read from their recent matches.

Which matches count, the rating line, the averages, guns and habits the
panel shows, and the run of wins or losses they are on.
"""

from __future__ import annotations

from typing import Any

from common.check import check
from vconstants import map_name_from_path, rank_from_tier


def form_streak(form: list[Any]) -> dict[str, Any] | None:
    """Return the run of results at the head of `form`, like three wins, or None."""
    if not form:
        return None
    t, n = form[0], 1
    for r in form[1:]:
        if r != t:
            break
        n += 1
    return {"type": t, "count": n}


# The modes a career is read from: the ones where a K/D means what it means
# in a ranked game. A deathmatch or an ability draft arena doubles somebody's
# kills and says nothing about how they play five against five.
STANDARD_QUEUES = frozenset({"competitive", "unrated", "swiftplay", "premier"})


def career_mids(entries: list[dict[str, Any]], count: int) -> list[str]:
    """Return the last few standard-mode matches, or of any mode when fewer than three are."""
    every = [e["MatchID"] for e in entries if e.get("MatchID")]
    standard = [
        e["MatchID"]
        for e in entries
        if e.get("MatchID") and str(e.get("QueueID") or "").lower() in STANDARD_QUEUES
    ]
    return (standard if len(standard) >= 3 else every)[:count]


def rating_run(updates: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """Riot's rating history as the chart wants it, newest first, without placements."""
    out: list[dict[str, Any]] = []
    for update in updates:
        tier = int(update.get("TierAfterUpdate") or 0)
        # A placement reports tier zero, a cliff to the bottom of a ladder the
        # player was never at.
        if tier < 3:
            continue
        out.append(
            {
                "matchId": update.get("MatchID"),
                "startMillis": update.get("MatchStartTime"),
                "map": map_name_from_path(update.get("MapID") or ""),
                "tierAfter": tier,
                "rrAfter": update.get("RankedRatingAfterUpdate"),
                "rrDelta": update.get("RankedRatingEarned"),
                "rankAfter": rank_from_tier(tier).get("name"),
            }
        )
    out.sort(key=lambda r: -int(r.get("startMillis") or 0))
    return out


def career_summary(matches: list[Any]) -> dict[str, Any]:
    """Return what a career's matches add up to: averages, agents, maps, guns and buys."""
    n = len(matches)
    if not n:
        return {
            "averages": {
                "games": 0,
                "wins": 0,
                "winRate": 0,
                "kd": 0,
                "kills": 0,
                "deaths": 0,
                "assists": 0,
                "hsPct": 0,
            },
            "coPlayers": [],
        }
    wins = sum(1 for m in matches if m["result"] == "Victory")
    k = sum(m["kills"] for m in matches)
    d = sum(m["deaths"] for m in matches)
    a = sum(m["assists"] for m in matches)
    hs = [m["hsPct"] for m in matches if m.get("hsPct") is not None]

    seen: dict[str, dict[str, Any]] = {}
    for m in matches:
        for mate in m["teammates"]:
            pid = mate.get("puuid")
            if not pid:
                continue
            e = seen.setdefault(
                pid, {"puuid": pid, "name": mate.get("name"), "sharedMatches": 0, "agents": set()}
            )
            e["sharedMatches"] += 1
            e["name"] = mate.get("name") or e["name"]
            if mate.get("agent"):
                e["agents"].add(mate["agent"])
    co_players = sorted(
        (
            {
                "puuid": e["puuid"],
                "name": e["name"],
                "sharedMatches": e["sharedMatches"],
                "agents": sorted(e["agents"]),
            }
            for e in seen.values()
            # One match together is a random teammate, not somebody they play with.
            if e["sharedMatches"] >= 2
        ),
        key=lambda x: x["sharedMatches"],
        reverse=True,
    )[:6]

    # Which guns they actually reach for, summed over every match in the
    # history. Each of those matches was fetched in full for this view
    # already, so the answer costs a loop rather than a request.
    guns: dict[str, int] = {}
    for row in matches:
        for gun in (row or {}).get("gunKills") or []:
            name = str(gun.get("name") or "")
            if name:
                guns[name] = guns.get(name, 0) + int(gun.get("kills") or 0)
    total_gun_kills = sum(guns.values())

    # The pistol round habits, summed the same way.
    forced = lost_pistols = won_pistols = 0
    bonus: dict[str, int] = {}
    for row in matches:
        habit = (row or {}).get("buys") or {}
        forced += int(habit.get("forced") or 0)
        lost_pistols += int(habit.get("lostPistols") or 0)
        won_pistols += int(habit.get("wonPistols") or 0)
        for name, rounds in (habit.get("bonus") or {}).items():
            bonus[str(name)] = bonus.get(str(name), 0) + int(rounds or 0)
    bonus_total = sum(bonus.values())
    top_guns = [
        {
            "name": name,
            "kills": kills,
            "share": round(100 * kills / total_gun_kills) if total_gun_kills else 0,
        }
        for name, kills in sorted(guns.items(), key=lambda kv: (-kv[1], kv[0]))[:6]
    ]

    return {
        "averages": {
            "games": n,
            "wins": wins,
            "winRate": round(100 * wins / n),
            "kills": round(k / n, 1),
            "deaths": round(d / n, 1),
            "assists": round(a / n, 1),
            "kd": round(k / d, 2) if d else float(k),
            "hsPct": round(sum(hs) / len(hs)) if hs else None,
        },
        "coPlayers": co_players,
        "topGuns": top_guns,
        # Do they force the round after losing a pistol, and what do they take
        # into the round after winning one.
        "forceHabit": {
            "forced": forced,
            "chances": lost_pistols,
            "pct": round(100 * forced / lost_pistols) if lost_pistols else None,
        },
        "bonusBuys": [
            {
                "name": name,
                "rounds": n,
                "share": round(100 * n / bonus_total) if bonus_total else 0,
            }
            for name, n in sorted(bonus.items(), key=lambda kv: (-kv[1], kv[0]))[:3]
        ],
        "bonusRounds": won_pistols,
    }


def _self_check() -> None:
    # A career is read from 5v5 modes when there are enough of them, and an
    # arena player still gets one rather than an empty panel.
    mixed = [
        {"MatchID": "a", "QueueID": "abilitydraftarena"},
        {"MatchID": "b", "QueueID": "competitive"},
        {"MatchID": "c", "QueueID": "deathmatch"},
        {"MatchID": "d", "QueueID": "unrated"},
        {"MatchID": "e", "QueueID": "swiftplay"},
    ]
    check(career_mids(mixed, 8) == ["b", "d", "e"])
    check(career_mids(mixed[:3], 8) == ["a", "b", "c"])

    # The rating line leaves out placements and runs newest first.
    run = rating_run(
        [
            {
                "MatchID": "old",
                "MatchStartTime": 1,
                "TierAfterUpdate": 12,
                "RankedRatingAfterUpdate": 40,
            },
            {"MatchID": "placement", "MatchStartTime": 0, "TierAfterUpdate": 0},
            {
                "MatchID": "new",
                "MatchStartTime": 2,
                "TierAfterUpdate": 13,
                "RankedRatingAfterUpdate": 5,
            },
        ]
    )
    check([r["matchId"] for r in run] == ["new", "old"], run)
    check(run[0]["rankAfter"] == "Gold 2", run)

    # Tallying the bonus buys must leave the match count the averages divide
    # by alone.
    bought = [
        {
            "result": result,
            "kills": kills,
            "deaths": 10,
            "assists": 2,
            "hsPct": 20,
            "teammates": [],
            "buys": {"bonus": {"Spectre": 1}},
        }
        for result, kills in (("Victory", 10), ("Defeat", 20))
    ]
    averages = career_summary(bought)["averages"]
    check((averages["games"], averages["kills"], averages["winRate"]) == (2, 15.0, 50), averages)

    print("career self-check OK (5v5 first, rating line, bonus buys)")


if __name__ == "__main__":
    _self_check()

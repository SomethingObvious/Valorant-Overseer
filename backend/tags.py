# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""Tags the app works out for itself, from fields the board already has.

These are the ones nobody has to write down: that somebody carries the
Operator, forces after a lost pistol or has played against you a dozen times.
Each is a word for the row and a sentence for the panel, and the sentence
always carries the numbers it was read from, because a tag that cannot say why
is a rumour. The habits come out of the match details fetched for the K/D, so
no tag costs a request.
"""

from __future__ import annotations

from typing import Any

from common.check import check

# The weapons a habit is worth a word for: the tag, the share of rounds
# carrying it that makes it a habit rather than a round, and the fewest
# rounds that can say so. A Spectre is bought on every half buy, so it
# takes a lot more of them to mean anything.
WEAPONS: tuple[tuple[str, str, float, int], ...] = (
    ("Operator", "op", 0.20, 4),
    ("Outlaw", "outlaw", 0.15, 3),
    ("Marshal", "marshal", 0.15, 3),
    ("Judge", "judge", 0.15, 3),
    ("Bucky", "bucky", 0.15, 3),
    ("Odin", "odin", 0.15, 3),
    ("Ares", "ares", 0.15, 3),
    ("Spectre", "spectre", 0.30, 6),
)

# The share of rounds they are in the first fight of. Two of the ten players
# are in every round's, so a fifth is average and nearly a third is somebody
# who goes looking for it.
ENTRY_SHARE = 0.30
# The share of lost pistol rounds they bought a gun after. At or over it they
# force and under it they save, so everybody with one on record is one or the
# other and a team's two counts add up to the team.
FORCE_SHARE = 0.5
# The fewest rounds any rate is read from.
ENOUGH_ROUNDS = 30

# What each tag means, for somebody who has not met it before. The "why"
# beside it is about this player, and this is about the word.
MEANS: dict[str, str] = {
    "op": "Plays the Operator a lot. Expect it on long angles.",
    "outlaw": "Buys the Outlaw often: a cheap two-shot sniper, usually on a half buy.",
    "marshal": "Buys the Marshal often: a light sniper that moves fast.",
    "judge": "Buys the Judge often: a shotgun, deadly close up and around corners.",
    "bucky": "Buys the Bucky often: a cheap shotgun for close angles.",
    "odin": "Buys the Odin often: a machine gun that sprays through walls.",
    "ares": "Buys the Ares often: a cheaper machine gun that sprays through walls.",
    "spectre": "Buys the Spectre a lot: an SMG, strong close up and on the move.",
    "sheriff": "Gets a lot of kills with the Sheriff: one-taps, even on an eco.",
    "entry": "Takes the round's first fight often. Expect them first through the door.",
    "forces": "Buys a gun after losing the pistol round rather than saving. "
    "Round two will have guns.",
    "saves": "Saves after losing the pistol round. Round two will be an eco.",
    "one-trick": "Plays the same agent in every recent match, and is comfortable on it.",
    "off main": "Not on the agent they usually play, and often weaker than their numbers.",
    "hot": "On a winning streak.",
    "cold": "On a losing streak.",
    "rival": "You have played against them several times before.",
    "deranked": "Was clearly higher last act, and probably better than this rank.",
    "clutch": "Wins a lot of rounds as the last one alive. Do not leave them for last.",
    "aced": "Killed all five in a round recently.",
    "planter": "Carries and plants the spike a lot on attack.",
    "strong map": "Wins a lot on this map.",
    "teammate": "Has been on your team before.",
}

# What sort of thing each tag is, for the heading of its explanation.
KIND: dict[str, str] = {
    **{tag: "Weapon" for _, tag, _, _ in WEAPONS},
    "sheriff": "Weapon",
    "entry": "Duels",
    "clutch": "Duels",
    "aced": "Duels",
    "planter": "Duels",
    "forces": "Economy",
    "saves": "Economy",
    "one-trick": "Agent",
    "off main": "Agent",
    "hot": "Form",
    "cold": "Form",
    "rival": "History",
    "teammate": "History",
    "deranked": "Rank",
    "strong map": "Map",
}

# The leaderboard's tag is its place, so it has no key of its own.
LEADERBOARD_MEANS = "On the leaderboard: one of the best players in the region."


def tags_for(p: dict[str, Any], map_name: str | None) -> list[dict[str, str]]:
    """Every tag this player has earned, most useful first."""
    out: list[dict[str, str]] = []

    def add(tag: str, why: str) -> None:
        ranked = tag.startswith("#")
        means = MEANS.get(tag, LEADERBOARD_MEANS if ranked else "")
        kind = KIND.get(tag, "Rank" if ranked else "")
        out.append({"tag": tag, "why": why, "means": means, "kind": kind})

    h = p.get("habits") or {}
    rounds = int(h.get("rounds") or 0)
    matches = int(h.get("matches") or 0)
    last = "their last game" if matches == 1 else f"their last {matches} games"

    carried = h.get("carried") or {}
    for weapon, tag, share, least in WEAPONS:
        n = int(carried.get(weapon) or 0)
        if rounds and n >= least and n / rounds >= share:
            add(tag, f"Carried the {weapon} in {n} of {rounds} rounds over {last}")

    kills = int(h.get("kills") or 0)
    sheriff = int((h.get("killsWith") or {}).get("Sheriff") or 0)
    if kills >= 20 and sheriff >= 5 and sheriff / kills >= 0.15:
        add("sheriff", f"{sheriff} of their {kills} kills with the Sheriff")

    first, died = int(h.get("firstBloods") or 0), int(h.get("firstDeaths") or 0)
    if rounds >= ENOUGH_ROUNDS and (first + died) / rounds >= ENTRY_SHARE:
        add(
            "entry",
            f"In the round's first fight {first + died} times in {rounds} rounds and won {first}",
        )

    lost, forced = int(h.get("lostPistols") or 0), int(h.get("forced") or 0)
    rounds_word = "round" if lost == 1 else "rounds"
    if lost and forced / lost >= FORCE_SHARE:
        add("forces", f"Bought a gun after {forced} of {lost} lost pistol {rounds_word}")
    elif lost:
        add("saves", f"Saved after {lost - forced} of {lost} lost pistol {rounds_word}")

    agent = p.get("agent")
    top = [a for a in p.get("topAgents") or [] if isinstance(a, dict) and a.get("agent")]
    if agent and matches >= 3 and top:
        played = {str(a["agent"]): int(a.get("games") or 0) for a in top}
        if played.get(agent, 0) >= matches:
            add("one-trick", f"{agent} in all of {last}")
        elif agent not in played:
            add("off main", f"No {agent} in {last}, mostly {top[0]['agent']}")

    streak = p.get("streak") or {}
    run = int(streak.get("count") or 0)
    if run >= 3:
        # career.form_streak writes the direction under "type", not "kind".
        won = str(streak.get("type") or "").upper().startswith("W")
        add("hot" if won else "cold", f"{'Won' if won else 'Lost'} their last {run} games")

    enc = p.get("encounter") or {}
    against = int(enc.get("againstCount") or 0)
    if against >= 3:
        record = f"{int(enc.get('winsAgainst') or 0)}W-{int(enc.get('lossesAgainst') or 0)}L"
        add("rival", f"Against you {against} times before, {record}")

    place = int(p.get("leaderboard") or 0)
    if place > 0:
        add(f"#{place}", f"#{place} on the leaderboard")

    now, before = int(p.get("rankTier") or 0), int(p.get("previousRankTier") or 0)
    if now >= 3 and before >= now + 3:
        add("deranked", f"Was {p.get('previousRank')} last act")

    clutches = int(h.get("clutches") or 0)
    if clutches >= 3:
        add("clutch", f"Won {clutches} clutches in {last}")

    aces = int(h.get("aces") or 0)
    if aces:
        add("aced", f"Aced {'once' if aces == 1 else f'{aces} times'} in {last}")

    plants = int(h.get("plants") or 0)
    if rounds >= ENOUGH_ROUNDS and plants >= 4 and plants / (rounds / 2) >= 0.3:
        add("planter", f"Planted the spike {plants} times in {last}")

    here = p.get("mapWinRate") or {}
    games, rate = int(here.get("games") or 0), int(here.get("winRate") or 0)
    if map_name and games >= 3 and rate >= 67:
        add("strong map", f"Won {rate}% of their last {games} games on {map_name}")

    together = int(enc.get("withCount") or 0)
    if together >= 2:
        add("teammate", f"On your side {together} times before")

    return out


def attach(board: dict[str, Any]) -> dict[str, Any]:
    """Tags every player on the board, and drops the raw habits they came from."""
    for p in board.get("players") or []:
        if isinstance(p, dict):
            p["autoTags"] = tags_for(p, board.get("map"))
            p.pop("habits", None)
    return board


def _self_check() -> None:
    base: dict[str, Any] = {
        "agent": "Jett",
        "topAgents": [{"agent": "Jett", "games": 5}],
        "habits": {
            "matches": 5,
            "rounds": 110,
            "carried": {"Operator": 31, "Vandal": 60, "Outlaw": 2},
            "kills": 90,
            "killsWith": {"Operator": 30, "Vandal": 55, "Sheriff": 5},
            "lostPistols": 4,
            "forced": 3,
            "firstBloods": 24,
            "firstDeaths": 10,
            "clutches": 1,
            "aces": 0,
            "plants": 2,
        },
        "streak": {"type": "W", "count": 4},
        "encounter": {"againstCount": 5, "winsAgainst": 1, "lossesAgainst": 4},
        "mapWinRate": {"games": 3, "winRate": 67},
        "rankTier": 14,
        "previousRankTier": 18,
        "previousRank": "Platinum 3",
    }
    tags = [t["tag"] for t in tags_for(base, "Ascent")]
    check(
        tags
        == [
            "op",
            "entry",
            "forces",
            "one-trick",
            "hot",
            "rival",
            "deranked",
            "strong map",
        ],
        tags,
    )
    # Two rounds of the Outlaw and five Sheriff kills in ninety are a round
    # and a pistol, not habits.
    check("outlaw" not in tags and "sheriff" not in tags, tags)

    why = {t["tag"]: t["why"] for t in tags_for(base, "Ascent")}
    check(why["op"] == "Carried the Operator in 31 of 110 rounds over their last 5 games", why)
    check(why["forces"] == "Bought a gun after 3 of 4 lost pistol rounds", why)
    # Every tag this can give says what it means and what sort it is.
    check(all(t["means"] and t["kind"] for t in tags_for(base, "Ascent")))
    ranked = tags_for({"leaderboard": 12}, None)
    check(
        ranked
        == [
            {
                "tag": "#12",
                "why": "#12 on the leaderboard",
                "means": LEADERBOARD_MEANS,
                "kind": "Rank",
            }
        ],
        ranked,
    )

    # Forcing and saving are an either-or once a pistol has been lost, with
    # half and over as forcing, and nothing to go on means neither.
    def econ(lost: int, forced: int) -> list[str]:
        habits = {**base["habits"], "lostPistols": lost, "forced": forced}
        tagged = tags_for({**base, "habits": habits}, None)
        return [t["why"] for t in tagged if t["tag"] in ("forces", "saves")]

    check(econ(4, 0) == ["Saved after 4 of 4 lost pistol rounds"])
    check(econ(4, 1) == ["Saved after 3 of 4 lost pistol rounds"])
    check(econ(4, 2) == ["Bought a gun after 2 of 4 lost pistol rounds"])
    check(econ(1, 1) == ["Bought a gun after 1 of 1 lost pistol round"])
    check(econ(0, 0) == [])

    # Off main only with enough matches to know what the main is.
    off = {**base, "agent": "Sage"}
    check("off main" in [t["tag"] for t in tags_for(off, None)])
    thin = {**off, "habits": {**base["habits"], "matches": 2}}
    check("off main" not in [t["tag"] for t in tags_for(thin, None)])

    # A losing run is cold, in the shape the backend writes a streak.
    losing = {**base, "streak": {"type": "L", "count": 3}}
    check("cold" in [t["tag"] for t in tags_for(losing, None)])
    check("hot" not in [t["tag"] for t in tags_for(losing, None)])

    # Nothing known, nothing said.
    check(tags_for({}, None) == [])

    board: dict[str, Any] = {"map": "Ascent", "players": [dict(base)]}
    attach(board)
    check("habits" not in board["players"][0])
    check(board["players"][0]["autoTags"][0]["tag"] == "op")

    print("tags self-check OK (weapons, economy, duels, form, history, thin samples left alone)")


if __name__ == "__main__":
    _self_check()

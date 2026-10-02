# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""What a match detail says about each player, round by round.

The scoreboard figures Riot doesn't hand over (KAST, first bloods, trades,
clutches, plants) and the habits the tags read, worked out from the kills and
the economy of every round.
"""

from __future__ import annotations

from typing import Any

import valapi
from common.check import check

# Three seconds is the trade window the public trackers settle on. Riot does
# not publish one.
_TRADE_WINDOW_MS = 3000


def round_stats(md: dict[str, Any], rounds: int) -> dict[str, dict[str, Any]]:
    """Per-player damage, KAST, opening duels, spike, clutches and multikills."""
    # Rounds get abandoned, players disconnect, and Riot has shipped rounds with
    # no playerStats at all, so every field here is read defensively.
    out: dict[str, dict[str, Any]] = {}
    # Which side each account played, for the clutch check below.
    teams: dict[str, str] = {}
    for player in md.get("players") or []:
        if isinstance(player, dict) and player.get("subject") and player.get("teamId"):
            teams[str(player["subject"])] = str(player["teamId"])

    def slot(puuid: str) -> dict[str, Any]:
        if puuid not in out:
            out[puuid] = {
                "damage": 0,
                "spent": 0,
                "kast": 0,
                "firstBloods": 0,
                "firstDeaths": 0,
                "headshots": 0,
                "bodyshots": 0,
                "legshots": 0,
                "plants": 0,
                "plantsWon": 0,
                "defuses": 0,
                "defusesWon": 0,
                "clutches": 0,
                "clutchesLost": 0,
                "multiKills": {2: 0, 3: 0, 4: 0, 5: 0},
                "weapons": {},
            }
        return out[puuid]

    for rr in md.get("roundResults") or []:
        stats = rr.get("playerStats") or []
        if not stats:
            continue

        kills: list[dict[str, Any]] = []
        clutched: set[str] = set()
        alive: set[str] = set()
        assisted: set[str] = set()
        killed_by: dict[str, str] = {}
        killed_at: dict[str, int] = {}
        per_player_kills: dict[str, int] = {}

        for ps in stats:
            sub = ps.get("subject")
            if not sub:
                continue
            alive.add(sub)
            entry = slot(sub)

            for dmg in ps.get("damage") or []:
                entry["damage"] += int(dmg.get("damage") or 0)
                entry["headshots"] += int(dmg.get("headshots") or 0)
                entry["bodyshots"] += int(dmg.get("bodyshots") or 0)
                entry["legshots"] += int(dmg.get("legshots") or 0)

            econ = ps.get("economy") or {}
            entry["spent"] += int(econ.get("spent") or 0)

            for kill in ps.get("kills") or []:
                when = int(kill.get("timeSinceRoundStartMillis") or 0)
                victim = kill.get("victim")
                kills.append({"killer": sub, "victim": victim, "at": when})
                per_player_kills[sub] = per_player_kills.get(sub, 0) + 1
                if victim:
                    killed_by[victim] = sub
                    killed_at[victim] = when
                for helper in kill.get("assistants") or []:
                    if helper:
                        assisted.add(helper)
                item = (kill.get("finishingDamage") or {}).get("damageItem") or ""
                if item:
                    entry["weapons"][item] = entry["weapons"].get(item, 0) + 1

        # A plant that loses the round is a different thing from one that wins
        # it, so plants and defuses each keep a second count of the ones that won.
        won_by = rr.get("winningTeam")
        planter = rr.get("bombPlanter")
        if planter:
            slot(planter)["plants"] += 1
            if won_by and teams.get(planter) == won_by:
                slot(planter)["plantsWon"] += 1
        defuser = rr.get("bombDefuser")
        if defuser:
            slot(defuser)["defuses"] += 1
            if won_by and teams.get(defuser) == won_by:
                slot(defuser)["defusesWon"] += 1

        # A clutch is being the last one standing on your side with at least
        # one opponent alive, and then winning the round. Rounds do not carry
        # sides, so they come from the match roster, but only for people in
        # this round's playerStats: the roster can list a coach, who never
        # dies. A deathmatch has a side per player and would make everyone the
        # last one standing, so it takes exactly two sides. With no winner
        # nothing is recorded rather than a loss being invented.
        played = {str(ps.get("subject")) for ps in stats if ps.get("subject")}
        sides: dict[str, set[str]] = {}
        for puuid in played:
            side = teams.get(puuid)
            if side:
                sides.setdefault(side, set()).add(puuid)
        if won_by and len(sides) == 2:
            order = sorted(kills, key=lambda k: k["at"])
            standing: dict[str, set[str]] = {side: set(members) for side, members in sides.items()}
            for kill in order:
                victim = str(kill.get("victim") or "")
                fell = teams.get(victim)
                if fell:
                    # `standing` holds only the sides that played this round,
                    # so a victim from any other side is not in it.
                    standing.get(fell, set()).discard(victim)
                for side_name, members in standing.items():
                    if len(members) != 1:
                        continue
                    alone = next(iter(members))
                    others = sum(len(m) for name, m in standing.items() if name != side_name and m)
                    if others >= 1 and alone not in clutched:
                        clutched.add(alone)
                        key = "clutches" if won_by == side_name else "clutchesLost"
                        slot(alone)[key] += 1

        if kills:
            first = min(kills, key=lambda k: k["at"])
            if first.get("killer"):
                slot(first["killer"])["firstBloods"] += 1
            if first.get("victim"):
                slot(first["victim"])["firstDeaths"] += 1

        for sub, count in per_player_kills.items():
            if count >= 2:
                slot(sub)["multiKills"][min(count, 5)] += 1

        died = set(killed_by)
        for sub in alive:
            traded = False
            killer = killed_by.get(sub)
            if killer is not None:
                # You were traded if whoever killed you died soon afterwards.
                avenged = killed_at.get(killer)
                if avenged is not None and 0 <= avenged - killed_at.get(sub, 0) <= _TRADE_WINDOW_MS:
                    traded = True
            if per_player_kills.get(sub) or sub in assisted or sub not in died or traded:
                slot(sub)["kast"] += 1

    span = max(1, rounds)
    for entry in out.values():
        entry["adr"] = round(entry["damage"] / span)
        entry["kastPct"] = round(entry["kast"] / span * 100)
        # Riot's own econ rating is damage per 1000 credits spent.
        entry["econ"] = round(entry["damage"] / entry["spent"] * 1000) if entry["spent"] else None
        # Only the count here. match_detail works out the headshot rate from
        # the same three fields, so there is one answer.
        entry["shots"] = entry["headshots"] + entry["bodyshots"] + entry["legshots"]
        ranked_weapons = sorted(entry["weapons"].items(), key=lambda kv: (-kv[1], str(kv[0])))
        entry["weaponKills"] = [
            {"name": valapi.weapon_name(item) or "Ability", "kills": count}
            for item, count in ranked_weapons[:6]
        ]
        entry["topWeapon"] = entry["weaponKills"][0] if entry["weaponKills"] else None
        entry["multiKills"] = {str(k): v for k, v in entry["multiKills"].items() if v}
        entry.pop("weapons", None)
        entry.pop("kast", None)
    return out


# A rifle and armour is about 3900, an SMG and light armour about 2000. Below
# that nobody has forced anything. They saved and bought a pistol.
FORCE_VALUE = 2000

# The pistol rounds and the round after each of them. Competitive plays twelve
# a side, so the second half opens at index twelve. Overtime does its own thing
# and is left out rather than guessed at.
PISTOL_ROUNDS = (0, 12)


def habits_of(details: list[dict[str, Any]], puuid: str) -> dict[str, Any]:
    """Return the raw counts tags.py reads, from the matches already fetched for the K/D."""
    h: dict[str, Any] = {
        "matches": 0,
        "rounds": 0,
        "carried": {},
        "kills": 0,
        "killsWith": {},
        "lostPistols": 0,
        "forced": 0,
        "firstBloods": 0,
        "firstDeaths": 0,
        "clutches": 0,
        "aces": 0,
        "plants": 0,
    }
    for md in details:
        rounds = [r for r in md.get("roundResults") or [] if isinstance(r, dict)]
        mine = round_stats(md, len(rounds)).get(puuid)
        if not mine:
            continue
        h["matches"] += 1
        for rr in rounds:
            for ps in rr.get("playerStats") or []:
                if isinstance(ps, dict) and ps.get("subject") == puuid:
                    h["rounds"] += 1
                    held = (ps.get("economy") or {}).get("weapon") or ""
                    name = valapi.weapon_name(str(held))
                    if name:
                        h["carried"][name] = h["carried"].get(name, 0) + 1
                    break
        for weapon in mine.get("weaponKills") or []:
            h["killsWith"][weapon["name"]] = h["killsWith"].get(weapon["name"], 0) + weapon["kills"]
            h["kills"] += weapon["kills"]
        buys = buy_habits(md, puuid)
        h["lostPistols"] += buys["lostPistols"]
        h["forced"] += buys["forced"]
        for key in ("firstBloods", "firstDeaths", "clutches", "plants"):
            h[key] += int(mine.get(key) or 0)
        h["aces"] += int((mine.get("multiKills") or {}).get("5") or 0)
    return h


def buy_habits(md: dict[str, Any], puuid: str) -> dict[str, Any]:
    """Whether they force after a lost pistol round, and what they buy after a won one."""
    rounds = md.get("roundResults") or []
    side = None
    for player in md.get("players") or []:
        if isinstance(player, dict) and player.get("subject") == puuid:
            side = player.get("teamId")
            break

    forced = 0
    lost_pistols = 0
    won_pistols = 0
    bonus: dict[str, int] = {}

    for first in PISTOL_ROUNDS:
        second = first + 1
        if not side or second >= len(rounds):
            continue
        pistol = rounds[first] if isinstance(rounds[first], dict) else {}
        winner = pistol.get("winningTeam")
        if not winner:
            continue
        economy: dict[str, Any] = {}
        for ps in (rounds[second] or {}).get("playerStats") or []:
            if isinstance(ps, dict) and ps.get("subject") == puuid:
                economy = ps.get("economy") or {}
                break
        if not economy:
            continue
        if winner == side:
            # A won pistol makes the next round the bonus round.
            won_pistols += 1
            name = valapi.weapon_name(str(economy.get("weapon") or "")) or ""
            if name:
                bonus[name] = bonus.get(name, 0) + 1
        else:
            lost_pistols += 1
            if int(economy.get("loadoutValue") or 0) >= FORCE_VALUE:
                forced += 1

    return {
        "forced": forced,
        "lostPistols": lost_pistols,
        "wonPistols": won_pistols,
        "bonus": bonus,
    }


def _self_check() -> None:
    # Two rounds, worked out by hand, covering every branch of round_stats:
    # an opening duel, an assist, a survivor, a trade inside the window and a
    # multikill.
    #
    #   round 1: A kills B at 5.0s, C assists.
    #     A  kill                         KAST, first blood
    #     B  died, untraded               no KAST, first death
    #     C  assist                       KAST
    #     D  survived                     KAST
    #   round 2: B kills A at 2.0s, C kills B at 3.0s, C kills D at 4.0s.
    #     A  traded, B died 1.0s later    KAST
    #     B  kill                         KAST, first blood
    #     C  two kills                    KAST, one 2k
    #     D  died, C never died           no KAST
    match = {
        "roundResults": [
            {
                "playerStats": [
                    {
                        "subject": "A",
                        "damage": [{"damage": 150, "headshots": 3, "bodyshots": 2}],
                        "economy": {"spent": 3900},
                        "kills": [
                            {
                                "victim": "B",
                                "timeSinceRoundStartMillis": 5000,
                                "assistants": ["C"],
                                "finishingDamage": {"damageItem": "vandal-id"},
                            },
                        ],
                    },
                    {"subject": "B", "damage": [{"damage": 40}], "economy": {"spent": 2900}},
                    {"subject": "C", "damage": [{"damage": 10}], "economy": {"spent": 800}},
                    {"subject": "D", "damage": [], "economy": {"spent": 0}},
                ],
                "winningTeam": "Blue",
                "bombPlanter": "A",
            },
            {
                "playerStats": [
                    {
                        "subject": "A",
                        "damage": [{"damage": 100, "headshots": 1, "bodyshots": 4}],
                        "economy": {"spent": 2900},
                    },
                    {
                        "subject": "B",
                        "damage": [{"damage": 120}],
                        "economy": {"spent": 3900},
                        "kills": [
                            {
                                "victim": "A",
                                "timeSinceRoundStartMillis": 2000,
                                "finishingDamage": {"damageItem": "vandal-id"},
                            },
                        ],
                    },
                    {
                        "subject": "C",
                        "damage": [{"damage": 260}],
                        "economy": {"spent": 3900},
                        "kills": [
                            {
                                "victim": "B",
                                "timeSinceRoundStartMillis": 3000,
                                "finishingDamage": {"damageItem": "vandal-id"},
                            },
                            {
                                "victim": "D",
                                "timeSinceRoundStartMillis": 4000,
                                "finishingDamage": {"damageItem": "vandal-id"},
                            },
                        ],
                    },
                    {"subject": "D", "damage": [{"damage": 30}], "economy": {"spent": 2400}},
                ],
                "winningTeam": "Blue",
                "bombDefuser": "D",
            },
        ],
        # A and C hold one side, B and D the other. The rounds cannot say who
        # clutched without this, because a round does not carry the sides.
        "players": [
            {"subject": "A", "teamId": "Blue"},
            {"subject": "C", "teamId": "Blue"},
            {"subject": "B", "teamId": "Red"},
            {"subject": "D", "teamId": "Red"},
        ],
    }

    stats = round_stats(match, 2)

    check(stats["A"]["firstBloods"] == 1, stats["A"])
    check(stats["A"]["firstDeaths"] == 1, stats["A"])
    check(stats["A"]["kastPct"] == 100, f"A should be traded in round 2: {stats['A']}")
    check(stats["A"]["adr"] == 125, stats["A"])
    check(stats["A"]["econ"] == round(250 / 6800 * 1000), stats["A"])

    # The habits the automatic tags read are the same counts, summed over
    # matches, with the rounds each player was actually in.
    habits = habits_of([match], "A")
    check(habits["matches"] == 1 and habits["rounds"] == 2, habits)
    check(habits["firstBloods"] == 1 and habits["firstDeaths"] == 1, habits)
    check(habits_of([match], "nobody")["matches"] == 0)

    # A landed 10 shots across the two rounds, 4 of them heads.
    check(stats["A"]["shots"] == 10, stats["A"])
    check(stats["A"]["headshots"] == 4, stats["A"])
    check(stats["D"]["shots"] == 0, stats["D"])

    # The spike. Both ends are named on the round itself.
    check(stats["A"]["plants"] == 1, stats["A"])
    check(stats["D"]["defuses"] == 1, stats["D"])
    # A's plant held. D defused and still lost the round, which is worth
    # telling apart from a defuse that won it.
    check(stats["A"]["plantsWon"] == 1, stats["A"])
    check(stats["D"]["defusesWon"] == 0, stats["D"])

    # Clutches. In round 2 C is left alone against B and D and wins it. D is
    # last alive in both rounds and loses both, which is not a clutch and is
    # worth counting separately rather than not at all.
    check(stats["C"]["clutches"] == 1, stats["C"])
    check(stats["C"]["clutchesLost"] == 0, stats["C"])
    check(stats["D"]["clutches"] == 0, stats["D"])
    check(stats["D"]["clutchesLost"] == 2, stats["D"])

    # The whole loadout, not just the favourite.
    check(stats["C"]["weaponKills"][0]["kills"] == 2, stats["C"])
    check(stats["C"]["topWeapon"]["kills"] == 2, stats["C"])

    check(stats["B"]["firstBloods"] == 1, stats["B"])
    check(stats["B"]["firstDeaths"] == 1, stats["B"])
    check(stats["B"]["kastPct"] == 50, f"B has no round-1 contribution: {stats['B']}")

    check(stats["C"]["kastPct"] == 100, stats["C"])
    check(stats["C"]["multiKills"].get("2") == 1, stats["C"])
    check(stats["C"]["firstBloods"] == 0, stats["C"])

    check(stats["D"]["kastPct"] == 50, f"D survived round 1 only: {stats['D']}")

    # A match Riot returned nothing useful for must not throw.
    check(round_stats({}, 0) == {})
    check(round_stats({"roundResults": [{}]}, 1) == {})
    empty = round_stats({"roundResults": [{"playerStats": [{"subject": "X"}]}]}, 1)
    check(empty["X"]["adr"] == 0, empty)
    check(empty["X"]["econ"] is None, empty)

    print("rounds self-check OK (KAST, trades, clutches, spike, habits, empty matches)")


if __name__ == "__main__":
    _self_check()

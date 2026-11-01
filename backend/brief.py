# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""The scouting brief: players in, lines out, each with the count behind it.

Every line is something a player tends to do, read from their own recent
matches, held against players of the same agent and rank band in the
bundled sample (norms.json). It was collected through one account and is
not a representative sample of the whole ranked population. A line keeps its
conditioning in its name: "in the opening kill" is a share of rounds,
"distance from teammates' centre at observed kills" is where they stood
when someone died, never where they spent the round. A line is a finding;
the suggestion after it, for a teammate or an opponent, is a possibility
the finding allows, not a second finding.

The measurements reuse the existing recent-match downloads. No corpus is
downloaded or built on the user's machine. _self_check() covers a round without
the player's stats row, kills without a clock, a snapshot without a
position, a rank median between two bands, a weapon id the catalogue
cannot name.

Never shown: a win probability, an archetype label, a tendency from fewer
than `min_read` matches, or a comparison with peers on a read that did
not persist in the player's rank band. Such a read is a record, the
figure with its count and nothing more.
"""

from __future__ import annotations

import json
import logging
import math
from bisect import bisect_right
from collections import Counter
from pathlib import Path
from statistics import median
from typing import TYPE_CHECKING, Any

import overseerlog
import valapi
from agents import UUID_TO_NAME, resolve_agent
from common.check import check, present

if TYPE_CHECKING:
    from collections.abc import Callable

LOG = overseerlog.get_logger("brief")
NORMS_PATH = Path(__file__).resolve().parent / "norms.json"
PISTOLS = (0, 12)
OVERTIME = 24
TRADE_MS = 5000
SIDES = ("attack", "defense")
KINDS = ("pistol", "after_lost_pistol", "low", "mid", "full")
# The buy families are by the equipment value Riot recorded for the round,
# which is what they held, not what they could have afforded.
KIND_WORDS = {
    "pistol": "pistol rounds",
    "after_lost_pistol": "rounds after pistol loss",
    "low": "rounds with under 2,000 gear",
    "mid": "rounds with 2,000-3,899 gear",
    "full": "rounds with 3,900+ gear",
}
POSITIONAL = ("open_rate", "open_kill_rate", "open_death_rate", "forward", "separation")
# Fewer rounds of a kind than this, and no buy line.
MIN_BUY_ROUNDS = 4
# Fewer rounds of this round's kind than this, and no "this round" line.
MIN_NOW_ROUNDS = 6
# The buy families tied to a kind of round, as against an equipment tier.
ROUND_KINDS = ("pistol", "after_lost_pistol")
# The round kind when the round is known and is neither: no family is this
# round's, and the round-kind families yield.
OTHER_ROUND = "other"
# Equipment worth this much after a lost pistol is a force buy, as
# backend/rounds.py counts it.
FORCE_VALUE = 2000
# A read that persists only at twenty matches in the player's band needs
# this many contributing matches before the brief compares them with peers.
READY20_MATCHES = 15
# The side a read without one in its name is about.
SIDE_OF = {"plant_rate": "attack"}
# What a flagged line allows, for a teammate and for an opponent, when the
# read runs high (+) or low (-): a short preparation, never a claim about
# what the player will do, and nothing at all where the measurement
# supports no defensible action (time of death, distance from spawn).
SUGGESTS = {
    ("open_all", 1): ("Stay close to trade.", "Set up a trade for the opening."),
    ("open_all", -1): ("Agree who takes first contact.", "Watch for another opener."),
    ("first_lethal_attack", -1): ("Be ready to trade early.", "Be ready for early contact."),
    ("first_lethal_attack", 1): (
        "Check their timing before you exec.",
        "Don't rule out a late fight.",
    ),
    ("first_lethal_defense", -1): (
        "Be ready to trade their early fight.",
        "Be ready for an early fight on defense.",
    ),
    ("first_lethal_defense", 1): (
        "Agree the hold before the round.",
        "Don't rule out a late fight.",
    ),
    ("kill_dist_mean", 1): (
        "Plan the long-angle hold together.",
        "Use utility before a long peek.",
    ),
    ("kill_dist_mean", -1): ("Plan the close-angle hold together.", "Clear close corners."),
    ("separation", 1): ("Check the split before you exec.", "Keep flank coverage."),
    ("separation", -1): ("Set up the trade together.", "Be ready for a second player."),
    ("plant_rate", 1): ("Offer them spike.", "Watch for their plant."),
    ("plant_rate", -1): ("Agree who plants.", ""),
    ("force_rate", 1): ("Agree the buy first.", "Don't assume an eco."),
    ("force_rate", -1): ("Agree the buy first.", ""),
    ("carry|sniper", 1): ("Ask about a sniper drop.", "Smoke or flash before peeking long."),
    ("carry|shotgun", 1): ("Plan a close hold together.", "Clear corners from range."),
    ("carry|machine gun", 1): ("Discuss a spam angle.", "Watch for spam through smokes."),
}

_NORMS: dict[str, Any] = {}


def norms() -> dict[str, Any]:
    """Return the shipped norms, read once."""
    if not _NORMS:
        _NORMS.update(json.loads(NORMS_PATH.read_text(encoding="utf-8")))
    return _NORMS


def band(tier: float | None) -> str | None:
    """Return the rank band a tier falls in.

    A median over an even number of matches can land between two tiers,
    8.5 between Bronze 3 and Silver 1; it rounds up, to the band the
    player was moving into.
    """
    if tier is None or not tier > 0:
        return None
    whole = math.floor(tier + 0.5)
    for lo, hi, name in norms()["bands"]:
        if lo <= whole <= hi:
            return str(name)
    return None


def _number(value: Any) -> float:
    try:
        number = float(value)
    except (TypeError, ValueError, OverflowError):
        return math.nan
    return number if math.isfinite(number) else math.nan


def _xy(location: Any) -> tuple[float, float] | None:
    """Return a finite position, or None for a snapshot without one."""
    if not isinstance(location, dict):
        return None
    x, y = _number(location.get("x")), _number(location.get("y"))
    return (x, y) if math.isfinite(x) and math.isfinite(y) else None


def _side(team: str, rr: dict[str, Any]) -> str | None:
    winner, role = rr.get("winningTeam"), rr.get("winningTeamRole")
    if not winner or role not in ("Attacker", "Defender"):
        return None
    if team == winner:
        return "attack" if role == "Attacker" else "defense"
    return "defense" if role == "Attacker" else "attack"


def _enemy(kill: dict[str, Any], teams: dict[str, str]) -> bool:
    killer_team = teams.get(str(kill.get("killer") or ""))
    victim_team = teams.get(str(kill.get("victim") or ""))
    return bool(killer_team and victim_team and killer_team != victim_team)


def _listed(kill: dict[str, Any]) -> dict[str, tuple[float, float]]:
    """Where Riot placed each living player at a kill, those with a usable position."""
    out: dict[str, tuple[float, float]] = {}
    for loc in kill.get("playerLocations") or []:
        if not isinstance(loc, dict):
            continue
        at = _xy(loc.get("location"))
        sub = str(loc.get("subject") or "")
        if sub and at is not None:
            out[sub] = at
    return out


def _distance(kill: dict[str, Any], killer: str) -> float | None:
    at = _listed(kill).get(killer)
    to = _xy(kill.get("victimLocation"))
    if at is None or to is None:
        return None
    distance = math.hypot(at[0] - to[0], at[1] - to[1])
    return distance if math.isfinite(distance) else None


def _round_kills(rr: dict[str, Any]) -> list[tuple[float, dict[str, Any]]]:
    """Every kill with a victim and its clock, in order, as flatten.py orders them.

    The clock is Riot's round time; a kill without one takes its game time
    less the round's typical offset, and one without either goes last.
    """
    raw = [
        kill
        for stats in rr.get("playerStats") or []
        for kill in (stats or {}).get("kills") or []
        if isinstance(kill, dict) and kill.get("victim")
    ]
    offsets = [
        _number(kill.get("gameTime")) - _number(kill.get("roundTime"))
        for kill in raw
        if math.isfinite(_number(kill.get("gameTime")))
        and math.isfinite(_number(kill.get("roundTime")))
    ]
    offset = median(offsets) if offsets else math.nan

    def clock(kill: dict[str, Any]) -> float:
        when = _number(kill.get("roundTime"))
        if math.isfinite(when):
            return when
        game = _number(kill.get("gameTime"))
        return game - offset if math.isfinite(game) and math.isfinite(offset) else math.inf

    return sorted(((clock(kill), kill) for kill in raw), key=lambda item: item[0])


def _opening(
    rr: dict[str, Any], kills: list[tuple[float, dict[str, Any]]], teams: dict[str, str]
) -> dict[str, Any] | None:
    """Pick the first enemy kill, or the first blood when the clocks cannot order them."""
    enemy = [(when, kill) for when, kill in kills if _enemy(kill, teams)]
    if not enemy:
        return None
    if all(math.isfinite(when) for when, _ in enemy):
        return enemy[0][1]
    named = [kill for _, kill in enemy if kill.get("killer") == rr.get("firstBloodPlayer")]
    return named[0] if len(named) == 1 else None


def _traded(kills: list[tuple[float, dict[str, Any]]], index: int, teams: dict[str, str]) -> bool:
    """Whether this kill's killer died to the victim's team within the trade window.

    As more_reads.py counts it: the killer's next recorded death, by event
    order, settles it. It is a trade when it carries a round time inside
    the window and came from the victim's team; any other death of the
    killer, timed or not, by an enemy or by their own team, is a boundary,
    since a killer who died and came back is a new life. Unrelated kills
    without a round time are skipped.
    """
    kill = kills[index][1]
    killer, victim = str(kill.get("killer") or ""), str(kill["victim"])
    when = _number(kill.get("roundTime"))
    if not math.isfinite(when) or killer not in teams or victim not in teams:
        return False
    for _, later in kills[index + 1 :]:
        t = _number(later.get("roundTime"))
        if str(later.get("victim")) == killer:
            return (
                math.isfinite(t)
                and t - when <= TRADE_MS
                and teams.get(str(later.get("killer") or "")) == teams[victim]
            )
        if math.isfinite(t) and t - when > TRADE_MS:
            break
    return False


class Places:
    """Riot's callouts for one map: distance from the side's spawn, as positions.py measures it."""

    def __init__(self, entry: dict[str, Any]) -> None:
        """Index one map's callouts."""
        callouts = [
            (str(c.get("superRegionName") or ""), at)
            for c in entry.get("callouts") or []
            if isinstance(c, dict) and (at := _xy(c.get("location"))) is not None
        ]
        self.spawn = {
            "attack": next((at for s, at in callouts if s == "Attacker Side"), None),
            "defense": next((at for s, at in callouts if s == "Defender Side"), None),
        }

    def to_spawn(self, side: str | None, at: tuple[float, float]) -> float | None:
        """Distance from the side's own spawn."""
        spawn = self.spawn.get(side or "")
        return math.hypot(spawn[0] - at[0], spawn[1] - at[1]) if spawn else None


_PLACES: dict[str, Places] = {}


def places_for(map_id: str) -> Places | None:
    """Return the callouts for a match's map path, from valorant-api, kept once read."""
    key = map_id.rstrip("/").lower()
    if key in _PLACES:
        return _PLACES[key]
    for entry in valapi.get("maps") or []:
        url = str(entry.get("mapUrl") or "").rstrip("/").lower()
        if url:
            _PLACES[url] = Places(entry)
    return _PLACES.get(key)


def _shield(armor: str) -> str:
    return str(norms()["shields"].get(armor, "unknown" if armor else "none"))


def _option(weapon: str | None) -> str:
    """Name the buy-menu option a recorded weapon is: a gun, other, none, or unknown.

    None is a weapon id the catalogue could not name, which is not the same
    as no weapon: it counts as unknown so a missing catalogue never reads
    as a round with no gun.
    """
    if weapon is None:
        return "unknown"
    if not weapon:
        return "none"
    return weapon.lower() if weapon in norms()["weapons"] else "other"


def match_reads(
    md: dict[str, Any],
    puuid: str,
    *,
    weapon_name: Callable[[str], str | None] | None = None,
    places: Places | None = None,
    map_name: Callable[[str], str | None] | None = None,
) -> dict[str, Any] | None:
    """One player's reads from one match detail, as sums with their denominators.

    None for a match that is not a full competitive game, or one they were
    not in. The sums follow the research tables: `open_all` is rounds they
    were in the opening kill over rounds with their stats row, as the
    round table has them; `first_lethal_<side>` the clock of their first
    enemy kill or death over such rounds with a timed one; `death_t` and
    `kill_dist_mean` every timed death and every gun kill, with or without
    a stats row, as the fight table has them; the buy families count rounds
    by option on each kind of round; the positional figures are per side
    over every round with a side, as positions.py observes every player.

    `weapon_name`, `places` and `map_name` default to the valorant-api
    lookups and are taken as arguments so a check can run without the network.
    """
    info = md.get("matchInfo")
    if not isinstance(info, dict):
        return None
    players = [p for p in md.get("players") or [] if isinstance(p, dict) and p.get("subject")]
    rounds = [r for r in md.get("roundResults") or [] if isinstance(r, dict)]
    if len(players) != 10 or not rounds or str(info.get("queueID") or "") != "competitive":
        return None
    me = next((p for p in players if p.get("subject") == puuid), None)
    if me is None:
        return None
    teams = {str(p["subject"]): str(p.get("teamId") or "") for p in players}
    team = teams[puuid]
    parties = Counter(str(p.get("partyId")) for p in players if p.get("partyId"))
    low, full = norms()["loadout_bands"]["low"], norms()["loadout_bands"]["full"]
    name_of = weapon_name or valapi.weapon_name
    if places is None:
        places = places_for(str(info.get("mapId") or ""))

    sums: dict[str, list[float]] = {}

    def add(name: str, num: float, den: float) -> None:
        cur = sums.setdefault(name, [0.0, 0.0])
        cur[0] += num
        cur[1] += den

    buys: dict[str, dict[str, float]] = {k: {"rounds": 0.0, "spent": 0.0} for k in KINDS}
    pos: dict[str, dict[str, list[float]]] = {s: {} for s in SIDES}
    lost_pistol = False
    pistol_known = False
    carried: Counter[str] = Counter()
    unresolved = 0
    rounds_seen = 0
    shots: list[dict[str, int]] = []
    for rr in sorted(rounds, key=lambda r: int(r.get("roundNum") or 0)):
        n = int(rr.get("roundNum") or 0)
        stats = next(
            (
                p
                for p in rr.get("playerStats") or []
                if isinstance(p, dict) and p.get("subject") == puuid
            ),
            None,
        )
        side = _side(team, rr)
        winner = str(rr.get("winningTeam") or "")
        kills = _round_kills(rr)
        opening = _opening(rr, kills, teams)
        opened = bool(opening and puuid in (opening.get("killer"), opening.get("victim")))

        # The positional figures, from every round with a side.
        if side:
            figs = pos[side]
            figs.setdefault("open_rate", []).append(float(opened))
            figs.setdefault("open_kill_rate", []).append(
                float(bool(opening and opening.get("killer") == puuid))
            )
            figs.setdefault("open_death_rate", []).append(
                float(bool(opening and opening.get("victim") == puuid))
            )
            if opening is not None and places is not None:
                # Where they stood at the opening kill: listed among the
                # living, or as the victim, whose spot Riot writes apart.
                at = _listed(opening).get(puuid)
                if at is None and opening.get("victim") == puuid:
                    at = _xy(opening.get("victimLocation"))
                far = places.to_spawn(side, at) if at is not None else None
                if far is not None:
                    figs.setdefault("forward", []).append(far)
            seps = []
            for _, kill in kills:
                listed = _listed(kill)
                mine = listed.get(puuid)
                if mine is None:
                    continue
                mates = [
                    at
                    for sub, at in listed.items()
                    if sub != puuid and teams.get(sub) == team and at != mine
                ]
                if mates:
                    cx = sum(m[0] for m in mates) / len(mates)
                    cy = sum(m[1] for m in mates) / len(mates)
                    seps.append(math.hypot(cx - mine[0], cy - mine[1]))
            if seps:
                figs.setdefault("separation", []).append(sum(seps) / len(seps))

        # Pace and range from every enemy kill they were part of.
        first: float | None = None
        for index, (_, kill) in enumerate(kills):
            if not _enemy(kill, teams):
                continue
            killer, victim = str(kill.get("killer") or ""), str(kill["victim"])
            if puuid not in (killer, victim):
                continue
            when = _number(kill.get("roundTime"))
            if math.isfinite(when) and (first is None or when < first):
                first = when
            if victim == puuid:
                if math.isfinite(when):
                    add("death_t", when, 1.0)
                    add("traded_death", float(_traded(kills, index, teams)), 1.0)
            elif (kill.get("finishingDamage") or {}).get("damageType") == "Weapon":
                dist = _distance(kill, killer)
                if dist is not None:
                    add("kill_dist_mean", dist, 1.0)

        if n in PISTOLS:
            # The round's result is known with or without a stats row.
            lost_pistol = team != winner
            pistol_known = bool(winner)
        if stats is None:
            # No stats row: the round table has no row either, so the reads
            # it carries (openings, first fight by side, buys, plants) have
            # nothing to count here.
            continue
        rounds_seen += 1
        shots.extend(
            {k: int(entry.get(k) or 0) for k in ("headshots", "bodyshots", "legshots")}
            for entry in stats.get("damage") or []
            if isinstance(entry, dict)
        )
        add("open_all", float(opened), 1.0)
        if side and first is not None:
            add(f"first_lethal_{side}", first, 1.0)
        if side == "attack":
            add("plant_rate", float(rr.get("bombPlanter") == puuid), 1.0)

        econ = stats.get("economy") or {}
        weapon_id = str(econ.get("weapon") or "")
        weapon = name_of(weapon_id) if weapon_id else ""
        if weapon is None:
            unresolved += 1
        elif weapon:
            carried[weapon] += 1
        loadout = _number(econ.get("loadoutValue"))

        # The buy families, overtime left out as credits reset there. The
        # pistol and after-lost-pistol kinds need no equipment value.
        if n < OVERTIME:
            kinds = []
            if n in PISTOLS:
                kinds.append("pistol")
            elif math.isfinite(loadout):
                kinds.append("low" if loadout < low else "mid" if loadout < full else "full")
            if n - 1 in PISTOLS and lost_pistol:
                kinds.append("after_lost_pistol")
                if pistol_known and econ:
                    # The force buy, as rounds.py counts it: a recorded
                    # economy after a pistol with a known winner.
                    forced = math.isfinite(loadout) and loadout >= FORCE_VALUE
                    add("force_rate", float(forced), 1.0)
            spent = _number(econ.get("spent"))
            option = _option(weapon)
            shield = f"shield_{_shield(str(econ.get('armor') or '').lower())}"
            for kind in kinds:
                fam = buys[kind]
                fam["rounds"] += 1
                fam["spent"] += spent if math.isfinite(spent) else 0.0
                fam[option] = fam.get(option, 0.0) + 1
                fam[shield] = fam.get(shield, 0.0) + 1

    heads = sum(hits["headshots"] for hits in shots)
    total = sum(hits["headshots"] + hits["bodyshots"] + hits["legshots"] for hits in shots)
    if total:
        add("hs_pct", float(heads), float(total))
    party = me.get("partyId")
    return {
        "match_id": str(info.get("matchId") or ""),
        "map": str((map_name or valapi.map_by_url)(str(info.get("mapId") or "")) or ""),
        "started_ms": int(info.get("gameStartMillis") or 0),
        "agent": UUID_TO_NAME.get(str(me.get("characterId") or "").lower()),
        "tier": me.get("competitiveTier"),
        "party_size": parties[str(party)] if party else None,
        "rounds": int((me.get("stats") or {}).get("roundsPlayed") or rounds_seen),
        "carried": dict(carried),
        "unresolved_weapons": unresolved,
        "sums": sums,
        "buys": buys,
        # Rates average over rounds; distances take the median, as scouting.py folds them.
        "positional": {
            s: {k: (sum(v) / len(v) if k.endswith("_rate") else median(v)) for k, v in figs.items()}
            for s, figs in pos.items()
        },
    }


def _cell(table: dict[str, Any], agent: str | None, band_name: str | None) -> dict[str, Any]:
    """Return the norm cell for an agent and band, or the band alone, or nothing."""
    if agent and f"{agent}|{band_name}" in table:
        return dict(table[f"{agent}|{band_name}"])
    if band_name and f"*|{band_name}" in table:
        return dict(table[f"*|{band_name}"])
    return {}


def _flag(z: float, matches: int) -> bool:
    rule = norms()["flag"]
    return abs(z) >= max(float(rule["z"]), float(rule["se"]) / math.sqrt(matches))


def _ready(read: str, band_name: str | None, matches: int) -> bool:
    """Whether a read persists in this band at the matches in hand.

    A read that clears the bar at ten matches a half in the band is ready
    from the first comparison; one that clears it only at twenty needs
    READY20_MATCHES contributing matches first.
    """
    if not band_name:
        return False
    n = norms()
    if band_name in n["ready"].get(read, []):
        return True
    return matches >= READY20_MATCHES and band_name in n.get("ready20", {}).get(read, [])


def curve_for(read: str, band_name: str | None) -> dict[str, Any] | None:
    """Return the real distribution of peers' averages for a read, in the band or ladder-wide."""
    curves = norms().get("curves") or {}
    found = curves.get(f"{read}|{band_name}") or curves.get(f"{read}|*")
    return dict(found) if found else None


def percentile(z: float, read: str, band_name: str | None, matches: int) -> float:
    """Place a player among their peers: the share of peers whose mean sits below their mean z.

    From the real distribution of players' ten-match means that
    the bundled histograms measured, where one exists for the read: the rank
    of z among its quantiles. Without one, a normal on the scale of
    peers' means: the per-match norm's spread is wider than the spread
    of players' means, and the read's persistence r in the band gives
    the player-level share of the variance, so a mean of `matches`
    per-match z-scores has standard deviation about
    sqrt(r + (1 - r) / matches).
    """
    found = curve_for(read, band_name)
    if found:
        q = [float(v) for v in found["quantiles"]]
        if z <= q[0]:
            return 0.0
        if z >= q[-1]:
            return 1.0
        i = bisect_right(q, z)
        lo, hi = q[i - 1], q[i]
        frac = (z - lo) / (hi - lo) if hi > lo else 0.0
        return (i - 1 + frac) / (len(q) - 1)
    return 0.5 * (1 + math.erf(position(z, read, band_name, matches) / math.sqrt(2)))


def position(z: float, read: str, band_name: str | None, matches: int) -> float:
    """Place a player's mean z on the curve's scale.

    On a real curve that is z itself, clipped to the curve's range; on
    the normal fallback, standard deviations of peers' means from the
    middle.
    """
    found = curve_for(read, band_name)
    if found:
        return min(max(z, float(found["lo"])), float(found["hi"]))
    n = norms()
    r = float((n.get("persistence") or {}).get(read, {}).get(band_name or "", 0.6))
    r = min(max(r, 0.2), 0.95)
    return z / math.sqrt(r + (1 - r) / max(1, matches))


def _shape(read: str, band_name: str | None) -> dict[str, Any]:
    """Return the curve to draw: the real density and range, or nothing for the normal fallback."""
    found = curve_for(read, band_name)
    if not found:
        return {"curve": [], "lo": -2.6, "hi": 2.6}
    return {"curve": list(found["density"]), "lo": float(found["lo"]), "hi": float(found["hi"])}


def _key(read: str) -> str:
    """Name the read's family for the wording tables; every carry read is one family."""
    return "carry" if read.startswith("carry_") else read.split("|", maxsplit=1)[0]


def _pair(
    r: dict[str, Any], read: str, weapons: dict[str, str]
) -> list[float] | tuple[float, float] | None:
    """One match's numerator and denominator for a read.

    The carry reads come from the weapons recorded per round over the
    rounds played, as their bundled norms are weighted; the rest are
    the sums match_reads() kept.
    """
    if read.startswith("carry_"):
        name = weapons.get(read[len("carry_") :])
        rounds = int(r.get("rounds") or 0)
        if not name or rounds <= 0:
            return None
        return (float((r.get("carried") or {}).get(name, 0)), float(rounds))
    pair = r["sums"].get(read)
    return list(pair) if pair else None


def _suggest(read: str, z: float, *, teammate: bool) -> str:
    key = _key(read)
    if key == "carry":
        key = f"carry|{SPECIAL.get(read[len('carry_') :], '')}"
    pair = SUGGESTS.get((key, 1 if z > 0 else -1))
    return (pair[0] if teammate else pair[1]) if pair else ""


def lines_for(
    puuid: str,
    reads: list[dict[str, Any]],
    map_name: str,
    *,
    teammate: bool,
    on_side: str | None = None,
    round_kind: str | None = None,
) -> list[dict[str, Any]]:
    """Every line the brief can say about one player from their matches, most telling first.

    `reads` is match_reads() of each of their recent matches, newest first.
    `on_side` is the side this player is on this round, attack or defense,
    when the board knows it, and `round_kind` the kind of round coming for
    them: pistol, after_lost_pistol, or None. Both only change which lines
    lead and which buy family is marked as this round's.
    Each line carries its kind, the figure, the count it rests on, how many
    matches contributed, the peers' figure composed from each contributing
    match's agent and band, the z-score, whether it clears the flag rule,
    and the words for it. A read that did not persist in the player's band
    is a record: the figure and its count, no comparison.
    """
    reads = [r for r in reads if r]
    n = norms()
    min_read = int(n["min_read"])
    if len(reads) < min_read:
        return []
    tiers = [float(r["tier"]) for r in reads if r.get("tier") is not None]
    band_name = band(median(tiers)) if tiers else None
    agents = Counter(str(r["agent"]) for r in reads if r.get("agent"))
    out: list[dict[str, Any]] = []
    matches = len(reads)
    who = "teammate" if teammate else "opponent"

    per_match: dict[str, dict[str, float]] = {}

    def line(kind: str, read: str, **fields: Any) -> None:
        out.append(
            {
                "puuid": puuid,
                "kind": kind,
                "read": read,
                "matches": matches,
                "band": band_name,
                "for": who,
                **fields,
            }
        )

    # Tendencies: each read's z against the norm for that match's agent and
    # band, averaged over matches the way scouting.py does it, so a player
    # who swaps agents meets each norm; the peers' figure is composed the
    # same way, weighted by what each match contributed.
    weapons = {w.lower(): str(w) for w in n["weapons"]}
    for read, words in n["words"].items():
        if read in POSITIONAL or read.startswith("at_plant"):
            continue
        zs: list[float] = []
        num = den = peer_num = peer_den = 0.0
        for r in reads:
            pair = _pair(r, read, weapons)
            if not pair or pair[1] <= 0:
                continue
            value = pair[0] / pair[1]
            num += pair[0]
            den += pair[1]
            stat = _cell(n["tendencies"], r.get("agent"), band(r.get("tier"))).get(read)
            if stat:
                zs.append((value - stat[0]) / stat[1])
                per_match.setdefault(read, {})[str(r.get("match_id"))] = zs[-1]
                peer_num += stat[0] * pair[1]
                peer_den += pair[1]
        if den <= 0 or (read.startswith("carry_") and num <= 0):
            # A weapon they never carried is no line at all.
            continue
        value = num / den
        count = int(den)
        if len(zs) < min_read or not _ready(read, band_name, len(zs)):
            # Too few matches met a norm, or the read did not persist in
            # this band: the figure stands as a record, without a comparison.
            line(
                "record",
                read,
                words=words,
                value=round(value, 4),
                count=count,
                contributing=len(zs),
                positive=sum(1 for v in zs if v > 0),
                persisted=_ready(read, band_name, len(zs)),
                text=_tendency_text(read, words, value, count, None, z=0.0, flag=False),
            )
            continue
        z = sum(zs) / len(zs)
        flag = _flag(z, len(zs))
        line(
            "tendency",
            read,
            words=words,
            value=round(value, 4),
            count=count,
            contributing=len(zs),
            positive=sum(1 for v in zs if v > 0),
            peers=round(peer_num / peer_den, 4),
            z=round(z, 2),
            pct=round(percentile(z, read, band_name, len(zs)), 3),
            pos=round(position(z, read, band_name, len(zs)), 3),
            flag=flag,
            persisted=True,
            suggest=_suggest(read, z, teammate=teammate) if flag else "",
            text=_tendency_text(read, words, value, count, peer_num / peer_den, z=z, flag=flag),
        )

    # Positional reads, pooled over maps in z-scores, nudged toward the
    # lobby's map where the research found a map-specific part, only where
    # the read persisted in this band.
    for side in SIDES:
        rows = [r for r in reads if r["positional"].get(side)]
        for fig in POSITIONAL:
            read = f"{fig}|{side}"
            zs, here = [], []
            for r in rows:
                value = r["positional"][side].get(fig)
                if value is None:
                    continue
                key = f"{r.get('map')}|{side}|{r.get('agent')}|{band(r.get('tier'))}"
                stat = n["positional"].get(key, {}).get(fig)
                if not stat:
                    continue
                z = (value - stat[0]) / stat[1]
                zs.append(z)
                per_match.setdefault(read, {})[str(r.get("match_id"))] = z
                if r.get("map") == map_name:
                    here.append(z)
            if len(zs) < min_read or not _ready(read, band_name, len(zs)):
                continue
            pooled = sum(zs) / len(zs)
            prior = n["pool_prior"].get(read)
            if prior is not None and here:
                weight = len(here) / (len(here) + float(prior))
                pooled = weight * (sum(here) / len(here)) + (1 - weight) * pooled
            flag = _flag(pooled, len(zs))
            more = "more" if pooled > 0 else "less"
            line(
                "positional",
                read,
                side=side,
                words=n["words"][fig],
                z=round(pooled, 2),
                pct=round(percentile(pooled, read, band_name, len(zs)), 3),
                pos=round(position(pooled, read, band_name, len(zs)), 3),
                count=len(zs),
                contributing=len(zs),
                positive=sum(1 for v in zs if v > 0),
                on_map=len(here),
                flag=flag,
                persisted=True,
                suggest=_suggest(fig, pooled, teammate=teammate) if flag else "",
                text=(
                    f"{side.capitalize()}: {LABELS[fig].format(side=side)}"
                    + (f" · {more} than peers" if flag else "")
                    + f" · {len(zs)} games · {len(here)} on {map_name}"
                ),
            )

    # Style: where they sit on the two axes the research found to replicate,
    # pace and spacing, as a position with a count, never a label.
    for axis, loads in AXES.items():
        per: dict[str, list[float]] = {}
        for read, sign in loads.items():
            for mid, z in per_match.get(read, {}).items():
                per.setdefault(mid, []).append(sign * z)
        scores = [sum(v) / len(v) for v in per.values()]
        if len(scores) < min_read:
            continue
        mean = sum(scores) / len(scores)
        above = sum(1 for v in scores if v > 0)
        # The axis's persistence is about its reads' average; the pace
        # reads sit near 0.72 at ten matches in every band.
        key = axis if curve_for(axis, band_name) else next(iter(loads))
        pct = percentile(mean, key, band_name, len(scores))
        pos = position(mean, key, band_name, len(scores))
        if abs(mean) < STYLE_Z:
            # Within the usual range: no line of its own; the tag still
            # places them.
            line(
                "style",
                axis,
                panel=False,
                words="",
                value=round(mean, 2),
                count=len(scores),
                contributing=len(scores),
                positive=above,
                pct=round(pct, 3),
                pos=round(pos, 3),
                persisted=True,
                text=f"{axis.capitalize()} score {mean:+.2f}, {pct:.0%} of peers below",
            )
            continue
        way = 1 if mean > 0 else -1
        line(
            "style",
            axis,
            panel=axis in PANEL_AXES,
            words=AXIS_WORDS[axis][way],
            pct=round(pct, 3),
            pos=round(pos, 3),
            value=round(mean, 2),
            count=len(scores),
            contributing=len(scores),
            positive=above,
            persisted=True,
            text=AXIS_WORDS[axis][way],
        )

    # The buy families: what they held on each kind of round, as a
    # distribution against peers composed from each contributing match's
    # agent and band, and the one weapon and the one shield to expect, each
    # the most common on its own.
    options = [*(w.lower() for w in n["weapons"]), "other", "none", "unknown"]
    shields = [f"shield_{s}" for s in n["shield_order"]]
    for kind in KINDS:
        rounds = sum(r["buys"][kind]["rounds"] for r in reads)
        if rounds < MIN_BUY_ROUNDS:
            continue
        counts: Counter[str] = Counter()
        spent = 0.0
        peer_sum: Counter[str] = Counter()
        peer_rounds = 0.0
        for r in reads:
            fam = r["buys"][kind]
            spent += fam["spent"]
            for opt, c in fam.items():
                if opt not in ("rounds", "spent"):
                    counts[opt] += int(c)
            peer = _cell(n["buys"], r.get("agent"), band(r.get("tier"))).get(kind)
            if peer and fam["rounds"]:
                peer_rounds += fam["rounds"]
                for opt in [*options, *shields, "spent"]:
                    peer_sum[opt] += float(peer.get(opt, 0.0)) * fam["rounds"]
        peers: dict[str, float] = (
            {opt: peer_sum[opt] / peer_rounds for opt in [*options, *shields, "spent"]}
            if peer_rounds
            else {}
        )
        known = [o for o in options if o != "unknown"]
        ranked = sorted((o for o in known if counts.get(o, 0)), key=lambda o: -counts[o])
        tied = [o for o in ranked if counts[o] == counts[ranked[0]]] if ranked else []
        top_w = ranked[0] if len(tied) == 1 else None
        second = None
        if top_w and len(ranked) > 1:
            lead, runner = counts[top_w] / rounds, counts[ranked[1]] / rounds
            if lead < SECOND_LEAD or runner >= lead - SECOND_GAP:
                second = ranked[1]
        top_s = max(shields, key=lambda s: counts.get(s, 0))
        armor_tied = [s for s in shields if counts.get(s, 0) == counts.get(top_s, 0)]
        expect = {
            "unknown": counts.get("unknown", 0),
            "weapon": top_w,
            "weapon_count": counts.get(top_w, 0) if top_w else 0,
            "tied": tied if len(tied) > 1 else [],
            "tie_count": counts[tied[0]] if len(tied) > 1 else 0,
            "second": second,
            "second_count": counts.get(second, 0) if second else 0,
            "shield": top_s[len("shield_") :],
            "shield_count": counts.get(top_s, 0),
            "shield_tied": (
                [a[len("shield_") :] for a in armor_tied] if len(armor_tied) > 1 else []
            ),
        }
        line(
            "buy",
            kind,
            words=KIND_WORDS[kind],
            count=int(rounds),
            contributing=sum(1 for r in reads if r["buys"][kind]["rounds"]),
            spent=round(spent / rounds),
            peers_spent=round(peers["spent"]) if peers else None,
            weapons=[
                {
                    "option": o,
                    "count": counts.get(o, 0),
                    "share": round(counts.get(o, 0) / rounds, 3),
                    "peers": round(peers[o], 3) if peers else None,
                }
                for o in options
                if counts.get(o, 0) or peers.get(o, 0.0) >= 0.02
            ],
            shields=[
                {
                    "option": s[len("shield_") :],
                    "count": counts.get(s, 0),
                    "share": round(counts.get(s, 0) / rounds, 3),
                    "peers": round(peers[s], 3) if peers else None,
                }
                for s in shields
                if counts.get(s, 0) or peers.get(s, 0.0) >= 0.02
            ],
            expect=expect,
            unknown_weapons=counts.get("unknown", 0),
            text=_buy_text(
                kind, int(rounds), counts, spent / rounds, expect, top_s=top_s, peers=peers
            ),
        )

    # Records: what they carried, who they play (every agent, so the scope
    # can count this lobby's agent), who they queue with.
    unresolved = sum(int(r.get("unresolved_weapons") or 0) for r in reads)
    rounds_all = sum(r["rounds"] for r in reads)
    carried: Counter[str] = Counter()
    for r in reads:
        carried.update(r["carried"])
    if rounds_all and carried:
        top = carried.most_common(3)
        line(
            "record",
            "carried",
            words="weapon recorded for the round, share of rounds",
            count=rounds_all,
            options=[{"option": w, "count": c, "share": round(c / rounds_all, 3)} for w, c in top],
            text="Recorded weapons: "
            + ", ".join(f"{_weapon_word(w.lower())} {c / rounds_all:.0%}" for w, c in top)
            + f" · {rounds_all} rounds"
            + (f" · weapon unknown in {unresolved} rounds" if unresolved else ""),
        )
    roles: Counter[str] = Counter()
    for name, c in agents.items():
        role = (resolve_agent(name) or {}).get("role")
        if role:
            roles[str(role)] += c
    if agents:
        line(
            "record",
            "agents",
            words="agents played, share of matches",
            count=matches,
            options=[
                {"option": a, "count": c, "share": round(c / matches, 3)}
                for a, c in agents.most_common()
            ],
            roles={k: round(v / matches, 3) for k, v in roles.items()},
            text="Recent agents: "
            + ", ".join(f"{a} {c}/{matches}" for a, c in agents.most_common()),
        )
    known = [r for r in reads if r.get("party_size") is not None]
    if known:
        in_party = sum(1 for r in known if int(r["party_size"]) > 1)
        line(
            "record",
            "party",
            words="in a party, share of matches",
            count=len(known),
            value=round(in_party / len(known), 3),
            text=f"Queued with others: {in_party}/{len(known)} games",
        )
    return _present(out, map_name, side=on_side, round_kind=round_kind)


# The two axes of play the research found to replicate across halves of the
# players: pace, early fights against late ones, and
# spacing, apart from the team against with it. Each is the mean of the
# per-match z of the reads that load on it, signed so that positive is early
# and positive is apart.
AXES = {
    "pace": {
        "open_all": 1.0,
        "first_lethal_attack": -1.0,
        "first_lethal_defense": -1.0,
        "death_t": -1.0,
        "open_rate|attack": 1.0,
        "open_rate|defense": 1.0,
    },
    "spacing": {"separation|attack": 1.0, "separation|defense": 1.0},
}
# A position this far from the middle, in the norm's standard deviations, is
# called early or late, apart or close; nearer than that is no line, since
# typical is not news.
STYLE_Z = 0.5
AXIS_WORDS = {
    "pace": {1: "Opening/pace score above peers", -1: "Opening/pace score below peers"},
    "spacing": {1: "Farther from team at kills", -1: "Closer to team at kills"},
}
# The axes the panel draws. Pace mixes frequency with timing, so "earlier"
# is not guaranteed by it; the side-specific timing reads carry that.
PANEL_AXES = ("spacing",)

# A neutral label per read for the full list: the event in the words
# players use, with no direction, since the list holds unflagged reads
# too and a small lean must not read as "often plants". Direction belongs
# to a line that earned the panel, in SHOWN.
LABELS = {
    "open_all": "Opening kill/death involvement",
    "first_lethal_attack": "First kill/death on attack",
    "first_lethal_defense": "First kill/death on defense",
    "death_t": "Death timing",
    "kill_dist_mean": "Gun-kill range",
    "traded_death": "Deaths traded within 5s",
    "hs_pct": "Headshot % of hits",
    "plant_rate": "Spike plants",
    "force_rate": "2,000+ gear after pistol loss",
    "carry": "Rounds with {weapon}",
    "open_rate": "Opening kill/death on {side}",
    "open_kill_rate": "First bloods on {side}",
    "open_death_rate": "First deaths on {side}",
    "forward": "Distance from spawn at first blood on {side}",
    "separation": "Team spacing at kills on {side}",
}
KIND_SHORT = {
    "pistol": "Pistol",
    "after_lost_pistol": "After pistol loss",
    "low": "Gear <2,000",
    "mid": "Gear 2,000-3,899",
    "full": "Gear 3,900+",
}
# Where a shown equipment line says it applies: the equipment value Riot
# recorded for the round, which is what they held, never their credits,
# so the tiers are named by gear, never eco, half buy or full buy.
KIND_SHOWN = {
    "pistol": "on pistol",
    "after_lost_pistol": "after pistol loss",
    "low": "with under 2,000 gear",
    "mid": "with 2,000-3,899 gear",
    "full": "with 3,900+ gear",
}
_UNITS = {
    "death_t": "deaths",
    "traded_death": "deaths",
    "kill_dist_mean": "kills",
    "hs_pct": "hits",
    "plant_rate": "attack rounds",
    "force_rate": "rounds after a lost pistol",
}


# What earns a place on the panel, after Codex's product review of 3 November
# 2026: a behaviour line only when it cleared the bar on a read that changes
# the next round's preparation, an equipment line only for a conspicuous
# choice against peers, at most two lines, at most one of them equipment,
# one per theme, and never a slot filled for its own sake.
PRIORITY = {
    "carry": 3,
    "open_all": 3,
    "first_lethal_attack": 3,
    "first_lethal_defense": 3,
    "open_rate": 3,
    "open_kill_rate": 3,
    "open_death_rate": 3,
    "forward": 2,
    "separation": 2,
    "plant_rate": 2,
    # The force buy matters on the round it is about, and leads there.
    "force_rate": 1,
}
THEME = {
    # A carried weapon and the equipment line for it are one fact.
    "carry": "equipment",
    "open_all": "opening",
    "first_lethal_attack": "opening",
    "first_lethal_defense": "opening",
    "open_rate": "opening",
    "open_kill_rate": "opening",
    "open_death_rate": "opening",
    "forward": "forward",
    "separation": "spacing",
    "plant_rate": "spike",
    "force_rate": "buy",
}
# The words a shown line uses: the event in players' terms, with the
# direction against peers, never a role ("entry", "lurker", "anchor") the
# measurement does not establish. "First blood" is the opening enemy kill;
# "opening kill/death" includes the victim.
SHOWN = {
    ("open_all", 1): "In the opening kill/death more often",
    ("open_all", -1): "In the opening kill/death less often",
    ("first_lethal_attack", -1): "Earlier first kill/death on attack",
    ("first_lethal_attack", 1): "Later first kill/death on attack",
    ("first_lethal_defense", -1): "Earlier first kill/death on defense",
    ("first_lethal_defense", 1): "Later first kill/death on defense",
    ("open_rate", 1): "Opening kill/death more often on {side}",
    ("open_rate", -1): "Opening kill/death less often on {side}",
    ("open_kill_rate", 1): "More first bloods on {side}",
    ("open_kill_rate", -1): "Fewer first bloods on {side}",
    ("open_death_rate", 1): "First death more often on {side}",
    ("open_death_rate", -1): "First death less often on {side}",
    ("forward", 1): "Farther from spawn at first blood on {side}",
    ("forward", -1): "Closer to spawn at first blood on {side}",
    ("separation", 1): "Farther from team at kills on {side}",
    ("separation", -1): "Closer to team at kills on {side}",
    ("plant_rate", 1): "Plants more often",
    ("plant_rate", -1): "Plants less often",
    ("force_rate", 1): "2,000+ gear after pistol loss more often",
    ("force_rate", -1): "2,000+ gear after pistol loss less often",
    ("carry", 1): "Carries {weapon} more often",
    ("death_t", -1): "Dies earlier in the round",
    ("death_t", 1): "Dies later in the round",
    ("kill_dist_mean", 1): "Gun kills from further than peers",
    ("kill_dist_mean", -1): "Gun kills from closer than peers",
}
SPECIAL = {
    "marshal": "sniper",
    "outlaw": "sniper",
    "operator": "sniper",
    "shorty": "shotgun",
    "bucky": "shotgun",
    "judge": "shotgun",
    "stinger": "SMG",
    "spectre": "SMG",
    "ares": "machine gun",
    "odin": "machine gun",
}
PISTOL_GUNS = ("classic", "shorty", "frenzy", "ghost", "bandit", "sheriff")
MAX_SHOWN = 2
# A shown behaviour line needs the standing to hold in this share of the
# contributing matches, and a shown carry read this many rounds carried.
MAJORITY = 0.6
CARRY_MIN_ROUNDS = 8
# A buy family names its second gun when the first holds under this share
# of the rounds, or the second comes within this much of it.
SECOND_LEAD, SECOND_GAP = 0.6, 0.15
# An equipment family needs this many rounds, over this many matches, with
# this share of its weapons named, before any option in it can be shown.
BUY_ROUNDS, BUY_MATCHES, BUY_KNOWN = 10, 5, 0.9


def _read_side(read: str) -> str | None:
    """Name the side a read is specific to, or None for one that spans both."""
    if "|" in read:
        return read.split("|")[1]
    if read in SIDE_OF:
        return SIDE_OF[read]
    for side in SIDES:
        if read.endswith(f"_{side}"):
            return side
    return None


def _behaviour_pick(
    row: dict[str, Any], side: str | None, round_kind: str | None
) -> tuple[float, str] | None:
    """Score a behaviour line for the panel, or None when it has no place there.

    A read about the other side of the round, when the side is known, is
    hidden this half and kept in the history. The force buy leads on the
    round after a lost pistol and is not shown on any other; a weapon
    habit is not shown on a pistol round. A carry read is shown only for a weapon that changes how
    the next fight is taken, a sniper, a shotgun or a machine gun, and only
    when they carry it more than peers.
    """
    if not row.get("flag") or not row.get("persisted"):
        return None
    z = float(row["z"])
    key = _key(row["read"])
    priority = PRIORITY.get(key, 0)
    if key == "force_rate" and round_kind == "after_lost_pistol":
        priority = 3
    n = int(row["contributing"])
    same = int(row.get("positive") or 0) if z > 0 else n - int(row.get("positive") or 0)
    if same < MAJORITY * n:
        # The pooled figure and the match-by-match standing must agree: a
        # habit holds in most of their matches, not in a few heavy ones.
        return None
    if key == "carry":
        family = SPECIAL.get(row["read"][len("carry_") :])
        rounds = float(row["value"]) * int(row["count"])
        if z <= 0 or family in (None, "SMG") or rounds < CARRY_MIN_ROUNDS:
            # Only a weapon that changes how the next fight is taken, and
            # an SMG is the ordinary half buy; carried often enough to matter.
            return None
    own = _read_side(row["read"])
    if side and own and own != side:
        # The other side of the round: kept in the history, not shown
        # this half.
        return None
    if key == "carry" and round_kind == "pistol":
        # The pistol history is what a pistol round is about.
        return None
    if priority < 2:
        return None
    bar = max(float(norms()["flag"]["z"]), float(norms()["flag"]["se"]) / math.sqrt(n))
    return 10 * priority + min(2.0, abs(z) / bar), THEME[key]


def _equipment_pick(
    row: dict[str, Any], round_kind: str | None
) -> tuple[float, dict[str, Any]] | None:
    """Score an equipment line, returning the one option that earned it.

    The family for the round coming is one step more pressing, and a
    family tied to a round kind (pistol, after pistol loss) has no place
    on a round of another kind.
    """
    rounds = int(row["count"])
    if round_kind and row["read"] in ROUND_KINDS and row["read"] != round_kind:
        # The pistol has passed, or the round after it: that family's
        # history is not this round's preparation.
        return None
    bonus = 1 if round_kind and row["read"] == round_kind else 0
    known = 1 - int(row.get("unknown_weapons") or 0) / max(1, rounds)
    if rounds < BUY_ROUNDS or int(row["contributing"]) < BUY_MATCHES or known < BUY_KNOWN:
        return None
    kind = row["read"]
    band_name, matches = row.get("band"), int(row["contributing"])
    best: tuple[float, dict[str, Any]] | None = None
    for w in row["weapons"]:
        opt, share, peers, count = w["option"], float(w["share"]), w.get("peers"), int(w["count"])
        if peers is None or not _ready(f"{kind}_{opt}", band_name, matches):
            # An option whose share did not persist in their band is a
            # record of these rounds, not a habit to plan around.
            continue
        gap = (share - float(peers)) * 100
        family = SPECIAL.get(opt)
        if family == "SMG" and kind in ("low", "mid"):
            # An SMG with under 3,900 in equipment is the ordinary half buy.
            continue
        if family:
            ok = share >= 0.15 and count >= 5 and gap >= 10 and share >= 2 * float(peers)
            priority = 3
        elif opt in PISTOL_GUNS and kind in ("pistol", "after_lost_pistol"):
            ok = share >= 0.65 and count >= 8 and gap >= 20
            priority = 3 if kind == "after_lost_pistol" else 2
        else:
            continue
        if ok:
            score = 10 * min(3, priority + bonus) + min(2.0, gap / 10)
            if best is None or score > best[0]:
                best = (
                    score,
                    {"kind": "weapon", "option": opt, "count": count, "peers": float(peers)},
                )
    for s in row["shields"]:
        share, peers, count = float(s["share"]), s.get("peers"), int(s["count"])
        if peers is None or kind == "low":
            # No shields on a save round is the save, not a habit.
            continue
        if not _ready(f"{kind}_shield_{s['option']}", band_name, matches):
            continue
        gap = (share - float(peers)) * 100
        if share >= 0.75 and gap >= 20:
            score = 10 * min(3, 2 + bonus) + min(2.0, gap / 10)
            if best is None or score > best[0]:
                best = (
                    score,
                    {
                        "kind": "shield",
                        "option": s["option"],
                        "count": count,
                        "peers": float(peers),
                    },
                )
    return best


def _select(
    rows: list[dict[str, Any]],
    map_name: str,
    side: str | None = None,
    round_kind: str | None = None,
) -> list[dict[str, Any]]:
    """Mark the lines the panel shows, at most MAX_SHOWN, and give each its shown words.

    The buy family for the round coming is marked `now`, whether or not it
    earned a top slot, so the panel can say what they held on such rounds.
    """
    picks: list[tuple[float, str, dict[str, Any]]] = []
    for row in rows:
        row["show"] = row["kind"] == "style" and bool(row.get("panel"))
        row["now"] = (
            bool(round_kind)
            and row["kind"] == "buy"
            and row["read"] == round_kind
            and int(row["count"]) >= MIN_NOW_ROUNDS
        )
        if row["kind"] in ("tendency", "positional"):
            pick = _behaviour_pick(row, side, round_kind)
            if pick:
                score, theme = pick
                read = row["read"]
                key, own_side = _key(read), _read_side(read) or ""
                z = float(row["z"])
                row["shown"] = SHOWN[(key, 1 if z > 0 else -1)].format(
                    side=own_side, weapon=_weapon_word(read[len("carry_") :])
                )
                n, positive = int(row["contributing"]), int(row.get("positive") or 0)
                same = positive if z > 0 else n - positive
                way = "above" if z > 0 else "at or below"
                if row["kind"] == "tendency" and row.get("peers") is not None:
                    own = _figure(read, float(row["value"]))
                    theirs = _figure(read, float(row["peers"]))
                    figure = (
                        f"{own} vs peers' {theirs} · {row['count']} {_UNITS.get(read, 'rounds')}"
                    )
                else:
                    figure = f"{n} games · {row.get('on_map', 0)} on {map_name}"
                row["figure"] = figure
                row["shown_evidence"] = f"{figure} · {way} peers {same}/{n} games"
                picks.append((score, theme, row))
        elif row["kind"] == "buy":
            gear = _equipment_pick(row, round_kind)
            if gear:
                score, option = gear
                rounds = int(row["count"])
                if option["kind"] == "weapon":
                    what = _weapon_word(option["option"])
                else:
                    what = _shield_word("shield_" + option["option"]).capitalize()
                row["shown"] = f"{what} {KIND_SHOWN[row['read']]}"
                rate = f"{option['count'] / rounds:.0%}"
                row["shown_evidence"] = (
                    f"{option['count']}/{rounds} rounds ({rate}) · peers {option['peers']:.0%}"
                )
                row["figure"] = row["shown_evidence"]
                row["pick"] = option
                picks.append((score, "equipment", row))
    picks.sort(key=lambda pick: -pick[0])
    themes: set[str] = set()
    shown = 0
    for _, theme, row in picks:
        if shown >= MAX_SHOWN or theme in themes:
            continue
        themes.add(theme)
        row["show"] = True
        row["rank"] = shown
        shown += 1
    rows.sort(
        key=lambda row: (
            not row["show"],
            row["kind"] != "style",
            not row.get("flag", False),
            -float(row["strength"]),
        )
    )
    return rows


def _present(
    rows: list[dict[str, Any]],
    map_name: str,
    side: str | None = None,
    round_kind: str | None = None,
) -> list[dict[str, Any]]:
    """Give every line a headline, its evidence in a few words, and a strength, then rank them.

    The headline is what the window shows; the evidence sits under it in
    small print; the strength is the standing against peers, so the top of
    the list is what matters most. Flagged lines come first, then the rest
    by strength, then buys, then records.
    """
    for row in rows:
        kind, read = row["kind"], row["read"]
        z = float(row.get("z") or 0.0)
        row["strength"] = round(abs(z), 2)
        if kind == "tendency":
            row["headline"] = _label(read)
            unit = _UNITS.get(read, "rounds")
            peers = row.get("peers")
            row["evidence"] = (
                _figure(read, float(row["value"]))
                + (f" vs peers' {_figure(read, float(peers))}" if peers is not None else "")
                + f" · {row['count']} {unit}"
            )
        elif kind == "positional":
            row["headline"] = _label(read)
            row["evidence"] = f"{row['count']} games · {row['on_map']} on {map_name}"
        elif kind == "buy":
            expect, rounds = row["expect"], int(row["count"])
            row["headline"] = (
                f"{KIND_SHORT[read]}: {_gun_summary(expect, rounds)}"
                f" · {_armor_summary(expect, rounds)}"
            )
            peer = next(
                (w.get("peers") for w in row["weapons"] if w["option"] == expect.get("weapon")),
                None,
            )
            if expect.get("tied"):
                row["evidence"] = _tie_text(expect, rounds)
            else:
                row["evidence"] = (
                    (
                        f"{_weapon_word(expect['weapon'])}: peers {peer:.0%} · "
                        if peer is not None
                        else ""
                    )
                    + f"avg spend {row['spent']:,}"
                    + (f" vs {row['peers_spent']:,}" if row.get("peers_spent") else "")
                )
        elif kind == "style":
            mean, n, above = float(row["value"]), int(row["count"]), int(row["positive"])
            row["strength"] = round(abs(mean), 2)
            row["headline"] = str(row["words"])
            if mean > 0:
                row["evidence"] = f"above peers {above}/{n} games"
            else:
                row["evidence"] = f"at or below peers {n - above}/{n} games"
        elif read == "carried":
            row["headline"] = "Recorded weapons: " + ", ".join(
                f"{_weapon_word(o['option'].lower())} {o['share']:.0%}" for o in row["options"]
            )
            row["evidence"] = f"{row['count']} rounds"
        elif read == "agents":
            row["headline"] = "Recent agents: " + ", ".join(
                f"{o['option']} {o['count']}/{row['count']}" for o in row["options"]
            )
            row["evidence"] = ""
        elif read == "party":
            together = round(float(row["value"]) * row["count"])
            row["headline"] = f"Queued with others: {together}/{row['count']} games"
            row["evidence"] = ""
        else:
            # A read that did not persist in this band, kept as a figure.
            row["headline"] = _label(read)
            unit = _UNITS.get(read, "rounds")
            row["evidence"] = f"{_figure(read, float(row['value']))} · {row['count']} {unit}"
    order = {"tendency": 0, "positional": 0, "style": 1, "buy": 2, "record": 3}
    rows.sort(
        key=lambda row: (
            not row.get("flag", False),
            order[row["kind"]],
            -float(row["strength"]),
            row["kind"] == "record" and row["read"] not in ("carried", "agents", "party"),
        )
    )
    return _select(rows, map_name, side, round_kind)


def _shield_word(option: str) -> str:
    kind = option[len("shield_") :]
    if kind == "none":
        return "no armor"
    return "armor unknown" if kind == "unknown" else f"{kind} armor"


def _weapon_word(option: str) -> str:
    """Name a buy-menu option as players say it: OP for the Operator."""
    if option == "none":
        return "no gun recorded"
    if option == "other":
        return "other weapon"
    if option == "unknown":
        return "weapon unknown"
    return "OP" if option == "operator" else option.capitalize()


def _label(read: str) -> str:
    """Name a read neutrally, with its side or weapon filled in."""
    key = _key(read)
    return LABELS.get(key, read).format(
        side=_read_side(read) or "", weapon=_weapon_word(read[len("carry_") :])
    )


def _gun_summary(expect: dict[str, Any], rounds: int) -> str:
    """Name the gun or guns to expect over the rounds, or the tie, or the unknown count."""
    if expect.get("weapon"):
        text = f"{_weapon_word(expect['weapon'])} {expect['weapon_count']}/{rounds}"
        if expect.get("second"):
            text += f" · {_weapon_word(expect['second'])} {expect['second_count']}/{rounds}"
        return text
    if expect.get("tied"):
        names = ", ".join(_weapon_word(o) for o in expect["tied"])
        return f"gun tie: {names} {expect['tie_count']}/{rounds} each"
    return f"weapon unknown {expect.get('unknown', 0)}/{rounds}"


def _armor_summary(expect: dict[str, Any], rounds: int) -> str:
    """Name the armor to expect over the rounds, or the tie."""
    if expect.get("shield_tied"):
        names = ", ".join("none" if a == "none" else a for a in expect["shield_tied"])
        return f"armor tie: {names} {expect['shield_count']}/{rounds} each"
    return f"{_shield_word('shield_' + expect['shield'])} {expect['shield_count']}/{rounds}"


def _tie_text(expect: dict[str, Any], rounds: int) -> str:
    names = ", ".join(_weapon_word(o) for o in expect["tied"])
    return f"Gun tie: {names} ({expect['tie_count']}/{rounds} each)"


def _buy_text(
    kind: str,
    rounds: int,
    counts: Counter[str],
    spent: float,
    expect: dict[str, Any],
    *,
    top_s: str,
    peers: dict[str, float],
) -> str:
    """Write one line per kind of round: the guns and the armor with counts, then the spend.

    Each is the most common on its own, not a loadout seen together. A tie
    between guns names them all and expects none; a family whose weapons
    the catalogue could not name says so instead of naming one.
    """
    gun = _gun_summary(expect, rounds)
    top_w = expect.get("weapon")
    if top_w and peers:
        gun += f" ({_weapon_word(top_w)} peers {peers.get(top_w, 0.0):.0%})"
    unknown = counts.get("unknown", 0)
    if unknown and top_w:
        gun += f" · weapon unknown {unknown}/{rounds}"
    armor = _armor_summary(expect, rounds)
    if peers and not expect.get("shield_tied"):
        armor += f" (peers {peers.get(top_s, 0.0):.0%})"
    text = f"{KIND_SHORT[kind]}: {gun} · {armor} · avg spend {spent:,.0f}"
    if peers:
        text += f" (peers {peers['spent']:,.0f})"
    else:
        text += " · no rank-band comparison"
    return text


def _figure(read: str, value: float) -> str:
    if read in ("first_lethal_attack", "first_lethal_defense", "death_t"):
        return f"{value / 1000:.0f}s"
    if read == "kill_dist_mean":
        return f"{value:,.0f} map units"
    return f"{value:.0%}"


def _tendency_text(
    read: str, words: str, value: float, count: int, peers: float | None, *, z: float, flag: bool
) -> str:
    """One line: the label, the figure with its count, and the peers' figure.

    The figure pools every round; the standing that decides the flag
    averages each match's z-score, as the replay scored it, so the two can
    disagree when a few heavy matches pull the pooled figure one way. The
    list says which way the standing went only when it cleared the bar.
    """
    del words
    unit = _UNITS.get(read, "rounds")
    text = f"{_label(read)}: {_figure(read, value)} · {count} {unit}"
    if peers is not None:
        text += f" · peers {_figure(read, peers)}"
    if flag:
        text += f" · {'more' if z > 0 else 'less'} than peers"
    return text


def brief(
    lobby: list[dict[str, Any]],
    map_name: str,
    details: dict[str, list[dict[str, Any]]],
    self_puuid: str | None = None,
    *,
    weapon_name: Callable[[str], str | None] | None = None,
    places: Places | None = None,
    map_lookup: Callable[[str], str | None] | None = None,
) -> dict[str, list[dict[str, Any]]]:
    """Lines for every player in a lobby, keyed by puuid.

    `lobby` rows carry `puuid` and `team`; `details` holds each player's
    recent match details, newest first. Players on the user's team get the
    teammate wording, the rest the opponent wording. A detail the engine
    cannot read costs that match, not the player or the lobby. The lookups
    pass through to match_reads() for a check without the network.
    """
    mine = next((str(p.get("team") or "") for p in lobby if p.get("puuid") == self_puuid), "")
    out: dict[str, list[dict[str, Any]]] = {}
    for p in lobby:
        puuid = str(p.get("puuid") or "")
        if not puuid:
            continue
        reads = []
        for md in details.get(puuid, []):
            if not isinstance(md, dict):
                continue
            try:
                read = match_reads(
                    md, puuid, weapon_name=weapon_name, places=places, map_name=map_lookup
                )
            except Exception as e:
                info = md.get("matchInfo")
                mid = str(info.get("matchId") or "?") if isinstance(info, dict) else "?"
                LOG.warning("brief skipped match %s for %s: %r", mid[:8], puuid[:8], e)
                continue
            if read:
                reads.append(read)
        teammate = bool(mine) and str(p.get("team") or "") == mine
        out[puuid] = lines_for(puuid, reads, map_name, teammate=teammate)
    return out


# The score seen at each round of a match, so the loser of a pistol round
# is known when the next round's board arrives.
_SCORES: dict[str, dict[int, tuple[int, int]]] = {}
PISTOL_ROUNDS = (1, 13)


def round_context(board: dict[str, Any]) -> tuple[int | None, str | None]:
    """Return the round coming and which team lost the pistol before it, ally or enemy, if any."""
    score = board.get("score") or {}
    n = score.get("round")
    if not isinstance(n, int) or n <= 0:
        return None, None
    ally, enemy = int(score.get("ally") or 0), int(score.get("enemy") or 0)
    seen = _SCORES.setdefault(str(board.get("matchId") or ""), {})
    seen[n] = (ally, enemy)
    if n - 1 == PISTOL_ROUNDS[0]:
        seen.setdefault(PISTOL_ROUNDS[0], (0, 0))
    if n - 1 not in PISTOL_ROUNDS:
        return n, None
    before = seen.get(n - 1, (0, 0) if n == 2 else None)
    if before is None:
        return n, None
    if ally > before[0]:
        return n, "enemy"
    if enemy > before[1]:
        return n, "ally"
    return n, None


def _results(seen: dict[int, tuple[int, int]], n: int, who: str) -> list[bool | None]:
    """Return the team's result of each round of this half before round n, None where unseen."""
    idx = 0 if who == "ally" else 1
    start = PISTOL_ROUNDS[1] if n >= PISTOL_ROUNDS[1] else PISTOL_ROUNDS[0]
    out: list[bool | None] = []
    for k in range(start, n):
        before, after = seen.get(k), seen.get(k + 1)
        out.append(None if before is None or after is None else after[idx] > before[idx])
    return out


def round_ctx(seen: dict[int, tuple[int, int]], n: int, who: str) -> str | None:
    """Name the round's economy context for a team.

    The pistol; the second round after a won or lost pistol; any later
    round after a won round, or after one, two, or three or more lost
    rounds in a row, since the loss bonus grows with the streak. None
    when the scores needed were not seen.
    """
    if n in PISTOL_ROUNDS:
        return "pistol"
    results = _results(seen, n, who)
    if not results or results[-1] is None:
        return None
    last = "won" if results[-1] else "lost"
    if n - 1 in PISTOL_ROUNDS:
        return f"second|{last}"
    streak = 0
    for won in reversed(results):
        if won is not False:
            break
        streak += 1
    return f"later|{last}|{min(streak, STREAK_CAP)}"


def forecast(
    rows: list[dict[str, Any]], ctx: str | None, band_name: str | None
) -> dict[str, Any] | None:
    """Forecast the gun and armor for the round coming, from the context and their tiers.

    The band's odds of each equipment tier in this context (from the
    bundled sample) weigh the player's own share of each
    gun within that tier, shrunk toward peers by `forecast_prior` rounds.
    Measured on the corpus: the most likely gun is the one recorded in
    48 percent of rounds, 59 percent when its odds reach a half, against
    45 for their usual gun; knowing the tier would give 55, so the rest
    is the Vandal-or-Phantom coin. Returns the guns and armor with their
    odds, most likely first, or None without a context or enough rounds.
    """
    n = norms()
    tiers = n.get("tiers") or {}
    if not ctx or ctx == "pistol":
        return None
    share = tiers.get(f"{band_name}|{ctx}") or tiers.get(f"*|{ctx}")
    if not share:
        return None
    prior = float(n.get("forecast_prior") or 4.0)
    guns: Counter[str] = Counter()
    armor: Counter[str] = Counter()
    own_rounds = 0
    for tier, p_tier in zip(TIERS, share, strict=True):
        row = next((r for r in rows if r["kind"] == "buy" and r["read"] == tier), None)
        if row is None or p_tier <= 0:
            continue
        count = int(row["count"])
        own_rounds += count
        for key, pool in (("weapons", guns), ("shields", armor)):
            for option in row[key]:
                peer = float(option.get("peers") or 0.0)
                pool[option["option"]] += (
                    p_tier * (int(option["count"]) + prior * peer) / (count + prior)
                )
    if own_rounds < FORECAST_ROUNDS or sum(guns.values()) <= 0:
        return None

    def ranked(pool: Counter[str]) -> list[tuple[str, float]]:
        total = sum(pool.values())
        return sorted(
            ((k, round(v / total, 3)) for k, v in pool.items() if k != "unknown"),
            key=lambda kv: -kv[1],
        )

    return {"ctx": ctx, "guns": ranked(guns), "armor": ranked(armor), "rounds": own_rounds}


def attach(board: dict[str, Any]) -> dict[str, Any]:
    """Compose every player's brief for this board, and drop the raw measurements.

    The lines depend on the board's
    map, on which team the user is on, on the side each player is on this
    round and on the round coming, so they are rebuilt from the
    measurements on every board and never carried from one lobby or one
    round to the next. A player with too few measurements gets no lines. A
    player whose measurements the engine cannot compose gets no lines
    either, logged, and the rest of the board is unaffected.
    """
    map_name = str(board.get("map") or "")
    mine = str(board.get("selfTeam") or "")
    my_side = str(board.get("side") or "").lower() or None
    other_side = {"attack": "defense", "defense": "attack"}.get(my_side or "")
    n, pistol_lost_by = round_context(board)
    seen = _SCORES.get(str(board.get("matchId") or ""), {})
    for p in board.get("players") or []:
        if not isinstance(p, dict):
            continue
        reads = p.pop("reads", None) or []
        puuid = str(p.get("puuid") or "")
        teammate = bool(mine) and str(p.get("team") or "") == mine
        side = my_side if teammate else other_side
        round_kind = OTHER_ROUND if n else None
        if n in PISTOL_ROUNDS:
            round_kind = "pistol"
        elif pistol_lost_by and (pistol_lost_by == "ally") == teammate:
            round_kind = "after_lost_pistol"
        try:
            p["brief"] = (
                lines_for(
                    puuid, reads, map_name, teammate=teammate, on_side=side, round_kind=round_kind
                )
                if puuid
                else []
            )
            agents_row: dict[str, Any] = next(
                (row for row in p["brief"] if row["read"] == "agents"), {}
            )
            options = list(agents_row.get("options") or [])
            played = [str(o["option"]) for o in options]
            agent = str(p.get("agent") or "")
            on_agent = next((int(o["count"]) for o in options if o["option"] == agent), 0)
            total = int(agents_row.get("count") or 0)
            scope = ""
            if agent and total and on_agent < total:
                scope = f"{agent}: {on_agent}/{total} games"
            if not scope and len(played) > 1:
                scope = "Includes " + ", ".join(played[:3]) + ("…" if len(played) > 3 else "")
            counted = [r.get("agent") for r in reads if r]
            ctx = round_ctx(seen, n, "ally" if teammate else "enemy") if n else None
            band_name = next((row.get("band") for row in p["brief"]), None)
            likely = forecast(p["brief"], ctx, band_name)
            if likely is not None:
                p["brief"].append(_forecast_line(likely, puuid, len(counted), scope))
            p["scout"] = scout(
                p["brief"],
                is_self=bool(p.get("isSelf")),
                side=side,
                round_kind=round_kind,
                likely=likely,
                after_pistol=n is not None and n - 1 in PISTOL_ROUNDS,
                agent=agent,
                on_agent=sum(1 for a in counted if a == agent),
                agents_known=bool(counted) and all(counted),
                matches=len(counted),
            )
            for row in p["brief"]:
                row["scope"] = scope
        except Exception as e:
            LOG.warning("brief not composed for %s: %r", puuid[:8], e)
            p["brief"] = []
    return board


# The equipment tiers, in the order the bundled odds use.
TIERS = ("low", "mid", "full")
# Lost rounds in a row beyond this count the same, as the loss bonus does.
STREAK_CAP = 3
# Fewer rounds across the tiers than this, and no forecast.
FORECAST_ROUNDS = 10
# The round's context in words, for the forecast block.
CTX_WORDS = {
    "second|won": "after a won pistol",
    "second|lost": "after a lost pistol",
    "later|won|0": "after a won round",
    "later|lost|1": "after a lost round",
    "later|lost|2": "after two lost rounds",
    "later|lost|3": "after three or more lost rounds",
}
# A second gun or armor is named when its odds reach this.
SECOND_ODDS = 0.15


def _forecast_line(likely: dict[str, Any], puuid: str, matches: int, scope: str) -> dict[str, Any]:
    """Write the forecast as a line of the history, with how it was made and how it does."""
    guns = ", ".join(f"{_weapon_word(g)} {p:.0%}" for g, p in likely["guns"][:3])
    armor = ", ".join(f"{_shield_word('shield_' + a)} {p:.0%}" for a, p in likely["armor"][:2])
    text = (
        f"Forecast {CTX_WORDS.get(likely['ctx'], likely['ctx'])}: {guns} · {armor}"
        f" · from {likely['rounds']} rounds and the band's tier odds in this context;"
        " right in 48% of past rounds, 59% when the odds reach a half"
    )
    return {
        "puuid": puuid,
        "kind": "forecast",
        "read": likely["ctx"],
        "matches": matches,
        "headline": "Forecast",
        "evidence": guns,
        "text": text,
        "strength": 0.0,
        "show": False,
        "now": False,
        "flag": False,
        "scope": scope,
    }


def _forecast_block(likely: dict[str, Any]) -> dict[str, Any]:
    """Lay out the forecast as the panel's block: label, guns with odds, armor with odds."""
    guns = likely["guns"]
    line = f"{_weapon_word(guns[0][0])} {guns[0][1]:.0%}"
    if len(guns) > 1 and guns[1][1] >= SECOND_ODDS:
        line += f" · {_weapon_word(guns[1][0])} {guns[1][1]:.0%}"
    armor = likely["armor"]
    arm = f"{ARMOR_WORD[armor[0][0]]} {armor[0][1]:.0%}" if armor else ""
    if len(armor) > 1 and armor[1][1] >= SECOND_ODDS:
        arm += f" · {ARMOR_WORD[armor[1][0]].lower()} {armor[1][1]:.0%}"
    return {
        "label": f"Likely this round · {CTX_WORDS.get(likely['ctx'], likely['ctx'])}",
        "guns": line,
        "armor": arm,
        "items": _gun_items([(g, f"{p:.0%}") for g, p in guns if p >= SECOND_ODDS] or guns[:1]),
    }


# Guns drawn as icons in a buy block, at most this many.
MAX_GUN_ICONS = 3


def _gun_items(pairs: list[tuple[str, Any]]) -> list[dict[str, str]]:
    """Name each gun the way the art is filed, with its figure, for the icon row."""
    out = []
    for option, value in pairs:
        if option in ("none", "other", "unknown"):
            continue
        name = next((w for w in norms()["weapons"] if w.lower() == option), option.capitalize())
        out.append({"name": name, "value": value if isinstance(value, str) else f"{value:.0%}"})
    return out[:MAX_GUN_ICONS]


# The reads a habits line can be made of: what they tend to do, measured
# against peers, in the order the panel prefers when nothing is flagged.
HABIT_READS = (
    "open_all",
    "first_lethal_attack",
    "first_lethal_defense",
    "plant_rate",
    "separation",
    "open_kill_rate",
    "open_death_rate",
    "death_t",
    "kill_dist_mean",
)
MAX_HABITS = 3


# Each habit as a sentence with the player's percentile among peers:
# "{p}" is the share of peers they sit beyond, "{side}" the read's side.
# The first form is for a standing above the peers' mean, the second below.
PCT_WORDS = {
    "open_all": (
        "In the opening fight more than {p} of peers",
        "In the opening fight less than {p} of peers",
    ),
    "first_lethal_attack": (
        "Fights later on attack than {p} of peers",
        "Fights earlier on attack than {p} of peers",
    ),
    "first_lethal_defense": (
        "Fights later on defense than {p} of peers",
        "Fights earlier on defense than {p} of peers",
    ),
    "death_t": ("Dies later than {p} of peers", "Dies earlier than {p} of peers"),
    "plant_rate": ("Plants more than {p} of peers", "Plants less than {p} of peers"),
    "kill_dist_mean": (
        "Kills from further than {p} of peers",
        "Kills from closer than {p} of peers",
    ),
    "separation": (
        "Further from the team than {p} of peers on {side}",
        "Closer to the team than {p} of peers on {side}",
    ),
    "open_kill_rate": (
        "Takes first blood more than {p} of peers on {side}",
        "Takes first blood less than {p} of peers on {side}",
    ),
    "open_death_rate": (
        "Dies first more than {p} of peers on {side}",
        "Dies first less than {p} of peers on {side}",
    ),
}
# The pace tag: beyond these shares of peers, the word applies.
TAG_HI, TAG_LO = 0.75, 0.25
# What a habit is called in plain words: above peers, below peers, and the
# two milder forms. Beyond HABIT_STRONG of peers the plain label stands;
# beyond HABIT_MILD the milder "tends to" form; nearer than that the read
# is no habit and gets no line.
HABIT_WORDS = {
    "open_all": (
        "Takes the first fight",
        "Stays out of the first fight",
        "Tends to take the first fight",
        "Tends to stay out of the first fight",
    ),
    "first_lethal_attack": (
        "Slow to engage on attack",
        "Fast to engage on attack",
        "Tends to engage late on attack",
        "Tends to engage early on attack",
    ),
    "first_lethal_defense": (
        "Holds and waits on defense",
        "Fights early on defense",
        "Tends to hold and wait on defense",
        "Tends to fight early on defense",
    ),
    "death_t": (
        "Stays alive late into rounds",
        "Dies early in rounds",
        "Tends to stay alive late into rounds",
        "Tends to die early in rounds",
    ),
    "plant_rate": (
        "Carries and plants the spike",
        "Leaves planting to others",
        "Tends to plant the spike",
        "Tends to leave planting to others",
    ),
    "kill_dist_mean": (
        "Fights at long range",
        "Fights up close",
        "Tends to fight at long range",
        "Tends to fight up close",
    ),
    "separation": (
        "Plays apart from the team on {side}",
        "Sticks with the team on {side}",
        "Tends to play apart from the team on {side}",
        "Tends to stick with the team on {side}",
    ),
    "open_kill_rate": (
        "Gets first bloods on {side}",
        "Rarely gets first blood on {side}",
        "Tends to get first bloods on {side}",
        "Tends not to get first blood on {side}",
    ),
    "open_death_rate": (
        "Dies first on {side}",
        "Rarely dies first on {side}",
        "Tends to die first on {side}",
        "Tends not to die first on {side}",
    ),
}
HABIT_STRONG, HABIT_MILD = 0.75, 0.6
# What the ends of each habit's curve mean: the left word for a low value
# of the read, the right for a high one, so the mark reads at a glance.
AXIS_ENDS = {
    "open_all": ("stays out", "takes fights"),
    "first_lethal_attack": ("early", "late"),
    "first_lethal_defense": ("early", "late"),
    "death_t": ("dies early", "dies late"),
    "plant_rate": ("plants less", "plants more"),
    "kill_dist_mean": ("close", "long range"),
    "separation": ("with team", "apart"),
    "open_kill_rate": ("fewer", "more first bloods"),
    "open_death_rate": ("fewer", "more first deaths"),
}
PACE_ENDS = ("passive", "aggressive")


def _ordinal(share: float) -> str:
    """Write a share of peers as the percentile it is: 85th, 91st, 2nd."""
    n = max(1, min(99, round(share * 100)))
    teens = range(10, 21)
    suffix = "th" if n % 100 in teens else {1: "st", 2: "nd", 3: "rd"}.get(n % 10, "th")
    return f"{n}{suffix}"


def _beyond(row: dict[str, Any]) -> float:
    """Return the share of peers the player sits beyond, whichever way."""
    pct = float(row.get("pct") or 0.5)
    return pct if float(row.get("z") or 0.0) > 0 else 1 - pct


def _habit_line(row: dict[str, Any], *, other: bool, is_self: bool) -> dict[str, Any]:
    """Word one habit plainly, with where they sit among peers as the evidence.

    "Plays apart from the team on defense", and under it "further from
    the team than 80% of peers on defense · 5/7 games": the percentile is
    the share of same-agent, same-rank players their mean sits beyond.
    Beyond three quarters the label stands; beyond six tenths it is
    "Tends to". A read that cleared the bar adds the count of their
    matches on that side of peers and, on the player's current side and
    never on the user's own card, what to do about it.
    """
    read, key = row["read"], _key(row["read"])
    z = float(row.get("z") or 0.0)
    side = _read_side(read) or ""
    beyond = _beyond(row)
    share = f"{beyond:.0%}"
    n, positive = int(row["contributing"]), int(row.get("positive") or 0)
    same = positive if z > 0 else n - positive
    forms4 = HABIT_WORDS.get(key, (_label(read),) * 4)
    strong = beyond >= HABIT_STRONG
    words = forms4[(0 if strong else 2) + (0 if z > 0 else 1)].format(side=side)
    forms = PCT_WORDS.get(key)
    evidence = (
        forms[0 if z > 0 else 1].format(p=share, side=side).lower()
        if forms
        else f"{share} of peers {'below' if z > 0 else 'above'}"
    )
    own = theirs = ""
    if row["kind"] == "tendency" and row.get("peers") is not None and read != "kill_dist_mean":
        own = _figure(read, float(row["value"]))
        theirs = _figure(read, float(row["peers"]))
        evidence += f" · {own} vs {theirs}"
    if row.get("flag"):
        evidence += f" · {same}/{n} games"
    suggest = str(row.get("suggest") or "") if row.get("flag") and not other and not is_self else ""
    return {
        "headline": words,
        "evidence": evidence,
        "suggest": suggest,
        # The one number the chart prints at the mark: the percentile
        # read left to right, the share of peers to the left of the mark,
        # so a mark on the left is a low percentile whatever the read.
        "own": "",
        "peers": "",
        "share": f"{_ordinal(float(row.get('pct') or 0.5))} pct",
        # Where they sit among peers, for the curve: the share below and
        # the position in standard deviations of peers' means.
        "pct": float(row.get("pct") or 0.5),
        "pos": float(row.get("pos") or 0.0),
        "left": AXIS_ENDS.get(key, ("below peers", "above peers"))[0],
        "right": AXIS_ENDS.get(key, ("below peers", "above peers"))[1],
        **_shape(read, row.get("band")),
    }


def _tag(rows: list[dict[str, Any]]) -> dict[str, Any] | None:
    """Name the player's pace from the pace score's percentile: aggressive, passive, or between.

    The pace score is the first axis the research found to replicate:
    opening involvement, the time of the first kill or death by side,
    the time of death. Beyond three quarters of peers it is aggressive,
    below a quarter passive; between, the word is "even pace" and the
    percentile still says where.
    """
    pace = next((r for r in rows if r["kind"] == "style" and r["read"] == "pace"), None)
    if pace is None:
        return None
    pct = float(pace.get("pct") or 0.5)
    n = int(pace["count"])
    if pct >= TAG_HI:
        word, where = "Aggressive", f"earlier into fights than {pct:.0%} of peers"
    elif pct <= TAG_LO:
        word, where = "Passive", f"later into fights than {1 - pct:.0%} of peers"
    else:
        word = "Even pace"
        where = (
            f"earlier into fights than {pct:.0%} of peers"
            if pct >= 0.5
            else f"later into fights than {1 - pct:.0%} of peers"
        )
    return {
        "headline": word,
        "evidence": f"{where} · {n} games",
        "suggest": "",
        "own": "",
        "peers": "",
        "share": f"{_ordinal(pct)} pct",
        "pct": pct,
        "pos": float(pace.get("pos") or 0.0),
        "left": PACE_ENDS[0],
        "right": PACE_ENDS[1],
        **_shape("pace", pace.get("band")),
    }


def _habits(rows: list[dict[str, Any]], side: str | None, *, is_self: bool) -> list[dict[str, Any]]:
    """Pick the habits a card always shows.

    Flagged reads first, whichever side (a read about the other side keeps
    its side in its words), this side first among them, then by distance
    from peers.
    """
    picks = []
    for row in rows:
        if row["kind"] not in ("tendency", "positional") or not row.get("persisted"):
            continue
        if _key(row["read"]) not in HABIT_READS:
            continue
        if _beyond(row) < HABIT_MILD and not row.get("flag"):
            # Near the middle of their peers: not a habit, no line.
            continue
        own = _read_side(row["read"])
        other = bool(side and own and own != side)
        picks.append((not row.get("flag"), other, -abs(float(row.get("z") or 0.0)), row))
    picks.sort(key=lambda p: (p[0], p[1], p[2]))
    return [
        _habit_line(row, other=other, is_self=is_self) for _, other, _, row in picks[:MAX_HABITS]
    ]


# The buy block's label per family: the round kind, or the gear tier that
# answers "what do they commonly carry when well equipped".
BLOCK_LABEL = {
    "pistol": "Pistol",
    "after_lost_pistol": "After pistol loss",
    "full": "With 3,900+ gear",
}
TOO_LITTLE = {
    "pistol": "Too little pistol history.",
    "after_lost_pistol": "Too little post-loss history.",
}
ARMOR_WORD = {
    "none": "No armor",
    "light": "Light",
    "heavy": "Heavy",
    "regen": "Regen",
    "unknown": "Armor unknown",
}


def _block(row: dict[str, Any], family: str) -> dict[str, Any]:
    """Lay out one buy family as the panel's block: label, guns, armor.

    The guns line names the leader over the rounds and a close second, or
    every tied gun; a finding on this family (a gun or armor far from
    peers) merges into its line with the rate and the peers' share, so
    the block says it once.
    """
    expect, rounds = row["expect"], int(row["count"])
    pick = row.get("pick") if row.get("show") else None
    if expect.get("tied"):
        names = " / ".join(_weapon_word(o) for o in expect["tied"])
        guns = f"{names}: {expect['tie_count']}/{rounds} each"
    elif expect.get("weapon"):
        guns = f"{_weapon_word(expect['weapon'])} {expect['weapon_count']}/{rounds}"
        if pick and pick["kind"] == "weapon" and pick["option"] == expect["weapon"]:
            guns += f" ({expect['weapon_count'] / rounds:.0%}) · peers {pick['peers']:.0%}"
        if expect.get("second"):
            guns += f" · {_weapon_word(expect['second'])} {expect['second_count']}/{rounds}"
    else:
        guns = f"Weapon unknown {expect.get('unknown', 0)}/{rounds}"
    if expect.get("shield_tied"):
        names = " / ".join(
            ARMOR_WORD[a] if i == 0 else ARMOR_WORD[a].lower()
            for i, a in enumerate(expect["shield_tied"])
        )
        armor = f"{names}: {expect['shield_count']}/{rounds} each"
    else:
        armor = f"{ARMOR_WORD[expect['shield']]} {expect['shield_count']}/{rounds}"
        if pick and pick["kind"] == "shield" and pick["option"] == expect["shield"]:
            armor += f" ({expect['shield_count'] / rounds:.0%}) · peers {pick['peers']:.0%}"
    if expect.get("tied"):
        each = f"{expect.get('tie_count') or 0}/{rounds}"
        pairs = [(o, each) for o in expect["tied"]]
    else:
        counted = {
            expect.get("weapon"): expect.get("weapon_count", 0),
            expect.get("second"): expect.get("second_count", 0),
        }
        pairs = [(o, f"{c}/{rounds}") for o, c in counted.items() if o]
    items = _gun_items(pairs)
    return {
        "label": f"{BLOCK_LABEL[family]} · {rounds} past rounds",
        "guns": guns,
        "armor": armor,
        "items": items,
    }


def scout(
    rows: list[dict[str, Any]],
    *,
    is_self: bool,
    side: str | None,
    round_kind: str | None,
    after_pistol: bool,
    agent: str,
    on_agent: int,
    agents_known: bool,
    matches: int,
    likely: dict[str, Any] | None = None,
) -> dict[str, Any]:
    """Compose the panel's Scout section from a player's lines, as Codex laid it out.

    The habits first, up to three lines every readable player gets: what
    they tend to do against peers, flagged reads first and this side
    first, each with its count. Then one tactical callout at most, a
    weapon habit, a buy habit or an equipment exception that ranked
    first; one buy block, the family for the round coming, else the
    forecast of the gun for this round from the score's context, else on
    an ordinary round with no finding the 3,900+ gear history; one footer with the
    sample and how much of it was on this lobby's agent. The user's own
    card carries no suggestion and no style and collapses on ordinary
    rounds. Everything else stays in the history behind the disclosure.
    """
    min_read = int(norms()["min_read"])
    footer = f"Last {matches} ranked games"
    if agent and matches and agents_known:
        if on_agent == 0:
            footer += f" · none on {agent}"
        elif on_agent == matches:
            footer += f" · all on {agent}"
        else:
            footer += f" · {agent} {on_agent}/{matches}"
    out: dict[str, Any] = {
        "title": "Your history" if is_self else "Scout",
        "tag": None,
        "habits": [],
        "main": None,
        "buy": None,
        "style": None,
        "empty": "",
        "footer": footer,
    }
    if matches < min_read:
        out["empty"] = f"Need {min_read} ranked games for a read."
        return out

    out["tag"] = _tag(rows)
    out["habits"] = _habits(rows, side, is_self=is_self)
    shown = sorted((r for r in rows if r.get("show") and "rank" in r), key=lambda r: r["rank"])
    # The tactical callout: a weapon habit, the buy habit, or an equipment
    # exception outside the round's own family. Behaviour reads are habits.
    main = next(
        (
            r
            for r in shown
            if (r["kind"] == "buy" and r["read"] not in ROUND_KINDS)
            or _key(r["read"]) in ("carry", "force_rate")
        ),
        None,
    )
    if main is not None:
        out["main"] = {
            "headline": main["shown"],
            "evidence": main.get("figure") or main["shown_evidence"],
            "suggest": "" if is_self else main.get("suggest") or "",
        }

    family = round_kind if round_kind in ROUND_KINDS else None
    if family is None and likely is not None and not is_self:
        out["buy"] = _forecast_block(likely)
    elif family is None and round_kind == OTHER_ROUND and not after_pistol and main is None:
        family = None if is_self else "full"
    if family:
        row = next((r for r in rows if r["kind"] == "buy" and r["read"] == family), None)
        if row is not None and int(row["count"]) >= MIN_NOW_ROUNDS:
            out["buy"] = _block(row, family)
        elif family in TOO_LITTLE and main is None:
            out["empty"] = TOO_LITTLE[family]

    if (
        not out["tag"]
        and not out["habits"]
        and not out["main"]
        and not out["buy"]
        and not out["empty"]
    ):
        out["empty"] = f"No strong read for {side}." if side else "No strong read."
    return out


def _fixture(rounds: list[dict[str, Any]], *, stats_for: dict[int, set[str]] | None = None) -> Any:
    """Build a ten-player competitive detail for the self-check, Red p0..p4 against Blue p5..p9.

    `stats_for` names the players with a stats row in each round; by default
    everyone has one. Each round's kills go on their killer's stats row, or
    on any row when the killer has none.
    """
    subjects = [f"p{i}" for i in range(10)]
    players = [
        {
            "subject": s,
            "teamId": "Red" if i < 5 else "Blue",
            "characterId": "add6443a-41bd-e414-f6ad-e58d267f4e95",
            "competitiveTier": 13,
            "stats": {"roundsPlayed": len(rounds)},
        }
        for i, s in enumerate(subjects)
    ]
    results = []
    for n, spec in enumerate(rounds):
        with_stats = set(subjects) if stats_for is None else stats_for.get(n, set(subjects))
        rows: dict[str, dict[str, Any]] = {
            s: {"subject": s, "kills": [], "economy": spec.get("economy", {}).get(s, {})}
            for s in subjects
            if s in with_stats
        }
        for kill in spec.get("kills", []):
            holder = kill["killer"] if kill["killer"] in rows else next(iter(rows), None)
            if holder:
                rows[holder]["kills"].append(kill)
        results.append(
            {
                "roundNum": n,
                "winningTeam": spec.get("winner", "Red"),
                "winningTeamRole": spec.get("role", "Attacker"),
                "playerStats": list(rows.values()),
                "firstBloodPlayer": spec.get("first_blood"),
            }
        )
    return {
        "matchInfo": {"matchId": "fixture", "queueID": "competitive", "mapId": "/Game/Maps/X"},
        "players": players,
        "roundResults": results,
    }


def _self_check() -> None:
    # Keep a release from accidentally bundling raw matches or a growing corpus.
    check(NORMS_PATH.stat().st_size <= 2 * 1024 * 1024, "peer baseline exceeds 2 MiB")
    n = norms()
    pending: list[Any] = [n]
    private_fields = {"puuid", "subject", "gameName", "tagLine", "matchId", "roundResults"}
    while pending:
        value = pending.pop()
        if isinstance(value, dict):
            check(not private_fields.intersection(value), "raw player or match data in baseline")
            pending.extend(value.values())
        elif isinstance(value, list):
            pending.extend(value)
    check(n["bands"][0][2] == "iron-bronze", n["bands"])
    check(band(13) == "silver-gold" and band(27) == "asc+" and band(None) is None, "band")
    check(band(8.5) == "silver-gold" and band(14.5) == "plat-dia", "band between tiers rounds up")
    check(
        _option("Vandal") == "vandal"
        and _option("") == "none"
        and _option("Golden Gun") == "other"
        and _option(None) == "unknown",
        "option",
    )
    check(_flag(0.6, 100) and not _flag(0.4, 100) and not _flag(0.6, 9), "flag rule")
    teams = {"a": "Red", "b": "Blue", "c": "Blue", "d": "Red", "e": "Blue"}
    # Through the ordering: a kill with only a game time sits between two
    # timed ones by the recovered clock, and one with neither goes last.
    kills = _round_kills(
        {
            "playerStats": [
                {
                    "subject": "a",
                    "kills": [
                        {"killer": "a", "victim": "b", "roundTime": 1000, "gameTime": 61000},
                        {"killer": "d", "victim": "e", "gameTime": 62000},
                        {"killer": "c", "victim": "a", "roundTime": 3000, "gameTime": 63000},
                        {"killer": "c", "victim": "d"},
                    ],
                }
            ]
        }
    )
    check([when for when, _ in kills] == [1000.0, 2000.0, 3000.0, math.inf], "recovered order")
    check(_traded(kills, 0, teams), "an unrelated recovered-clock kill does not end the search")
    check(not _traded(kills, 2, teams), "no trade after the last kill")
    # The killer's own death, however recorded, is a boundary: a later-life
    # death cannot be the trade.
    boundary = _round_kills(
        {
            "playerStats": [
                {
                    "subject": "a",
                    "kills": [
                        {"killer": "a", "victim": "b", "roundTime": 1000, "gameTime": 61000},
                        {"killer": "d", "victim": "a", "gameTime": 61500},
                        {"killer": "c", "victim": "a", "roundTime": 2000, "gameTime": 62000},
                    ],
                }
            ]
        }
    )
    check(not _traded(boundary, 0, teams), "an untimed death of the killer ends the search")
    # With an enemy kill nobody can place, the first blood names the opening.
    check(_opening({}, kills, teams) is None, "no opening without a chronology")
    check(
        _opening({"firstBloodPlayer": "a"}, kills, teams) is kills[0][1], "opening by first blood"
    )
    check(_opening({}, kills[:3], teams) is kills[0][1], "opening by clock")
    # A first kill with a game time but no round time keeps its place in the
    # order and is the opening, as flatten.py recovers it.
    rr = {
        "playerStats": [
            {
                "subject": "a",
                "kills": [
                    {"killer": "a", "victim": "b", "gameTime": 61000},
                    {"killer": "a", "victim": "c", "gameTime": 65000, "roundTime": 5000},
                ],
            }
        ]
    }
    ordered = _round_kills(rr)
    check(
        ordered[0][0] == 1000.0 and _opening(rr, ordered, teams) is ordered[0][1], "recovered clock"
    )
    check(
        _xy({}) is None and _xy({"x": "nan", "y": 1}) is None and _xy({"x": 1, "y": 2}) == (1, 2),
        "xy",
    )
    check(match_reads({"matchInfo": {}, "players": [], "roundResults": []}, "a") is None, "empty")
    check(lines_for("a", [], "Bind", teammate=True) == [], "too few")

    # A round without p0's stats row still counts p0's death there and the
    # positional round, as the fight and position tables do, but not toward
    # the round table's openings; an empty location is skipped, not fatal.
    guns = {"finishingDamage": {"damageType": "Weapon"}}
    md = _fixture(
        [
            {
                "kills": [
                    {
                        "killer": "p5",
                        "victim": "p0",
                        "roundTime": 2000,
                        "victimLocation": {"x": 0, "y": 0},
                        "playerLocations": [{"subject": "p5", "location": {"x": 300, "y": 400}}],
                        **guns,
                    }
                ],
                "economy": {"p0": {"weapon": "gun-1", "loadoutValue": 800, "spent": 800}},
            },
            {
                "kills": [
                    {
                        "killer": "p5",
                        "victim": "p0",
                        "roundTime": 9000,
                        "victimLocation": {},
                        "playerLocations": [],
                        **guns,
                    }
                ],
            },
        ],
        stats_for={0: {f"p{i}" for i in range(10)}, 1: {f"p{i}" for i in range(1, 10)}},
    )
    places = Places(
        {"callouts": [{"superRegionName": "Attacker Side", "location": {"x": 0, "y": 0}}]}
    )
    names = {"gun-1": "Ghost", "gun-x": None}

    def read_fixture() -> dict[str, Any]:
        return present(
            match_reads(
                md, "p0", weapon_name=names.get, places=places, map_name=lambda _: "Fixture"
            ),
            "fixture",
        )

    reads = read_fixture()
    check(reads["map"] == "Fixture", "map lookup injected")
    check(reads["sums"]["open_all"] == [1.0, 1.0], reads["sums"].get("open_all"))
    check(reads["sums"]["death_t"] == [11000.0, 2.0], reads["sums"].get("death_t"))
    check(reads["positional"]["attack"]["open_rate"] == 1.0, reads["positional"])
    check(reads["positional"]["attack"]["open_death_rate"] == 1.0, "death counted without stats")
    check(reads["positional"]["attack"]["forward"] == 0.0, "victim location at the opening")
    check(
        reads["buys"]["pistol"]["ghost"] == 1 and reads["buys"]["pistol"]["rounds"] == 1,
        "pistol buy",
    )
    check(reads["unresolved_weapons"] == 0 and reads["carried"] == {"Ghost": 1}, "carried")
    # A weapon id the catalogue cannot name is unknown, never no gun.
    md["roundResults"][0]["playerStats"][0]["economy"]["weapon"] = "gun-x"
    reads = read_fixture()
    check(
        reads["unresolved_weapons"] == 1 and reads["buys"]["pistol"].get("unknown") == 1, "unknown"
    )
    check("none" not in reads["buys"]["pistol"], "no false no-gun")
    # A pistol round with no equipment value still belongs to the pistol family.
    md["roundResults"][0]["playerStats"][0]["economy"] = {"weapon": "gun-1"}
    reads = read_fixture()
    check(reads["buys"]["pistol"]["rounds"] == 1, "pistol without loadout value")
    # A family whose weapons the catalogue cannot name expects no weapon.
    unknown_only = {k: {"rounds": 0.0, "spent": 0.0} for k in KINDS}
    unknown_only["pistol"] = {"rounds": 5.0, "spent": 0.0, "unknown": 5.0, "shield_none": 5.0}
    rows = [
        {**reads, "sums": {}, "positional": {s: {} for s in SIDES}, "buys": unknown_only}
        for _ in range(5)
    ]
    buy = next(row for row in lines_for("p0", rows, "Bind", teammate=True) if row["kind"] == "buy")
    check(buy["expect"]["weapon"] is None and "weapon unknown 25/25" in buy["text"], buy["text"])
    # One detail that fails inside its rounds costs that match, with a
    # warning, and not the lobby. A detail that is not a match is skipped.
    broken = _fixture([{"kills": []}])
    broken["roundResults"][0]["roundNum"] = "x"
    caught: list[logging.LogRecord] = []

    class _Catch(logging.Handler):
        def emit(self, record: logging.LogRecord) -> None:
            caught.append(record)

    handler = _Catch()
    LOG.addHandler(handler)
    try:
        lobby = [{"puuid": "p0", "team": "Red"}, {"puuid": "p5", "team": "Blue"}]
        out = brief(
            lobby,
            "Bind",
            {"p0": [broken, {"matchInfo": ["malformed"]}], "p5": []},
            self_puuid="p0",
            weapon_name=names.get,
            places=places,
            map_lookup=lambda _: "Fixture",
        )
    finally:
        LOG.removeHandler(handler)
    check(set(out) == {"p0", "p5"} and out["p0"] == [] and out["p5"] == [], "bad detail isolated")
    check(len(caught) == 1 and "skipped match fixture" in caught[0].getMessage(), caught)
    check("ValueError" in caught[0].getMessage(), caught[0].getMessage())
    # A lost pistol is known without a stats row: the next round is still
    # a round after a lost pistol.
    md = _fixture(
        [
            {"kills": [], "winner": "Blue"},
            {"kills": [], "economy": {"p0": {"weapon": "gun-1", "loadoutValue": 2500}}},
        ],
        stats_for={0: {f"p{i}" for i in range(1, 10)}, 1: {f"p{i}" for i in range(10)}},
    )
    reads = read_fixture()
    check(reads["buys"]["after_lost_pistol"]["rounds"] == 1, "lost pistol known without stats")
    check(reads["sums"]["force_rate"] == [1.0, 1.0], reads["sums"].get("force_rate"))
    # A tie between guns names no gun.
    tie = {k: {"rounds": 0.0, "spent": 0.0} for k in KINDS}
    tie["pistol"] = {"rounds": 4.0, "spent": 0.0, "ghost": 2.0, "sheriff": 2.0, "shield_none": 4.0}
    tie_rows = [
        {**reads, "sums": {}, "positional": {s: {} for s in SIDES}, "buys": tie} for _ in range(5)
    ]
    buy = next(
        row for row in lines_for("p0", tie_rows, "Bind", teammate=True) if row["kind"] == "buy"
    )
    check(buy["expect"]["weapon"] is None and buy["expect"]["tied"] == ["ghost", "sheriff"], buy)
    check("gun tie: Ghost, Sheriff 10/20 each" in buy["text"], buy["text"])
    check("gun tie: Ghost, Sheriff 10/20 each" in buy["headline"], buy["headline"])
    check("Gun tie" in buy["evidence"], buy["evidence"])
    # attach() composes for the board in hand and holds the spec's invariants:
    # only players on the board, at least min_read measurements behind every
    # line, composed for this map and this side, nothing left raw.
    rows = [
        {
            **reads,
            "match_id": f"m{i}",
            "map": "Bind",
            "sums": {"open_all": [8.0, 20.0]},
            "buys": unknown_only,
        }
        for i in range(6)
    ]
    board: dict[str, Any] = {
        "map": "Bind",
        "selfTeam": "Red",
        "players": [
            {"puuid": "p0", "team": "Red", "reads": rows},
            {"puuid": "p5", "team": "Blue", "reads": rows},
            {"puuid": "p6", "team": "Blue", "reads": rows[:2]},
            {"puuid": "p7", "team": "Blue"},
        ],
    }
    attach(board)
    players: dict[str, dict[str, Any]] = {p["puuid"]: p for p in board["players"]}
    check(all("reads" not in p for p in players.values()), "raw measurements dropped")
    check(players["p6"]["brief"] == [] and players["p7"]["brief"] == [], "too few, no lines")
    check(players["p0"]["brief"] and players["p5"]["brief"], "lines for the readable players")
    check(
        all(row["headline"] and row["evidence"] is not None for row in players["p0"]["brief"]),
        "every line has a headline and its evidence",
    )
    styles = [row for row in players["p0"]["brief"] if row["kind"] == "style"]
    check([row["read"] for row in styles] == ["pace"], styles)
    check(styles[0]["count"] == 6 and 0 < float(styles[0]["pct"]) < 1, styles[0])
    check(not styles[0]["show"], "the pace score stays off the panel")
    check(
        all(abs(float(row["value"])) >= STYLE_Z or not row["words"] for row in styles), "no typical"
    )
    # A weapon never carried is no line; one carried is, named for the weapon.
    carries = [row for row in players["p0"]["brief"] if row["read"].startswith("carry_")]
    check([row["read"] for row in carries] == ["carry_ghost"], carries)
    check(carries[0]["headline"] == "Rounds with Ghost", carries[0]["headline"])
    # Round one is a pistol round for everyone; round two after a lost
    # pistol is marked for the team that lost it, read from the score.
    round_one: dict[str, Any] = {
        **board,
        "matchId": "m1",
        "score": {"ally": 0, "enemy": 0, "round": 1},
    }
    round_one["players"] = [{"puuid": "p0", "team": "Red", "reads": rows}]
    attach(round_one)
    check(
        any(r.get("now") and r["read"] == "pistol" for r in round_one["players"][0]["brief"]),
        "pistol now",
    )
    # On a round of another kind the pistol family is nobody's "now" and
    # earns no slot; the scope rides on every line.
    round_five: dict[str, Any] = {
        **board,
        "matchId": "m1",
        "score": {"ally": 2, "enemy": 2, "round": 5},
    }
    round_five["players"] = [{"puuid": "p0", "team": "Red", "reads": rows}]
    attach(round_five)
    five = round_five["players"][0]["brief"]
    check(not any(r.get("now") for r in five), "no family is round five's")
    check(not any(r.get("show") and r["read"] in ROUND_KINDS for r in five), "pistol yields")
    check(all("scope" in r for r in five), "scope on every line")
    # The section: round two after a lost pistol shows that family's block
    # with the tie named; round one the pistol block; round five no block
    # and the user's own card collapsed to its footer.
    rows_lost = [{**r, "buys": tie} for r in rows]
    round_two: dict[str, Any] = {
        **board,
        "matchId": "m1",
        "score": {"ally": 0, "enemy": 1, "round": 2},
        "players": [{"puuid": "p0", "team": "Red", "reads": rows_lost, "agent": "Jett"}],
    }
    attach(round_two)
    sc = round_two["players"][0]["scout"]
    check(sc["title"] == "Scout" and sc["footer"] == "Last 6 ranked games · all on Jett", sc)
    check(sc["buy"] is None and sc["empty"] == "Too little post-loss history.", sc)
    check(sc["main"] is None and sc["habits"], sc)
    check(all("of peers" in h["evidence"] for h in sc["habits"]), sc["habits"])
    check(all(h["headline"] and "%" not in h["headline"] for h in sc["habits"]), sc["habits"])
    check(
        all(0 < h["pct"] < 1 and (h["pos"] > 0) == (h["pct"] > 0.5) for h in sc["habits"]), "curve"
    )
    check(all(h["left"] and h["right"] for h in sc["habits"]), "curve ends named")
    check(all(h["curve"] and h["lo"] <= h["pos"] <= h["hi"] for h in sc["habits"]), "real curves")
    check(all(h["share"].endswith(" pct") for h in sc["habits"]), "the chart's number")
    check(
        _ordinal(0.85) == "85th" and _ordinal(0.91) == "91st" and _ordinal(0.12) == "12th",
        "ordinal",
    )
    check(percentile(0.0, "open_all", "silver-gold", 10) > 0.4, "the median sits near the middle")
    check(percentile(9.0, "open_all", "silver-gold", 10) == 1.0, "beyond every peer")
    check(sc["tag"] and sc["tag"]["headline"] in ("Aggressive", "Passive", "Even pace"), sc["tag"])
    check(abs(percentile(0.0, "no_such_read", "silver-gold", 10) - 0.5) < 1e-9, "normal median")
    check(percentile(1.0, "no_such_read", "silver-gold", 10) > 0.85, "a sd above is well up")
    check(all(h["headline"] and not h["suggest"] for h in sc["habits"]), sc["habits"])
    tie_lost = dict(tie)
    tie_lost["after_lost_pistol"] = {
        "rounds": 2.0,
        "spent": 0.0,
        "ghost": 1.0,
        "sheriff": 1.0,
        "shield_none": 2.0,
    }
    round_two["players"] = [
        {"puuid": "p0", "team": "Red", "reads": [{**r, "buys": tie_lost} for r in rows]}
    ]
    attach(round_two)
    sc = round_two["players"][0]["scout"]
    check(sc["buy"]["label"] == "After pistol loss · 12 past rounds", sc["buy"])
    check(sc["buy"]["guns"] == "Ghost / Sheriff: 6/12 each", sc["buy"])
    check(sc["buy"]["armor"].startswith("No armor 12/12"), sc["buy"])
    check(sc["empty"] == "", sc)
    round_one["players"] = [{"puuid": "p0", "team": "Red", "reads": rows_lost, "isSelf": True}]
    attach(round_one)
    sc = round_one["players"][0]["scout"]
    check(sc["title"] == "Your history" and sc["buy"]["label"] == "Pistol · 24 past rounds", sc)
    check(sc["buy"]["guns"] == "Ghost / Sheriff: 12/24 each" and sc["habits"], sc)
    round_five["players"] = [{"puuid": "p0", "team": "Red", "reads": rows_lost, "isSelf": True}]
    attach(round_five)
    sc = round_five["players"][0]["scout"]
    check(sc["buy"] is None and sc["empty"] == "" and sc["main"] is None, sc)
    check(sc["habits"] and all(not h["suggest"] for h in sc["habits"]), "no coaching on own card")
    # The round's context from the scores seen: the pistol, the second
    # round by the pistol's result, later rounds by the last result and
    # the losses in a row, None where a score was missed.
    seen = {1: (0, 0), 2: (0, 1), 3: (0, 2), 4: (1, 2), 5: (1, 3)}
    check(round_ctx(seen, 1, "ally") == "pistol", "pistol ctx")
    check(round_ctx(seen, 2, "ally") == "second|lost", round_ctx(seen, 2, "ally"))
    check(round_ctx(seen, 2, "enemy") == "second|won", round_ctx(seen, 2, "enemy"))
    check(round_ctx(seen, 3, "ally") == "later|lost|2", round_ctx(seen, 3, "ally"))
    check(round_ctx(seen, 4, "ally") == "later|won|0", round_ctx(seen, 4, "ally"))
    check(round_ctx(seen, 5, "ally") == "later|lost|1", round_ctx(seen, 5, "ally"))
    check(round_ctx({2: (0, 1), 5: (1, 3)}, 5, "ally") is None, "missed scores, no ctx")
    check(round_ctx({13: (6, 6), 14: (7, 6)}, 14, "ally") == "second|won", "second half")
    # The forecast: their Vandal on full buys, the band's odds of a full
    # buy after a won round, and a close Phantom from peers.
    buys_full = {k: {"rounds": 0.0, "spent": 0.0} for k in KINDS}
    buys_full["full"] = {"rounds": 4.0, "spent": 0.0, "vandal": 4.0, "shield_heavy": 4.0}
    full_rows = [{**r, "buys": buys_full} for r in rows]
    lines = lines_for("p0", full_rows, "Bind", teammate=True)
    likely = forecast(lines, "later|won|0", "silver-gold")
    check(likely is not None and likely["guns"][0][0] == "vandal", likely)
    check(likely is not None and likely["armor"][0][0] == "heavy", likely)
    check(forecast(lines, "pistol", "silver-gold") is None, "no forecast on the pistol")
    check(forecast(lines, None, "silver-gold") is None, "no forecast without a context")
    _SCORES["m7"] = {1: (0, 0), 2: (1, 0), 3: (2, 0), 4: (3, 0)}
    round_five_won: dict[str, Any] = {
        **board,
        "matchId": "m7",
        "score": {"ally": 4, "enemy": 0, "round": 5},
        "players": [{"puuid": "p5", "team": "Blue", "reads": full_rows}],
    }
    attach(round_five_won)
    sc = round_five_won["players"][0]["scout"]
    check(sc["buy"]["label"] == "Likely this round · after three or more lost rounds", sc["buy"])
    check(sc["buy"]["guns"].startswith("Vandal"), sc["buy"])
    check(any(r["kind"] == "forecast" for r in round_five_won["players"][0]["brief"]), "line")
    few = {**board, "players": [{"puuid": "p6", "team": "Blue", "reads": rows[:2]}]}
    attach(few)
    sc = few["players"][0]["scout"]
    check(sc["empty"] == "Need 5 ranked games for a read." and "Last 2" in sc["footer"], sc)
    check(
        round_context({"matchId": "m1", "score": {"ally": 0, "enemy": 1, "round": 2}})
        == (2, "ally"),
        "lost",
    )
    check(
        round_context({"matchId": "m1", "score": {"ally": 1, "enemy": 0, "round": 2}})
        == (2, "enemy"),
        "won",
    )
    _SCORES["m2"] = {13: (6, 6)}
    check(
        round_context({"matchId": "m2", "score": {"ally": 6, "enemy": 7, "round": 14}})
        == (14, "ally"),
        "r14",
    )
    check(
        round_context({"matchId": "m3", "score": {"ally": 6, "enemy": 7, "round": 14}})
        == (14, None),
        "unknown",
    )
    shown_rows = [row for row in players["p0"]["brief"] if row.get("show")]
    check(len([r for r in shown_rows if r["kind"] != "style"]) <= MAX_SHOWN, "at most two shown")
    check(
        all(row.get("flag") or row["kind"] in ("buy", "style") for row in shown_rows),
        "shown lines earned it",
    )
    check(
        all(
            row.get("shown") and row.get("shown_evidence")
            for row in shown_rows
            if row["kind"] != "style"
        ),
        "shown words",
    )
    first = players["p0"]["brief"][0]
    check(first["kind"] in ("tendency", "positional") and first["flag"], first)
    check(first.get("show"), "a shown line leads")
    ranks = [bool(row.get("show")) for row in players["p0"]["brief"]]
    check(ranks == sorted(ranks, reverse=True), "shown lines come first")
    check(
        all(row["matches"] >= int(norms()["min_read"]) for row in players["p0"]["brief"]),
        "enough behind every line",
    )
    check(
        all(row["for"] == "teammate" for row in players["p0"]["brief"])
        and all(row["for"] == "opponent" for row in players["p5"]["brief"]),
        "composed for this side of the lobby",
    )
    check(
        all(
            "on Bind" in row["text"]
            for row in players["p0"]["brief"]
            if row["kind"] == "positional"
        ),
        "composed for this map",
    )


if __name__ == "__main__":
    _self_check()

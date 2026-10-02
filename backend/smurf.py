# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""Whether an account looks like it is playing under its real rank.

Every sign is weighed rather than counted, and the reasons go out with the
flag, so the panel can say why.
"""

from __future__ import annotations

from common.check import check
from vconstants import rank_from_tier

# Tier 20 is Diamond 3. Tiers run three to a rank from Iron 1 at 3, so Diamond
# is 18 to 20 and Immortal starts at 24.
#
# Every signal carries a weight rather than a vote, because they are not the
# same size. A level 40 account whose peak is Immortal is most of an argument
# by itself, and one good headshot percentage is a hint. Two points is a flag.
_SMURF_PEAK_TIER = 20
_SMURF_LEVEL = 60
_SMURF_KD = 1.35
_SMURF_KD_STRONG = 1.8
_SMURF_KD_LEVEL = 80
_SMURF_KD_MATCHES = 5
_SMURF_WR = 62.0
_SMURF_WR_STRONG = 70.0
_SMURF_WR_GAMES = 15
_SMURF_WR_GAMES_STRONG = 20
_SMURF_WR_LEVEL = 100
_SMURF_HS = 30.0
_SMURF_GAP_TIERS = 3
_SMURF_GAP_STRONG = 6
_SMURF_FLAG_SCORE = 2


def smurf_signals(
    *,
    level: int | None,
    peak_tier: int | None,
    rank_tier: int | None,
    kd: float | None,
    win_rate: float | None,
    games: int | None,
    kd_matches: int | None = None,
    hs: float | None = None,
) -> list[tuple[str, int]]:
    """Every signal that fires, each with what it is worth.

    A low level is what makes an ordinary number suspicious, so most signals
    only count under one. The strong versions count at any level, hidden
    included, since an account two whole ranks under its peak is worth a look
    however old it is.
    """
    reasons: list[tuple[str, int]] = []
    lvl = level or 0
    # Zero is a hidden level.
    fresh = lvl > 0
    peak = peak_tier or 0
    now = rank_tier or 0
    # An unranked account has no rank to measure a gap from.
    gap = peak - now if now >= 3 else 0
    matches = kd_matches or 0
    high_peak = fresh and lvl < _SMURF_LEVEL and peak >= _SMURF_PEAK_TIER
    if high_peak:
        reasons.append((f"Level {lvl}, peak {rank_from_tier(peak_tier)['name']}", 2))
    # Playing well below their own peak. Skipped when the peak already spoke
    # above, because two lines saying the same thing about the same account is
    # not two pieces of evidence.
    elif gap >= _SMURF_GAP_STRONG:
        reasons.append((f"{gap} ranks below their peak of {rank_from_tier(peak)['name']}", 2))
    elif fresh and lvl < _SMURF_LEVEL and peak and gap >= _SMURF_GAP_TIERS:
        reasons.append((f"{gap} ranks below peak", 1))
    # The K/D is the last few matches, not a career, and three good games is
    # something anybody has, so under five matches it is not evidence at all.
    if kd is not None and kd >= _SMURF_KD and matches >= _SMURF_KD_MATCHES:
        low = fresh and lvl < _SMURF_KD_LEVEL
        said = f"K/D {kd} at level {lvl}" if low else f"K/D {kd} over {matches} games"
        if kd >= _SMURF_KD_STRONG:
            reasons.append((said, 2))
        elif low:
            reasons.append((said, 1))
    if win_rate is not None and win_rate >= _SMURF_WR and (games or 0) >= _SMURF_WR_GAMES:
        strong = win_rate >= _SMURF_WR_STRONG and (games or 0) >= _SMURF_WR_GAMES_STRONG
        if strong:
            reasons.append((f"Won {win_rate}% of {games} games", 2))
        elif fresh and lvl < _SMURF_WR_LEVEL:
            reasons.append((f"Won {win_rate}% of {games} games", 1))
    # Aim counts at any level, but only as a hint, off the K/D's own sample.
    if hs is not None and hs >= _SMURF_HS and matches >= _SMURF_KD_MATCHES:
        reasons.append((f"{round(hs)}% headshots", 1))
    return reasons


def compute_smurf(
    *,
    level: int | None,
    peak_tier: int | None,
    rank_tier: int | None,
    kd: float | None,
    win_rate: float | None,
    games: int | None,
    kd_matches: int | None = None,
    hs: float | None = None,
) -> tuple[bool, list[str]]:
    """Return whether a player looks like a smurf, and the reasons why."""
    signals = smurf_signals(
        level=level,
        peak_tier=peak_tier,
        rank_tier=rank_tier,
        kd=kd,
        win_rate=win_rate,
        games=games,
        kd_matches=kd_matches,
        hs=hs,
    )
    if not signals:
        return False, []
    # Two points is a flag: one strong signal, or two ordinary ones agreeing.
    # One ordinary number on a fresh account is how a good week gets called a
    # smurf, so it is not enough alone. The reasons come back either way, and
    # one point still shows in the panel as worth a look, without the accusation.
    score = sum(weight for _, weight in signals)
    return score >= _SMURF_FLAG_SCORE, [text for text, _ in signals]


def _self_check() -> None:
    # The flag. A hidden level only loses the signals that are "for that
    # level". A fall of two whole ranks and a dominant act still count, and a
    # K/D off three matches is not evidence.
    hidden, why = compute_smurf(
        level=0, peak_tier=26, rank_tier=12, kd=2.5, win_rate=90.0, games=50
    )
    check(
        hidden
        and why
        == [
            "14 ranks below their peak of Immortal 3",
            "Won 90.0% of 50 games",
        ],
        why,
    )
    check(
        compute_smurf(level=0, peak_tier=12, rank_tier=12, kd=1.5, win_rate=64.0, games=30)
        == (
            False,
            [],
        )
    )
    # An old account two ranks under its peak is worth a look on that alone,
    # and one rank under is nothing. Unranked has no gap at all.
    fallen, why = compute_smurf(
        level=124, peak_tier=19, rank_tier=12, kd=None, win_rate=None, games=None
    )
    check(fallen and why == ["7 ranks below their peak of Diamond 2"], why)
    check(
        compute_smurf(level=124, peak_tier=15, rank_tier=12, kd=None, win_rate=None, games=None)[1]
        == []
    )
    check(
        compute_smurf(level=124, peak_tier=26, rank_tier=0, kd=None, win_rate=None, games=None)[1]
        == []
    )
    # A dominant run flags at any level, and says how many games it was.
    dominant, why = compute_smurf(
        level=300, peak_tier=12, rank_tier=12, kd=2.1, win_rate=None, games=None, kd_matches=5
    )
    check(dominant and why == ["K/D 2.1 over 5 games"], why)
    hot, why = compute_smurf(
        level=41, peak_tier=12, rank_tier=12, kd=1.9, win_rate=None, games=None, kd_matches=3
    )
    check((hot, why) == (False, []), why)
    hot, why = compute_smurf(
        level=41, peak_tier=12, rank_tier=12, kd=1.9, win_rate=None, games=None, kd_matches=5
    )
    check(hot and why == ["K/D 1.9 at level 41"], why)
    # 1.4 is the same signal at half the weight: shown, not flagged.
    warm, why = compute_smurf(
        level=41, peak_tier=12, rank_tier=12, kd=1.4, win_rate=None, games=None, kd_matches=5
    )
    check(not warm and why == ["K/D 1.4 at level 41"], why)
    # Two ordinary signals agreeing is a flag.
    both, why = compute_smurf(
        level=41,
        peak_tier=12,
        rank_tier=12,
        kd=1.4,
        win_rate=None,
        games=None,
        kd_matches=5,
        hs=34.0,
    )
    check(both and why == ["K/D 1.4 at level 41", "34% headshots"], why)
    # Aim needs the same sample the K/D needs, and at any level it is a hint
    # that takes something else to flag.
    check(
        compute_smurf(
            level=41, peak_tier=12, rank_tier=12, kd=None, win_rate=None, games=None, hs=40.0
        )[1]
        == []
    )
    aim, why = compute_smurf(
        level=90,
        peak_tier=12,
        rank_tier=12,
        kd=None,
        win_rate=None,
        games=None,
        kd_matches=5,
        hs=40.0,
    )
    check(not aim and why == ["40% headshots"], why)
    # Sitting a little below their own peak counts on a low level account.
    gap, why = compute_smurf(
        level=41, peak_tier=17, rank_tier=12, kd=None, win_rate=None, games=None
    )
    check(not gap and why == ["5 ranks below peak"], why)
    # A low level on a high peak is one signal and enough on its own.
    flagged, why = compute_smurf(
        level=41, peak_tier=24, rank_tier=12, kd=None, win_rate=None, games=None
    )
    check(flagged and why == ["Level 41, peak Immortal 1"], why)
    # Diamond 3 is where the low level peak signal starts. Below it, the same
    # account is judged by its gap alone.
    near, why = compute_smurf(
        level=41, peak_tier=19, rank_tier=15, kd=None, win_rate=None, games=None
    )
    check(not near and why == ["4 ranks below peak"], why)
    # Same rank as their peak, nothing to say.
    check(
        compute_smurf(level=41, peak_tier=12, rank_tier=12, kd=None, win_rate=None, games=None)[1]
        == []
    )
    # Above 60 the level stops arguing for itself: one ordinary number is not
    # enough, two are, and an extreme one is on its own.
    one, why = compute_smurf(
        level=75, peak_tier=12, rank_tier=12, kd=1.5, win_rate=None, games=None, kd_matches=5
    )
    check(not one and why == ["K/D 1.5 at level 75"], why)
    two, why = compute_smurf(
        level=75, peak_tier=12, rank_tier=12, kd=1.5, win_rate=64.0, games=30, kd_matches=5
    )
    check(two and len(why) == 2, why)
    alone, why = compute_smurf(
        level=75, peak_tier=12, rank_tier=12, kd=None, win_rate=88.0, games=40
    )
    check(alone and why == ["Won 88.0% of 40 games"], why)
    # A win rate off nine games is not a win rate.
    check(
        compute_smurf(level=75, peak_tier=12, rank_tier=12, kd=None, win_rate=80.0, games=9)[1]
        == []
    )

    print("smurf self-check OK (peak gaps, dominant runs, aim, thin samples)")


if __name__ == "__main__":
    _self_check()

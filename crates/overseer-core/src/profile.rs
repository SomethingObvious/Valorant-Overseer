//! One account's history, typed: the answer to a `profile` request.
//!
//! The board carries what fits on a row. This is everything else about one
//! person, and it only exists because somebody clicked them: the last few
//! matches with the rating after each, which guns they actually reach for,
//! who they keep turning up with, and what they do on the round after a lost
//! pistol.
//!
//! Same rule as [`crate::board`]: every field optional, because the socket is
//! a trust boundary and Riot drops fields mid patch. A history that half
//! arrived is a panel with gaps, never a window that closed.
//!
//! The shape is `handle_data_request("profile")` in `backend/app.py`, which is
//! `live_match.player_career` plus `_career_summary`, and the names match
//! across all three.

use serde::Deserialize;

/// Somebody who was on your side in one of these matches.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CoPlayer {
    /// Their account id.
    pub puuid: Option<String>,
    /// Their name and tag.
    pub name: Option<String>,
    /// How many of these matches you both appeared in.
    pub shared_matches: Option<u32>,
    /// What they played across them.
    pub agents: Vec<String>,
    /// Whether that is enough to call it a duo rather than a coincidence.
    pub is_party: bool,
}

/// One weapon and how much of their killing it did.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Gun {
    /// Vandal, Phantom, Operator.
    pub name: Option<String>,
    /// Kills with it across this history.
    pub kills: Option<u32>,
    /// What share of their kills that is, as a percentage.
    pub share: Option<u32>,
}

/// What they do on the round after losing a pistol.
///
/// The single most readable habit in the game: a player who forces every time
/// is a player whose second round you can plan for.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ForceHabit {
    /// How many times they forced.
    pub forced: Option<u32>,
    /// How many times they could have.
    pub chances: Option<u32>,
    /// The first over the second, as a percentage.
    pub pct: Option<u32>,
}

/// What they take into the round after winning a pistol.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BonusBuy {
    /// The weapon.
    pub name: Option<String>,
    /// How many bonus rounds they took it into.
    pub rounds: Option<u32>,
    /// What share of their bonus rounds that is.
    pub share: Option<u32>,
}

/// The averages across the whole history, so a single odd match does not read
/// as a trend.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Averages {
    /// How many matches these numbers are over, which is the one number that
    /// says how much to trust the rest.
    pub games: Option<u32>,
    /// How many of them were won.
    pub wins: Option<u32>,
    /// The second over the first, as a percentage.
    pub win_rate: Option<f64>,
    /// Kills per match.
    pub kills: Option<f64>,
    /// Deaths per match.
    pub deaths: Option<f64>,
    /// Assists per match.
    pub assists: Option<f64>,
    /// Total kills over total deaths, which is not the mean of the per match
    /// ratios and should not be presented as one.
    pub kd: Option<f64>,
    /// Headshot percentage.
    pub hs_pct: Option<f64>,
}

/// One match out of the history.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CareerMatch {
    /// Riot's own id for it.
    pub match_id: Option<String>,
    /// Where it was played.
    pub map: Option<String>,
    /// Competitive, Unrated, Swiftplay.
    pub mode: Option<String>,
    /// When it started, in milliseconds since the epoch.
    pub start_millis: Option<i64>,
    /// Victory, Defeat, Draw.
    pub result: Option<String>,
    /// Rounds won.
    pub score: Option<u32>,
    /// Rounds lost.
    pub opponent_score: Option<u32>,
    /// Who they played.
    pub agent: Option<String>,
    /// The agent's own colour, for the rail beside the row.
    pub agent_color: Option<String>,
    /// Kills.
    pub kills: Option<u32>,
    /// Deaths.
    pub deaths: Option<u32>,
    /// Assists.
    pub assists: Option<u32>,
    /// Kills over deaths for this match alone.
    pub kd: Option<f64>,
    /// Average combat score.
    pub acs: Option<f64>,
    /// Headshot percentage in this match.
    pub hs_pct: Option<f64>,
    /// How many of them queued together.
    pub party_size: Option<u32>,
    /// What the match did to their rating. Absent outside competitive.
    pub rr_delta: Option<i32>,
    /// The rating they were on afterwards.
    pub rr_after: Option<u32>,
    /// The tier they were on afterwards.
    pub tier_after: Option<u32>,
    /// That tier's name.
    pub rank_after: Option<String>,
}

impl CareerMatch {
    /// Whether this match moved a competitive rating, which is the test for
    /// whether it belongs on the rating chart.
    #[must_use]
    pub const fn is_ranked(&self) -> bool {
        self.rr_after.is_some()
    }

    /// Whether it was won, in the one place that has to know how the backend
    /// spells it.
    #[must_use]
    pub fn won(&self) -> bool {
        self.result
            .as_deref()
            .is_some_and(|r| r.eq_ignore_ascii_case("victory"))
    }
}

/// Everything a `profile` request answers with.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Profile {
    /// Whose it is, so a late answer to an abandoned question can be told
    /// apart from the current one by more than its id.
    pub puuid: Option<String>,
    /// `live` or `demo`, which decides whether any of this is real.
    pub source: Option<String>,
    /// Newest first, which is the order everything below assumes.
    pub matches: Vec<CareerMatch>,
    /// The averages over all of them.
    pub averages: Averages,
    /// Who they keep turning up with.
    pub co_players: Vec<CoPlayer>,
    /// Which guns did the killing.
    pub top_guns: Vec<Gun>,
    /// Whether they force after a lost pistol.
    pub force_habit: ForceHabit,
    /// What they buy after winning one.
    pub bonus_buys: Vec<BonusBuy>,
    /// How many bonus rounds there were to buy in.
    pub bonus_rounds: Option<u32>,
}

impl Profile {
    /// The rating after each ranked match, oldest first.
    ///
    /// Reversed on the way out because the history arrives newest first and
    /// every chart in the world runs left to right through time. Unranked
    /// matches are dropped rather than plotted flat: a line that does not
    /// move because nothing was at stake reads as a plateau.
    #[must_use]
    pub fn rating_run(&self) -> Vec<&CareerMatch> {
        let mut run: Vec<&CareerMatch> = self.matches.iter().filter(|m| m.is_ranked()).collect();
        run.reverse();
        run
    }
}

#[cfg(test)]
mod tests {
    use super::Profile;

    /// The shape the backend actually sends, read the way the panel reads it.
    #[test]
    fn a_career_arrives_whole() {
        let raw = r#"{
            "source": "demo",
            "puuid": "abc",
            "matches": [
                {"matchId": "m2", "map": "Icebox", "mode": "Competitive",
                 "result": "Victory", "agent": "KAY/O", "kills": 22, "deaths": 14,
                 "assists": 5, "kd": 1.57, "acs": 264, "hsPct": 31,
                 "rrDelta": 19, "rrAfter": 72, "tierAfter": 18, "rankAfter": "Diamond 3"},
                {"matchId": "m1", "map": "Lotus", "mode": "Unrated",
                 "result": "Defeat", "agent": "Clove", "kills": 11, "deaths": 17}
            ],
            "averages": {"games": 2, "wins": 1, "winRate": 50, "kd": 0.95, "hsPct": 28},
            "coPlayers": [{"puuid": "z", "name": "Day#9932", "sharedMatches": 2,
                           "agents": ["Chamber"], "isParty": true}],
            "topGuns": [{"name": "Vandal", "kills": 140, "share": 62}],
            "forceHabit": {"forced": 5, "chances": 8, "pct": 63},
            "bonusBuys": [{"name": "Spectre", "rounds": 4, "share": 57}],
            "bonusRounds": 7
        }"#;
        let profile: Profile = serde_json::from_str(raw).unwrap();
        assert_eq!(profile.matches.len(), 2);
        assert_eq!(profile.averages.games, Some(2));
        assert_eq!(profile.top_guns.first().and_then(|g| g.share), Some(62));
        assert!(profile.co_players.first().is_some_and(|c| c.is_party));
        assert_eq!(profile.force_habit.pct, Some(63));

        // Only the competitive match is on the rating chart, and the run
        // reads oldest first whichever way the history arrived.
        let run = profile.rating_run();
        assert_eq!(run.len(), 1);
        assert_eq!(run.first().and_then(|m| m.rr_after), Some(72));
        assert!(profile.matches.first().is_some_and(super::CareerMatch::won));
    }

    /// A history that half arrived is a panel with gaps, never a panic.
    #[test]
    fn an_empty_answer_is_still_an_answer() {
        let profile: Profile = serde_json::from_str("{}").unwrap();
        assert!(profile.matches.is_empty());
        assert_eq!(profile.averages.kd, None);
        assert!(profile.rating_run().is_empty());

        // And a field this build has never heard of is a newer backend, not
        // a fault.
        let newer: Profile =
            serde_json::from_str(r#"{"matches": [], "somethingNew": {"a": 1}}"#).unwrap();
        assert!(newer.matches.is_empty());
    }
}

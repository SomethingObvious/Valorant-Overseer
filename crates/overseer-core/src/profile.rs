//! One account's history, typed: what `handle_data_request("profile")` in
//! `backend/app.py` sends when a player is clicked, every field optional as in
//! [`crate::board`].

use serde::Deserialize;

use crate::board::number;

/// Somebody who was on your side in one of these matches.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CoPlayer {
    /// Their account id.
    pub puuid: Option<String>,
    /// Their name and tag.
    pub name: Option<String>,
    /// How many of these matches you both appeared in.
    #[serde(deserialize_with = "number::whole")]
    pub shared_matches: Option<u32>,
    /// What they played across them.
    pub agents: Vec<String>,
}

/// One weapon and how much of their killing it did.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Gun {
    /// Vandal, Phantom, Operator.
    pub name: Option<String>,
    /// Kills with it across this history.
    #[serde(deserialize_with = "number::whole")]
    pub kills: Option<u32>,
    /// What share of their kills that is, as a percentage.
    #[serde(deserialize_with = "number::whole")]
    pub share: Option<u32>,
}

/// How often they force buy after losing a pistol round, which is the most
/// readable habit in the game.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ForceHabit {
    /// How many times they could have.
    #[serde(deserialize_with = "number::whole")]
    pub chances: Option<u32>,
    /// The first over the second, as a percentage.
    #[serde(deserialize_with = "number::whole")]
    pub pct: Option<u32>,
}

/// What they take into the round after winning a pistol.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BonusBuy {
    /// The weapon.
    pub name: Option<String>,
    /// How many bonus rounds they took it into.
    #[serde(deserialize_with = "number::whole")]
    pub rounds: Option<u32>,
    /// What share of their bonus rounds that is.
    #[serde(deserialize_with = "number::whole")]
    pub share: Option<u32>,
}

/// The averages across the whole history, so one odd match does not read as a
/// trend.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Averages {
    /// How many matches these numbers cover, which says how far to trust them.
    #[serde(deserialize_with = "number::whole")]
    pub games: Option<u32>,
    /// How many of them were won.
    #[serde(deserialize_with = "number::whole")]
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
    /// ratios.
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
    /// Victory, Defeat, Draw.
    pub result: Option<String>,
    /// Rounds won.
    #[serde(deserialize_with = "number::whole")]
    pub score: Option<u32>,
    /// Who they played.
    pub agent: Option<String>,
    /// The agent's own colour, for the rail beside the row.
    pub agent_color: Option<String>,
    /// Kills.
    #[serde(deserialize_with = "number::whole")]
    pub kills: Option<u32>,
    /// Deaths.
    #[serde(deserialize_with = "number::whole")]
    pub deaths: Option<u32>,
    /// Assists.
    #[serde(deserialize_with = "number::whole")]
    pub assists: Option<u32>,
    /// Kills over deaths for this match alone.
    pub kd: Option<f64>,
    /// Average combat score.
    pub acs: Option<f64>,
    /// Headshot percentage in this match.
    pub hs_pct: Option<f64>,
    /// What the match did to their rating. Absent outside competitive.
    #[serde(deserialize_with = "number::signed")]
    pub rr_delta: Option<i64>,
    /// The rating they were on afterwards.
    #[serde(deserialize_with = "number::whole")]
    pub rr_after: Option<u32>,
    /// The tier they were on afterwards.
    #[serde(deserialize_with = "number::whole")]
    pub tier_after: Option<u32>,
    /// That tier's name.
    pub rank_after: Option<String>,
}

impl CareerMatch {
    /// Whether this match moved a competitive rating, which puts it on the
    /// rating chart.
    #[must_use]
    pub const fn is_ranked(&self) -> bool {
        self.rr_after.is_some()
    }

    /// Whether it was won, the one place that knows how the backend spells it.
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
    /// Whose it is, so a late answer to an abandoned question is caught by
    /// more than its id.
    pub puuid: Option<String>,
    /// `live` or `demo`, which decides whether any of this is real.
    pub source: Option<String>,
    /// Newest first, which is the order everything below assumes.
    pub matches: Vec<CareerMatch>,
    /// Their last twenty ranked games, newest first, since `matches` mixes in
    /// every mode.
    pub rating: Vec<CareerMatch>,
    /// The averages over all of them.
    pub averages: Averages,
    /// Who they keep turning up with.
    pub co_players: Vec<CoPlayer>,
    /// `log` when those came from every lobby this app has recorded with
    /// you, rather than from these matches.
    pub co_players_from: Option<String>,
    /// Which guns did the killing.
    pub top_guns: Vec<Gun>,
    /// Whether they force after a lost pistol.
    pub force_habit: ForceHabit,
    /// What they buy after winning one.
    pub bonus_buys: Vec<BonusBuy>,
    /// How many bonus rounds there were to buy in.
    #[serde(deserialize_with = "number::whole")]
    pub bonus_rounds: Option<u32>,
}

impl Profile {
    /// The rating after each ranked match, oldest first, taken from `matches`
    /// when an older backend sent no `rating`.
    #[must_use]
    pub fn rating_run(&self) -> Vec<&CareerMatch> {
        let source = if self.rating.is_empty() {
            &self.matches
        } else {
            &self.rating
        };
        // Unranked matches are dropped rather than plotted flat, because a line
        // that does not move when nothing was at stake reads as a plateau.
        let mut run: Vec<&CareerMatch> = source.iter().filter(|m| m.is_ranked()).collect();
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
        assert_eq!(profile.force_habit.pct, Some(63));

        // Only the competitive match is on the rating chart, and the run
        // reads oldest first whichever way the history arrived.
        let run = profile.rating_run();
        assert_eq!(run.len(), 1);
        assert_eq!(run.first().and_then(|m| m.rr_after), Some(72));
        assert!(profile.matches.first().is_some_and(super::CareerMatch::won));
    }

    /// The rating line comes from the rating history when there is one,
    /// however few ranked games are among the last matches.
    #[test]
    fn the_rating_line_is_the_rating_history() {
        let raw = r#"{
            "matches": [
                {"matchId": "u", "mode": "Unrated"},
                {"matchId": "c", "mode": "Competitive", "rrAfter": 40, "tierAfter": 12}
            ],
            "rating": [
                {"matchId": "c", "rrAfter": 40, "tierAfter": 12, "rrDelta": 18},
                {"matchId": "b", "rrAfter": 22, "tierAfter": 12, "rrDelta": -15},
                {"matchId": "a", "rrAfter": 37, "tierAfter": 12, "rrDelta": 20}
            ]
        }"#;
        let profile: Profile = serde_json::from_str(raw).unwrap();
        let run: Vec<Option<&str>> = profile
            .rating_run()
            .iter()
            .map(|m| m.match_id.as_deref())
            .collect();
        assert_eq!(run, [Some("a"), Some("b"), Some("c")]);
    }

    /// Python sends `22.0` as readily as `22`, and one float in a count must
    /// not cost the whole history.
    #[test]
    fn a_whole_number_with_a_decimal_point_still_arrives() {
        let raw = r#"{
            "matches": [{"kills": 22.0, "deaths": 14.4, "rrDelta": -12.6,
                         "startMillis": 1758000000000.0, "rrAfter": 72.0}],
            "averages": {"games": 2.0, "wins": 1.0},
            "coPlayers": [{"sharedMatches": 2.0}],
            "topGuns": [{"kills": 140.0, "share": 61.6}],
            "forceHabit": {"forced": 5.0, "chances": 8.0, "pct": 62.5},
            "bonusBuys": [{"rounds": 4.0, "share": 57.0}],
            "bonusRounds": 7.0
        }"#;
        let profile: Profile = serde_json::from_str(raw).expect("a rounded history would not read");
        let game = profile.matches.first().expect("one match was sent");
        assert_eq!(game.kills, Some(22));
        assert_eq!(game.deaths, Some(14));
        assert_eq!(game.rr_delta, Some(-13));
        assert_eq!(game.rr_after, Some(72));
        assert_eq!(profile.averages.games, Some(2));
        assert_eq!(
            profile.co_players.first().and_then(|c| c.shared_matches),
            Some(2)
        );
        assert_eq!(profile.top_guns.first().and_then(|g| g.share), Some(62));
        assert_eq!(profile.force_habit.pct, Some(63));
        assert_eq!(profile.bonus_buys.first().and_then(|b| b.rounds), Some(4));
        assert_eq!(profile.bonus_rounds, Some(7));
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

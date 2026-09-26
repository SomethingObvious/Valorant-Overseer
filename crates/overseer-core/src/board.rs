//! What the bridge broadcasts, typed.
//!
//! Every field is optional, and that is a decision rather than laziness: the
//! socket is a trust boundary, and Riot's own API omits fields freely mid
//! patch. A malformed or partial frame has to read as a missing value, never as
//! a panic, because the thing on screen when that happens is a live match.
//!
//! This is the subset the window draws today. It grows as the window does, and
//! the names match `backend/live_match.py` so a field can be traced across the
//! two halves by grepping for one word.

use serde::Deserialize;

/// One player's row, as the backend assembled it.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Player {
    /// Riot's account id. The only stable key; names change and repeat.
    pub puuid: Option<String>,
    /// Display name, already joined with the tag line by the backend.
    pub name: Option<String>,
    /// True for the account this machine is signed in as.
    pub is_self: bool,
    /// `Blue` or `Red`, as Riot labels them.
    pub team: Option<String>,
    /// Agent name, absent until they lock in.
    pub agent: Option<String>,
    /// The agent's role, for the rail glyph.
    pub role: Option<String>,
    /// Rank name, for example `Gold 2`.
    pub rank: Option<String>,
    /// Rank tier, three per group from Iron at 3. The colour comes off this.
    pub rank_tier: Option<u32>,
    /// Ranked rating within the current tier.
    pub rr: Option<i64>,
    /// Best rank ever reached.
    pub peak_rank: Option<String>,
    /// Tier of that peak.
    pub peak_rank_tier: Option<u32>,
    /// The act it was reached in, as Riot labels acts.
    pub peak_act: Option<String>,
    /// Kills over deaths across the last few matches, not a career figure.
    pub kd: Option<f64>,
    /// Win rate over the career the backend could see.
    pub win_rate: Option<f64>,
    /// Matches behind that win rate.
    pub games: Option<u32>,
    /// Account level, or zero when they have hidden it.
    pub level: Option<u32>,
    /// Whether the weighed signals came to a flag.
    pub smurf: bool,
    /// The signals themselves, flagged or not, in the backend's words.
    pub smurf_reasons: Vec<String>,
}

/// A lobby, an agent select or a live match, as one frame.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Board {
    /// `MENUS`, `PREGAME`, `INGAME` or a waiting state.
    pub state: Option<String>,
    /// The same thing in words, for the header.
    pub state_label: Option<String>,
    /// Map name, once there is one.
    pub map: Option<String>,
    /// Queue name, once there is one.
    pub mode: Option<String>,
    /// Which side we start on, or are on now.
    pub side: Option<String>,
    /// Everyone the backend could see, both teams once the match starts.
    pub players: Vec<Player>,
    /// The team this account is on, so the board knows which block is ours.
    pub self_team: Option<String>,
}

impl Board {
    /// The players on one side, in the order the backend sent them.
    #[must_use]
    pub fn team<'a>(&'a self, team: &str) -> Vec<&'a Player> {
        self.players
            .iter()
            .filter(|p| p.team.as_deref() == Some(team))
            .collect()
    }

    /// How many accounts on the board came back flagged.
    #[must_use]
    pub fn flagged(&self) -> usize {
        self.players.iter().filter(|p| p.smurf).count()
    }
}

#[cfg(test)]
mod tests {
    use super::Board;

    /// A frame with nothing in it must parse, because the backend sends exactly
    /// that between a match ending and the menus coming back.
    #[test]
    fn empty_frame_parses() {
        let board: Board = serde_json::from_str("{}").expect("an empty object is a valid board");
        assert!(board.players.is_empty());
        assert_eq!(board.flagged(), 0);
    }

    /// Fields this build has never heard of are Riot adding something, not an
    /// error, and a player missing everything but a name still draws.
    #[test]
    fn unknown_fields_and_missing_ones_are_survivable() {
        let json = r#"{
            "state": "INGAME",
            "somethingRiotAddedLater": 7,
            "players": [
                {"name": "A#EU", "team": "Blue", "smurf": true, "kd": 1.5},
                {"name": "B#EU", "team": "Red"}
            ]
        }"#;
        let board: Board = serde_json::from_str(json).expect("a partial board is still a board");
        assert_eq!(board.state.as_deref(), Some("INGAME"));
        assert_eq!(board.team("Blue").len(), 1);
        assert_eq!(board.team("Red").len(), 1);
        assert_eq!(board.flagged(), 1);
        let first = board.players.first().expect("two players were sent");
        assert_eq!(first.kd, Some(1.5));
        let second = board.players.get(1).expect("two players were sent");
        assert_eq!(second.kd, None);
        assert!(!second.smurf);
    }

    /// A field arriving with the wrong type is the one case that cannot be
    /// papered over silently, so it must be an error rather than a default.
    #[test]
    fn a_wrong_type_is_an_error() {
        let json = r#"{"players": [{"kd": "one and a half"}]}"#;
        assert!(serde_json::from_str::<Board>(json).is_err());
    }
}

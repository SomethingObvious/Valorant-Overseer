//! What the bridge broadcasts, typed.
//!
//! Every field is optional, and that is a decision rather than laziness: the
//! socket is a trust boundary, and Riot's own API omits fields freely mid
//! patch. A malformed or partial frame has to read as a missing value, never
//! as a panic, because the thing on screen when that happens is a live match.
//!
//! This mirrors `tui/src/types.ts`, which mirrors `backend/live_match.py`, and
//! the names match all the way down so a field can be traced across the three
//! by grepping for one word. Everything the backend sends is here, whether or
//! not the window draws it yet: a field that arrives and is thrown away is a
//! field somebody has to rediscover later.

use serde::Deserialize;

/// A weapon skin, as the store names it.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Skin {
    /// What the skin is called.
    pub name: Option<String>,
}

/// One weapon and what is equipped on it.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WeaponSkin {
    /// Vandal, Phantom, Operator.
    pub weapon: Option<String>,
    /// What they have on it, if anything.
    pub skin: Option<Skin>,
}

/// A party Riot told us about.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Party {
    /// Riot's own id for the group.
    pub id: Option<String>,
    /// Which of the coloured rails this party gets.
    pub color: Option<String>,
    /// First party, second party, and so on down the board.
    pub number: Option<u32>,
    /// How many of them there are.
    pub size: Option<u32>,
}

/// A party the app worked out rather than one Riot handed over.
///
/// Riot only reveals a party for accounts whose presence is visible, so for
/// strangers this is inference from how often they have shared a side.
/// `shared` and `same` are the evidence it rests on, and they are shown
/// beside the guess because a guess presented as a fact is a lie.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StackGuess {
    /// How many the app thinks are together.
    pub size: Option<u32>,
    /// How sure it is, as a percentage.
    pub confidence: Option<u32>,
    /// How many lobbies the two accounts have shared.
    pub shared: Option<u32>,
    /// How many of those they were on the same side for.
    pub same: Option<u32>,
}

/// A run of wins or losses.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Streak {
    /// `W` or `L`.
    #[serde(rename = "type")]
    pub kind: Option<String>,
    /// How long it has been going.
    pub count: Option<u32>,
}

/// An agent they play, and how much.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TopAgent {
    /// The agent's name.
    pub agent: Option<String>,
    /// Matches on it.
    pub games: Option<u32>,
}

/// How they do on the map being played.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MapWinRate {
    /// Percentage won.
    pub win_rate: Option<f64>,
    /// Out of how many.
    pub games: Option<u32>,
}

/// How often this account has been in a lobby with you before.
///
/// The backend attaches this to every live board from its own local log, so
/// it costs no request to Riot at all.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Encounter {
    /// Times on your team.
    pub with_count: Option<u32>,
    /// Times against you.
    pub against_count: Option<u32>,
    /// Won together.
    pub wins_with: Option<u32>,
    /// Lost together.
    pub losses_with: Option<u32>,
    /// Beaten them.
    pub wins_against: Option<u32>,
    /// Lost to them.
    pub losses_against: Option<u32>,
    /// Drawn together.
    pub draws_with: Option<u32>,
    /// Drawn against.
    pub draws_against: Option<u32>,
}

impl Encounter {
    /// Every lobby this account has been in with you.
    #[must_use]
    pub fn total(&self) -> u32 {
        self.with_count.unwrap_or(0) + self.against_count.unwrap_or(0)
    }
}

/// One player's row, as the backend assembled it.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Player {
    /// Riot's account id. The only stable key; names change and repeat.
    pub puuid: Option<String>,
    /// Display name, already joined with the tag line by the backend.
    pub name: Option<String>,
    /// True when Riot is hiding the name, which is a fact worth showing.
    pub name_hidden: bool,
    /// `Blue` or `Red`, as Riot labels them.
    pub team: Option<String>,
    /// True for the account this machine is signed in as.
    pub is_self: bool,
    /// The title they have equipped.
    pub title: Option<String>,

    /// Agent name, absent until they lock in.
    pub agent: Option<String>,
    /// The agent's role, for the rail glyph.
    pub role: Option<String>,
    /// Their own colour, from Riot's own agent metadata.
    pub agent_color: Option<String>,

    /// Rank name, for example `Gold 2`.
    pub rank: Option<String>,
    /// Rank tier, three per group from Iron at 3. The colour comes off this.
    pub rank_tier: Option<u32>,
    /// Ranked rating within the current tier.
    pub rr: Option<i64>,
    /// What the last match did to that rating.
    pub rr_earned: Option<i64>,
    /// Their place on the leaderboard, for the ranks that have one.
    pub leaderboard: Option<u32>,
    /// Best rank ever reached.
    pub peak_rank: Option<String>,
    /// Tier of that peak.
    pub peak_rank_tier: Option<u32>,
    /// The act it was reached in, as Riot labels acts.
    pub peak_act: Option<String>,
    /// Where they finished the previous act.
    pub previous_rank: Option<String>,

    /// Kills over deaths across the last few matches, not a career figure.
    pub kd: Option<f64>,
    /// Headshot percentage over those same matches.
    pub hs_pct: Option<f64>,
    /// Win rate over the career the backend could see.
    pub win_rate: Option<f64>,
    /// Matches behind that win rate.
    pub games: Option<u32>,
    /// One letter per recent match, newest first.
    pub form: Vec<String>,
    /// The run they are on.
    pub streak: Option<Streak>,
    /// What they play most.
    pub top_agents: Vec<TopAgent>,
    /// How they do on this map.
    pub map_win_rate: Option<MapWinRate>,

    /// Account level, or zero when they have hidden it.
    pub level: Option<u32>,
    /// True when the level is hidden rather than unknown.
    pub level_hidden: bool,
    /// What they have equipped, when the backend could see it.
    pub weapons: Vec<WeaponSkin>,

    /// The party Riot says they are in.
    pub party: Option<Party>,
    /// The party the app thinks they are in.
    pub stack_guess: Option<StackGuess>,
    /// How often you have played with or against them.
    pub encounter: Option<Encounter>,

    /// Whether the weighed signals came to a flag.
    pub smurf: bool,
    /// The signals themselves, flagged or not, in the backend's words.
    pub smurf_reasons: Vec<String>,
}

impl Player {
    /// The name to draw, which is never empty.
    #[must_use]
    pub fn display_name(&self) -> &str {
        self.name.as_deref().unwrap_or("-")
    }

    /// How many times you have met them, from whichever record exists.
    #[must_use]
    pub fn met(&self) -> u32 {
        self.encounter.as_ref().map_or(0, Encounter::total)
    }
}

/// A team's numbers, as the backend averaged them.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TeamStats {
    /// The average rank, in words.
    pub avg_rank: Option<String>,
    /// That average as a tier, for the colour.
    pub avg_rank_tier: Option<u32>,
    /// The average K/D across the side.
    pub avg_kd: Option<f64>,
    /// The average win rate across the side.
    pub avg_win_rate: Option<f64>,
    /// How many of them came back flagged.
    pub smurf_count: Option<u32>,
}

/// One ranked match in today's session.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SessionPoint {
    /// Which map it was on.
    pub map: Option<String>,
    /// Victory, Defeat or Draw, in Riot's words.
    pub result: Option<String>,
    /// What it did to the rating.
    pub delta: Option<i64>,
    /// The rating after it.
    pub rr: Option<i64>,
}

/// Today, in ranked rating.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Session {
    /// The sum of the deltas below.
    pub net: Option<i64>,
    /// One entry per match, oldest first.
    pub points: Vec<SessionPoint>,
}

/// The score, once there is one.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Score {
    /// Rounds your side has won.
    pub ally: Option<u32>,
    /// Rounds the other side has won.
    pub enemy: Option<u32>,
    /// Which round is being played.
    pub round: Option<u32>,
}

/// How far through agent select the lobby is.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LockProgress {
    /// How many have locked in.
    pub locked: Option<u32>,
    /// Out of how many.
    pub total: Option<u32>,
}

/// Something the backend wants said out loud.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Notice {
    /// How loudly to say it.
    pub level: Option<String>,
    /// What to do about it.
    pub action: Option<String>,
    /// The thing itself.
    pub message: Option<String>,
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
    /// Riot's id for the match, which changes when a new one starts.
    pub match_id: Option<String>,
    /// The team this account is on, so the board knows which block is ours.
    pub self_team: Option<String>,
    /// Everyone the backend could see, both teams once the match starts.
    pub players: Vec<Player>,
    /// Each side's averages, keyed by `Blue` and `Red`.
    pub team_stats: std::collections::HashMap<String, TeamStats>,
    /// The score, once the match has started.
    pub score: Option<Score>,
    /// How far through agent select the lobby is.
    pub lock_progress: Option<LockProgress>,
    /// The backend's estimate, as a percentage for your side.
    pub win_prob: Option<f64>,
    /// Today, in ranked rating.
    pub session: Option<Session>,
    /// Something the backend wants said out loud.
    pub notice: Option<Notice>,
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

    /// The numbers for one side, if the backend sent them.
    #[must_use]
    pub fn stats(&self, team: &str) -> Option<&TeamStats> {
        self.team_stats.get(team)
    }
}

#[cfg(test)]
mod tests {
    use super::Board;

    /// A frame with nothing in it must parse, because the backend sends
    /// exactly that between a match ending and the menus coming back.
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
        assert_eq!(second.met(), 0);
    }

    /// The whole payload, as the backend really sends it, so that adding a
    /// field to the Python side and forgetting it here is visible.
    #[test]
    fn the_rich_fields_all_arrive() {
        let json = r#"{
            "state": "INGAME",
            "matchId": "m-1",
            "selfTeam": "Blue",
            "winProb": 56.0,
            "score": {"ally": 3, "enemy": 1, "round": 5},
            "teamStats": {"Blue": {"avgRank": "Gold 1", "avgKd": 1.5, "smurfCount": 1}},
            "session": {"net": -1, "points": [{"map": "Icebox", "result": "Victory", "delta": 22}]},
            "players": [{
                "name": "A#EU", "team": "Blue", "isSelf": true, "title": "Legend",
                "agent": "KAY/O", "role": "Initiator", "rank": "Gold 2", "rankTier": 13,
                "rr": 37, "rrEarned": -12, "leaderboard": 412, "peakRank": "Gold 3",
                "peakRankTier": 14, "peakAct": "V25 Act 4", "previousRank": "Gold 1",
                "kd": 0.91, "hsPct": 33.0, "winRate": 52.0, "games": 118,
                "form": ["W", "L", "W"], "streak": {"type": "W", "count": 2},
                "topAgents": [{"agent": "KAY/O", "games": 42}],
                "mapWinRate": {"winRate": 67.0, "games": 6},
                "level": 154, "levelHidden": false,
                "weapons": [{"weapon": "Vandal", "skin": {"name": "Reaver"}}],
                "party": {"id": "p1", "number": 1, "size": 2},
                "stackGuess": {"size": 3, "confidence": 88, "shared": 9, "same": 7},
                "encounter": {"withCount": 3, "againstCount": 1, "winsWith": 2}
            }]
        }"#;
        let board: Board = serde_json::from_str(json).expect("the real payload parses");
        let player = board.players.first().expect("one player");
        assert_eq!(player.title.as_deref(), Some("Legend"));
        assert_eq!(player.rr_earned, Some(-12));
        assert_eq!(player.leaderboard, Some(412));
        assert_eq!(player.previous_rank.as_deref(), Some("Gold 1"));
        assert_eq!(player.hs_pct, Some(33.0));
        assert_eq!(player.form.len(), 3);
        assert_eq!(player.streak.as_ref().and_then(|s| s.count), Some(2));
        assert_eq!(player.top_agents.first().and_then(|a| a.games), Some(42));
        assert_eq!(player.map_win_rate.as_ref().and_then(|m| m.games), Some(6));
        assert_eq!(player.weapons.len(), 1);
        assert_eq!(
            player.stack_guess.as_ref().and_then(|s| s.confidence),
            Some(88)
        );
        assert_eq!(player.met(), 4);
        assert_eq!(board.stats("Blue").and_then(|s| s.smurf_count), Some(1));
        assert_eq!(board.score.as_ref().and_then(|s| s.round), Some(5));
        assert_eq!(board.session.as_ref().map(|s| s.points.len()), Some(1));
        assert_eq!(board.win_prob, Some(56.0));
    }

    /// A field arriving with the wrong type is the one case that cannot be
    /// papered over silently, so it must be an error rather than a default.
    #[test]
    fn a_wrong_type_is_an_error() {
        let json = r#"{"players": [{"kd": "one and a half"}]}"#;
        assert!(serde_json::from_str::<Board>(json).is_err());
    }
}

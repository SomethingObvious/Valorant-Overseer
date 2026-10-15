//! Your last few matches with their end-of-game scoreboards, typed: what
//! `handle_data_request("history")` in `backend/app.py` sends for the History
//! screen, every field optional as in [`crate::board`].

use serde::Deserialize;

use crate::board::number;

/// The matches that came back, newest first.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Recent {
    /// One per match.
    pub games: Vec<Game>,
    /// How many were asked for, which can be more than exist.
    #[serde(deserialize_with = "number::whole")]
    pub asked: Option<u32>,
    /// Set when Riot held some back, so fewer came than exist.
    pub partial: bool,
}

/// One match, as you saw it.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Game {
    /// Riot's own id for it.
    pub match_id: Option<String>,
    /// Where it was played.
    pub map: Option<String>,
    /// Competitive, Swiftplay, Spike Rush.
    pub mode: Option<String>,
    /// When it started, in milliseconds since the epoch.
    #[serde(deserialize_with = "number::signed")]
    pub started_at: Option<i64>,
    /// How long it ran, in milliseconds.
    #[serde(deserialize_with = "number::signed")]
    pub length_ms: Option<i64>,
    /// Your rounds, then theirs. Absent in a deathmatch.
    pub score: Option<[u32; 2]>,
    /// Victory, Defeat or Draw, from your side.
    pub result: Option<String>,
    /// What it did to your rating. Absent outside competitive.
    #[serde(deserialize_with = "number::signed")]
    pub rr_delta: Option<i64>,
    /// Your side's id, which the players carry too.
    pub your_team: Option<String>,
    /// Everybody, best combat score first.
    pub players: Vec<Line>,
}

/// One player's line on a scoreboard.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Line {
    /// Their account id.
    pub puuid: Option<String>,
    /// Their name and tag.
    pub name: Option<String>,
    /// Which side they were on.
    pub team: Option<String>,
    /// Who they played.
    pub agent: Option<String>,
    /// Kills.
    #[serde(deserialize_with = "number::whole")]
    pub kills: Option<u32>,
    /// Deaths.
    #[serde(deserialize_with = "number::whole")]
    pub deaths: Option<u32>,
    /// Assists.
    #[serde(deserialize_with = "number::whole")]
    pub assists: Option<u32>,
    /// Average combat score.
    #[serde(deserialize_with = "number::whole")]
    pub acs: Option<u32>,
    /// Average damage a round.
    #[serde(deserialize_with = "number::whole")]
    pub adr: Option<u32>,
    /// Headshots as a share of hits, as a percentage.
    #[serde(deserialize_with = "number::whole")]
    pub hs_pct: Option<u32>,
    /// Rounds with a kill, an assist, a survival or a trade, as a percentage.
    #[serde(deserialize_with = "number::whole")]
    pub kast: Option<u32>,
    /// Rounds they got the first kill of.
    #[serde(deserialize_with = "number::whole")]
    pub first_bloods: Option<u32>,
    /// Whether this is you.
    pub is_subject: bool,
}

#[cfg(test)]
mod tests {
    use super::Recent;

    /// What the backend sends, a draw's missing RR and a deathmatch's missing
    /// score included, reads without losing a field.
    #[test]
    fn a_history_reads_as_sent() {
        let games: Recent = serde_json::from_str(
            r#"{"games":[
                {"matchId":"m1","map":"Haven","mode":"Competitive","startedAt":1790000000000,
                 "lengthMs":2100000.0,"score":[13,9],"result":"Victory","rrDelta":20,
                 "yourTeam":"Blue","players":[{"puuid":"me","name":"Me#1","team":"Blue",
                 "agent":"Brimstone","kills":21,"deaths":15,"assists":15,"acs":242.4,
                 "adr":133,"hsPct":null,"kast":92,"firstBloods":3,"isSubject":true}]},
                {"matchId":"m2","mode":"Deathmatch","players":[]}
            ],"asked":10,"partial":true}"#,
        )
        .unwrap();
        assert_eq!(games.asked, Some(10));
        assert!(games.partial);
        let first = games.games.first().unwrap();
        assert_eq!(first.score, Some([13, 9]));
        assert_eq!(first.rr_delta, Some(20));
        assert_eq!(first.length_ms, Some(2_100_000));
        let me = first.players.first().unwrap();
        assert_eq!((me.kills, me.acs, me.hs_pct), (Some(21), Some(242), None));
        assert!(me.is_subject);
        assert_eq!(games.games.get(1).unwrap().score, None);
    }
}

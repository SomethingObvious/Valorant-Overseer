//! The Lineups screen's data, typed, both ways.
//!
//! This is what `handle_data_request("lineups")` in `backend/app.py` sends,
//! and the lineup the window sends back to be saved. Points are on the
//! minimap, from 0 at its top left to 1 at its bottom right.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::board::number;

/// Everything the screen draws from.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Atlas {
    /// Every map with bomb sites, by name.
    pub maps: Vec<Plan>,
    /// Every agent and the abilities a lineup can be for.
    pub agents: Vec<Kit>,
    /// Every saved lineup, on every map.
    pub lineups: Vec<Lineup>,
    /// The shapes drawn on each map, by map name.
    pub drawings: HashMap<String, Vec<Drawing>>,
    /// Which of the programs a clip needs are installed.
    pub tools: Tools,
    /// The agent you play most, from your recorded matches, for when no
    /// lobby is on the board to say.
    pub main: Option<String>,
}

/// One map as the screen draws it.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Plan {
    /// Ascent, Bind, Haven.
    pub name: String,
    /// The minimap on disk, once the backend has it.
    pub minimap: Option<String>,
    /// The site letters, A and B, or A, B and C.
    pub sites: Vec<String>,
    /// The places the game names, like A Main.
    pub callouts: Vec<Callout>,
    /// How much of the minimap's width one game unit is, a hundredth of a
    /// metre.
    pub scale: Option<f32>,
    /// Where the attackers start, on the minimap.
    pub attack: Option<[f32; 2]>,
    /// Where the defenders start.
    pub defend: Option<[f32; 2]>,
}

/// A place on a map with the name players call it.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Callout {
    /// A Main, Mid Courtyard.
    pub name: String,
    /// Where it is on the minimap.
    pub at: [f32; 2],
}

/// An agent and their abilities.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Kit {
    /// Brimstone, Viper.
    pub name: String,
    /// In key order: C, Q, E and X.
    pub abilities: Vec<Ability>,
}

/// One of an agent's abilities.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Ability {
    /// The key it is on by default: C, Q, E or X.
    pub key: String,
    /// Incendiary, Snake Bite.
    pub name: Option<String>,
    /// Its icon on disk, white on clear.
    pub icon: Option<String>,
}

/// One lineup: where to stand, where it lands, and the clip of the throw.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Lineup {
    /// Its file name, set by the backend when it is first saved.
    pub id: Option<String>,
    /// The map it is on.
    pub map: String,
    /// Who throws it.
    pub agent: String,
    /// Which ability, by name.
    pub ability: Option<String>,
    /// `attack` or `defense`.
    pub side: Option<String>,
    /// The site letter it is for.
    pub site: Option<String>,
    /// What it is called, like A Main to Default.
    pub title: String,
    /// How to throw it, in the author's words.
    pub notes: Option<String>,
    /// Everything else about it, as long as it needs.
    pub description: Option<String>,
    /// Pictures of it, as files on this PC.
    pub images: Vec<String>,
    /// Where to stand.
    pub stand: Option<[f32; 2]>,
    /// Where it lands.
    pub land: Option<[f32; 2]>,
    /// The clip of the throw.
    pub clip: Option<Clip>,
}

/// A clip, and where it was cut from.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Clip {
    /// The link or the file it was cut from.
    pub source: Option<String>,
    /// Where it starts in the source, in seconds.
    pub from: Option<f64>,
    /// Where it ends in the source, in seconds.
    pub to: Option<f64>,
    /// Its volume as a percentage of the source's.
    #[serde(deserialize_with = "number::whole")]
    pub volume: Option<u32>,
    /// The cut clip on disk.
    pub file: Option<String>,
}

/// A shape drawn on a map, to show a range or an area.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Drawing {
    /// `rect`, `circle`, `oval`, `cone`, `triangle`, `text`, `textbox` or
    /// `brush`.
    pub kind: String,
    /// Its colour, like `#FF4655`.
    pub colour: String,
    /// Where the drag that drew it started: a corner, a centre or a
    /// cone's tip.
    pub a: [f32; 2],
    /// Where it ended: the other corner, a point on the edge, or the
    /// middle of a cone's far edge.
    pub b: [f32; 2],
    /// How wide a cone opens, in degrees.
    pub spread: f32,
    /// How solid it is, from 0.1 to 1.
    pub opacity: f32,
    /// What a `text` label or a `textbox` says.
    pub text: String,
    /// A brush stroke's points, on the minimap, in the order drawn.
    pub points: Vec<[f32; 2]>,
    /// How thick a brush stroke is, in widths of the minimap.
    pub width: f32,
}

// By hand rather than derived, because a shape kept before it had an
// opacity is a solid one, not an invisible one.
impl Default for Drawing {
    fn default() -> Self {
        Self {
            kind: String::new(),
            colour: String::new(),
            a: [0.0; 2],
            b: [0.0; 2],
            spread: 0.0,
            opacity: 1.0,
            text: String::new(),
            points: Vec::new(),
            width: 0.0,
        }
    }
}

/// Which of the programs a clip needs are installed.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Tools {
    /// yt-dlp, for a clip from a link.
    pub ytdlp: bool,
    /// ffmpeg, for cutting a clip and setting its volume.
    pub ffmpeg: bool,
    /// ffplay, for watching one.
    pub ffplay: bool,
}

#[cfg(test)]
mod tests {
    use super::{Atlas, Lineup};

    /// What the backend sends reads whole, and a lineup goes back in the
    /// shape the backend reads.
    #[test]
    fn an_atlas_reads_and_a_lineup_goes_back() {
        let atlas: Atlas = serde_json::from_str(
            r##"{"maps":[{"name":"Ascent","minimap":"C:/m/ascent.png","sites":["A","B"],
                 "callouts":[{"name":"A Main","at":[0.62,0.41]}]}],
                "agents":[{"name":"Brimstone","abilities":[{"slot":"Ability1","key":"Q",
                 "name":"Incendiary","icon":null}]}],
                "lineups":[{"id":"b-1","map":"Ascent","agent":"Brimstone","title":"A Main",
                 "stand":[0.6,0.5],"land":[0.3,0.1],
                 "clip":{"source":"https://example.com","from":70.0,"to":77.0,"volume":60.0}}],
                "drawings":{"Ascent":[{"kind":"cone","colour":"#18E5A7","a":[0.2,0.2],
                 "b":[0.3,0.3],"spread":60.0}]},
                "tools":{"ytdlp":true,"ffmpeg":true,"ffplay":false}}"##,
        )
        .unwrap();
        assert_eq!(atlas.maps.first().unwrap().sites, ["A", "B"]);
        assert!(atlas.tools.ytdlp && !atlas.tools.ffplay);
        let cone = atlas.drawings.get("Ascent").unwrap().first().unwrap();
        assert_eq!((cone.kind.as_str(), cone.spread), ("cone", 60.0));
        assert!(
            (cone.opacity - 1.0).abs() < f32::EPSILON,
            "a shape kept without an opacity is solid"
        );
        let lineup = atlas.lineups.first().unwrap();
        assert_eq!(lineup.clip.as_ref().unwrap().volume, Some(60));
        let sent = serde_json::to_value(lineup).unwrap();
        assert_eq!(
            sent.get("stand").unwrap(),
            &serde_json::json!([0.6_f32, 0.5_f32])
        );
        assert_eq!(sent.pointer("/clip/from").unwrap(), 70.0);
        let back: Lineup = serde_json::from_value(sent).unwrap();
        assert_eq!(&back, lineup);
    }
}

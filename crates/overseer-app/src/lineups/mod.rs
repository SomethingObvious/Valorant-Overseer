//! The Lineups screen: every map with bomb sites, its plantable ground lit,
//! your own lineups on it, and shapes drawn over it to show ranges, with a
//! click on the map to add a lineup and a button to save the map as a picture.
//!
//! Lineups stay in this window and never reach the overlay, since Riot's
//! rules rule out telling somebody where to go in the middle of a round.

mod form;
mod pick;
mod pictures;
mod plan;
mod player;
mod shapes;
mod side;
mod sketch;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use egui::{CentralPanel, Color32, Panel, Rect, Ui, pos2};
use overseer_core::{Ability, Atlas, Bridge, Clip, Drawing, Kit, Lineup};
use overseer_ui::{art, colour};

use crate::settings::MapTurn;
use crate::view;

use pick::ANY;
pub(crate) use player::{Prefs, SIZES, SPEEDS, speed_name};
pub(crate) use shapes::COLOURS;
pub(crate) use sketch::swatch;

/// How wide the panel down the right is.
const SIDE: f32 = 380.0;

/// Everything the screen draws from, or where the request for it has got to.
#[derive(Debug, Default)]
enum Load {
    /// Nothing asked yet.
    #[default]
    Idle,
    /// Asked, with the last answer kept on screen until the new one lands.
    Asking {
        /// The bridge's id for the question.
        id: u64,
        /// What was showing when it was asked.
        kept: Option<Box<Atlas>>,
    },
    /// The answer.
    Have(Box<Atlas>),
    /// Why there is nothing, in words for a player.
    Refused(String),
}

/// A request the screen sent that changes something, waiting on its answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Job {
    /// Saving the lineup being written, clip and all.
    Save,
    /// Throwing a lineup away.
    Delete,
    /// Keeping a map's shapes.
    Drawing,
}

/// What order the lineups are listed in, after what they share.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Order {
    /// A, then B, then C, then any without a site.
    #[default]
    Site,
    /// By name.
    Name,
    /// By who throws it.
    Agent,
    /// Attacking, then defending.
    Side,
}

impl Order {
    /// What the sort chip says.
    const fn label(self) -> &'static str {
        match self {
            Self::Site => "By Site",
            Self::Name => "By Name",
            Self::Agent => "By Agent",
            Self::Side => "By Side",
        }
    }

    /// The one after it, as the chip goes round them.
    const fn next(self) -> Self {
        match self {
            Self::Site => Self::Name,
            Self::Name => Self::Agent,
            Self::Agent => Self::Side,
            Self::Side => Self::Site,
        }
    }

    /// How two lineups compare in this order, leaving a tie for the name.
    fn compare(self, a: &Lineup, b: &Lineup) -> std::cmp::Ordering {
        // Something set comes before nothing set.
        let key = |v: Option<&String>| (v.is_none(), v.cloned().unwrap_or_default());
        match self {
            Self::Site => key(a.site.as_ref()).cmp(&key(b.site.as_ref())),
            Self::Name => std::cmp::Ordering::Equal,
            Self::Agent => a.agent.cmp(&b.agent),
            Self::Side => key(a.side.as_ref()).cmp(&key(b.side.as_ref())),
        }
    }
}

/// Whether `lineup` has every word of `wanted` somewhere: its name, agent,
/// ability, site, notes or description.
fn matches(lineup: &Lineup, wanted: &str) -> bool {
    let all = [
        Some(&lineup.title),
        Some(&lineup.agent),
        lineup.ability.as_ref(),
        lineup.site.as_ref(),
        lineup.notes.as_ref(),
        lineup.description.as_ref(),
    ]
    .into_iter()
    .flatten()
    .map(|s| s.to_lowercase())
    .collect::<Vec<_>>()
    .join(" ");
    wanted
        .split_whitespace()
        .all(|word| all.contains(&word.to_lowercase()))
}

/// What the side panel is doing.
#[derive(Debug, Default)]
enum Mode {
    /// Reading the lineups on a map.
    #[default]
    Browse,
    /// Writing one.
    Edit(Box<Draft>),
    /// Drawing shapes on the map.
    Draw(Box<Sketch>),
}

/// What the next click on the map sets while a lineup is being written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place {
    /// Where to stand.
    Stand,
    /// Where it lands.
    Land,
}

/// A lineup being written, with its clip's fields as they were typed.
#[derive(Debug, Clone)]
struct Draft {
    /// Everything but the clip and the notes.
    lineup: Lineup,
    /// What the next click on the map sets.
    placing: Place,
    /// The pin following the pointer while a drag on the map goes on.
    grab: Option<Place>,
    /// A link, or the path of a video on this PC.
    source: String,
    /// Where the clip starts, as typed, like 1:10.
    from: String,
    /// Where it ends.
    to: String,
    /// Its volume as a percentage of the source's.
    volume: f32,
    /// How to throw it.
    notes: String,
    /// Everything else about it, as long as it needs.
    description: String,
    /// The path of a picture being typed in, to add.
    picture: String,
    /// What the agent search has in it.
    find: String,
    /// What the ability search has in it, for any agent.
    find_ability: String,
    /// Whether every agent's face is open to pick from.
    choosing: bool,
    /// Whether the clip's fields are open.
    clip_open: bool,
    /// Whether the notes box is open.
    notes_open: bool,
    /// How long the clip's source runs, for the trim bar.
    measure: Measure,
}

/// What is known about how long a clip's source runs.
#[derive(Debug, Clone, Default)]
struct Measure {
    /// Its length in seconds, once the backend has said.
    length: Option<f64>,
    /// The source that was asked about, so a new one is asked about again.
    of: String,
    /// Whether the backend is still finding out.
    asking: bool,
    /// Why it couldn't, when it couldn't.
    failed: Option<String>,
    /// Set by Load Video, so a link is only fetched once somebody asks.
    wanted: bool,
    /// The span the trim bar showed when a handle was taken, kept until
    /// it is let go.
    held: Option<(f64, f64)>,
    /// Where the handle being dragged was last, for the exact frame there
    /// once it is let go.
    last: Option<f64>,
    /// A file on this PC the window can play it from, once there is one.
    file: Option<String>,
    /// Whether the backend is still getting that file, which for a link
    /// is a download.
    fetching: bool,
}

impl Draft {
    /// A fresh lineup on `map`, for `agent` when one is picked already.
    fn new(map: &str, agent: Option<&str>) -> Self {
        Self {
            lineup: Lineup {
                map: map.to_owned(),
                agent: agent.unwrap_or_default().to_owned(),
                side: Some("attack".to_owned()),
                ..Lineup::default()
            },
            placing: Place::Stand,
            grab: None,
            source: String::new(),
            from: String::new(),
            to: String::new(),
            volume: 100.0,
            notes: String::new(),
            description: String::new(),
            picture: String::new(),
            find: String::new(),
            find_ability: String::new(),
            choosing: agent.is_none(),
            clip_open: false,
            notes_open: false,
            measure: Measure::default(),
        }
    }

    /// A saved lineup, opened to change it.
    fn of(lineup: &Lineup) -> Self {
        let clip = lineup.clip.clone().unwrap_or_default();
        Self {
            lineup: Lineup {
                clip: None,
                notes: None,
                ..lineup.clone()
            },
            placing: if lineup.stand.is_some() {
                Place::Land
            } else {
                Place::Stand
            },
            grab: None,
            source: clip.source.unwrap_or_default(),
            from: clip.from.map(clock).unwrap_or_default(),
            to: clip.to.map(clock).unwrap_or_default(),
            volume: clip.volume.map_or(100.0, |v| v as f32),
            notes: lineup.notes.clone().unwrap_or_default(),
            description: lineup.description.clone().unwrap_or_default(),
            picture: String::new(),
            find: String::new(),
            find_ability: String::new(),
            choosing: false,
            clip_open: lineup.clip.is_some(),
            notes_open: lineup.notes.is_some(),
            // Its video was loaded once already, so it loads again unasked.
            measure: Measure {
                wanted: true,
                ..Measure::default()
            },
        }
    }

    /// The lineup as the backend saves it, named for its ability and site
    /// when nobody gave it a name.
    fn sent(&self) -> Lineup {
        let notes = self.notes.trim();
        let description = self.description.trim();
        let title = self.lineup.title.trim();
        Lineup {
            notes: (!notes.is_empty()).then(|| notes.to_owned()),
            description: (!description.is_empty()).then(|| description.to_owned()),
            title: if title.is_empty() {
                self.named()
            } else {
                title.to_owned()
            },
            ..self.lineup.clone()
        }
    }

    /// The name it gets when it isn't given one, like A Site Incendiary.
    fn named(&self) -> String {
        let what = self
            .lineup
            .ability
            .clone()
            .unwrap_or_else(|| format!("{} Lineup", self.lineup.agent));
        match self.lineup.site.as_deref() {
            Some(site) => format!("{site} Site {what}"),
            None => what,
        }
    }

    /// What still stops it being saved, in words, or nothing once it can be.
    const fn missing(&self) -> Option<&'static str> {
        match (
            self.lineup.agent.is_empty(),
            self.lineup.stand.is_some(),
            self.lineup.land.is_some(),
        ) {
            (_, false, _) => Some("Click the map where you stand to throw it."),
            (_, true, false) => Some("Click the map where it lands."),
            (true, true, true) => Some("Pick the agent who throws it, or Any Agent."),
            (false, true, true) => None,
        }
    }

    /// The clip as the backend cuts it, or nothing when no source is given.
    fn clip(&self) -> serde_json::Value {
        if self.source.trim().is_empty() {
            return serde_json::Value::Null;
        }
        serde_json::json!({
            "source": self.source.trim(),
            "from": self.from.trim(),
            "to": self.to.trim(),
            "volume": self.volume.round(),
        })
    }
}

/// Shapes being drawn on a map, and what the next drag draws.
#[derive(Debug, Clone)]
struct Sketch {
    /// Every shape on the map, the newest last.
    shapes: Vec<Drawing>,
    /// Which of [`shapes::KINDS`] the next drag draws.
    kind: usize,
    /// Which of [`COLOURS`] it is in.
    colour: usize,
    /// How wide a cone opens, in degrees.
    spread: f32,
    /// How solid the next shape is, from 0.1 to 1.
    opacity: f32,
    /// How thick the next brush stroke is, in widths of the minimap.
    width: f32,
    /// The points of the brush stroke being drawn, on the minimap.
    stroke: Vec<[f32; 2]>,
    /// The shape drawn or moved last, which the colour and the opacity
    /// change too.
    picked: Option<usize>,
    /// Where the drag drawing a shape started, on the minimap.
    dragging: Option<[f32; 2]>,
    /// The words the next label says.
    words: String,
    /// The shape being dragged by its dot: which one, where the drag
    /// started, and the shape as it was then.
    moving: Option<(usize, [f32; 2], Drawing)>,
    /// The picked shape being resized by a grip: which shape, which grip,
    /// and the shape as it was when the drag started.
    resizing: Option<(usize, usize, Drawing)>,
    /// Set by the first press of Clear All, which the second one confirms.
    confirm: bool,
}

impl Sketch {
    /// The shape the next drag draws, from `start` to `end`.
    fn drawing(&self, start: [f32; 2], end: [f32; 2]) -> Drawing {
        Drawing {
            kind: self.tool().to_owned(),
            colour: COLOURS
                .get(self.colour)
                .copied()
                .unwrap_or("#FF4655")
                .to_owned(),
            a: start,
            b: end,
            spread: self.spread,
            opacity: self.opacity,
            text: match self.tool() {
                "text" => self.words.trim().to_owned(),
                "textbox" => self.words.trim_end().to_owned(),
                _ => String::new(),
            },
            points: if self.tool() == "brush" {
                self.stroke.clone()
            } else {
                Vec::new()
            },
            width: self.width,
        }
    }

    /// The kind the next drag or click draws, as kept.
    fn tool(&self) -> &'static str {
        shapes::KINDS
            .get(self.kind)
            .map_or("circle", |(kind, _)| kind)
    }

    /// Whether the next click places a label rather than a drag drawing a
    /// shape.
    fn lettering(&self) -> bool {
        self.tool() == "text"
    }

    /// Picks the shape at `index`, so the panel shows its colour, its
    /// opacity and a label's words, and changing them changes it.
    fn pick(&mut self, index: usize) {
        self.picked = Some(index);
        let Some(shape) = self.shapes.get(index) else {
            return;
        };
        if let Some(at) = COLOURS
            .iter()
            .position(|c| c.eq_ignore_ascii_case(&shape.colour))
        {
            self.colour = at;
        }
        self.opacity = shape.opacity;
        if matches!(shape.kind.as_str(), "text" | "textbox") {
            self.words.clone_from(&shape.text);
        }
        if shape.kind == "brush" {
            self.width = shape.width;
        }
    }
}

/// Something the side panel asked the backend to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Request {
    /// Save the lineup being written.
    Save,
    /// Throw away the selected lineup.
    Delete,
    /// Find out how long the clip's source runs.
    Measure,
    /// Get a file of the clip's source the window can play.
    Watch,
}

/// Where an exported map goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Export {
    /// A PNG in `lineups\exports`.
    Save,
    /// The clipboard, for pasting straight into a chat.
    Copy,
}

/// The screen's state apart from the data: which map, whose lineups, and
/// what the side panel is doing.
#[derive(Debug, Default)]
struct View {
    /// The map on screen.
    map: Option<String>,
    /// Only this agent's lineups, when set.
    agent: Option<String>,
    /// Words a lineup has to have somewhere, to be listed.
    looking_for: String,
    /// What order the list is in.
    order: Order,
    /// The lineup picked, by id.
    selected: Option<String>,
    /// Lineups stacked on one spot that a click landed on, by id, for the
    /// player to pick one of: where the click was, the ids, and the frame the
    /// list opened on, which a click that opened it can't also close it on.
    choosing: Option<(egui::Pos2, Vec<String>, u64)>,
    /// What the side panel is doing.
    mode: Mode,
    /// A request in flight, and what it was.
    pending: Option<(u64, Job)>,
    /// The lineup being saved, kept while the backend cuts its clip so a
    /// save that fails can open it again as it was.
    saving: Option<Box<Draft>>,
    /// A lineup saved while the list was being asked for again, to add to
    /// that list, since the backend may have read it before the save landed.
    late: Option<Lineup>,
    /// How long a clip runs, asked for apart from `pending` so Save doesn't
    /// wait on it.
    measuring: Option<u64>,
    /// A file of the clip's source to play, asked for apart in the same way.
    watching: Option<u64>,
    /// The video in the panel, while one is shown.
    player: Option<player::Player>,
    /// How a clip plays when it first shows, from the settings.
    prefs: Prefs,
    /// The pictures showing big, and which of them is on screen.
    viewing: Option<(Vec<String>, usize)>,
    /// Which way up the maps are drawn, from the settings.
    turn_to: MapTurn,
    /// Whether every lineup's name shows, from the settings.
    names_by_default: bool,
    /// The colour areas are drawn in, from the settings, or by side.
    area: Option<Color32>,
    /// Show Names or Hide Names pressed since Lineups opened, which wins over
    /// the setting until it closes.
    names: Option<bool>,
    /// Quarter turns clockwise asked for with Turn since Lineups opened, on
    /// top of the way the settings turn each map.
    turned: u8,
    /// What the last request came to, and whether it failed.
    said: Option<(String, bool)>,
    /// Set by the first press of Delete, which the second one confirms.
    confirm: bool,
    /// The agent you play, who a new lineup starts with.
    you: Option<String>,
    /// The agent you play most, whose outline the blacked-out figure is.
    main: Option<String>,
    /// Shapes changed on a map and not yet sent to be kept.
    unsaved: Option<(String, Vec<Drawing>)>,
    /// An export asked for and not yet done.
    export: Option<Export>,
    /// Where the map was on screen when the window was asked for a picture.
    shot: Option<Rect>,
    /// The folder the last picture was saved in.
    saved_to: Option<PathBuf>,
}

/// How the settings say the maps are drawn.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Look<'a> {
    /// Which way up.
    pub(crate) turn: MapTurn,
    /// Whether every lineup's name shows when Lineups opens.
    pub(crate) names: bool,
    /// The colour of an ability's area, or by side when there is none.
    pub(crate) area: Option<&'a str>,
}

/// The Lineups screen.
#[derive(Debug, Default)]
pub(crate) struct Lineups {
    /// Everything the screen draws from.
    load: Load,
    /// Which map, whose lineups, and what the side panel is doing.
    view: View,
}

impl Lineups {
    /// Asks for everything the screen draws from, on the current match's map
    /// when there is one and nothing is being written. `you` is the agent a
    /// new lineup starts with.
    pub(crate) fn open(
        &mut self,
        bridge: &Bridge,
        live: bool,
        map: Option<&str>,
        (you, main): (Option<&str>, Option<&str>),
    ) {
        self.view.you = you.map(ToOwned::to_owned);
        // Only a board that knows replaces it. Out of a lobby the lineups'
        // own answer says, from your recorded matches.
        if let Some(main) = main {
            self.view.main = Some(main.to_owned());
        }
        if let Some(map) = map
            && matches!(self.view.mode, Mode::Browse)
        {
            self.view.map = Some(map.to_owned());
        }
        if !live || matches!(self.load, Load::Asking { .. }) {
            return;
        }
        // Asked on every open, which is cheap after the first: the minimaps
        // are on disk and a lineup is a small file.
        self.ask(bridge);
    }

    /// Asks for everything the screen draws from, keeping what is showing on
    /// screen until the answer lands.
    fn ask(&mut self, bridge: &Bridge) {
        let kept = match std::mem::take(&mut self.load) {
            Load::Have(atlas) => Some(atlas),
            Load::Asking { kept, .. } => kept,
            Load::Idle | Load::Refused(_) => None,
        };
        let id = bridge.ask("lineups", serde_json::json!({}));
        self.load = Load::Asking { id, kept };
    }

    /// Moves to `map`, the one being played, while lineups are only being
    /// read and the map is one there are lineups for.
    pub(crate) fn follow(&mut self, map: &str) {
        let known = match &self.load {
            Load::Have(atlas)
            | Load::Asking {
                kept: Some(atlas), ..
            } => atlas.maps.iter().any(|m| m.name == map),
            _ => false,
        };
        if known && matches!(self.view.mode, Mode::Browse) && self.view.map.as_deref() != Some(map)
        {
            self.view.map = Some(map.to_owned());
            self.view.selected = None;
        }
    }

    /// Whether a clip or a picture is showing full screen.
    pub(crate) fn fullscreen(&self) -> bool {
        self.view.viewing.is_some() || self.view.player.as_ref().is_some_and(player::Player::big)
    }

    /// Whether the answer with this id is one this screen is waiting on.
    pub(crate) fn waiting_on(&self, id: u64) -> bool {
        matches!(self.load, Load::Asking { id: waiting, .. } if waiting == id)
            || self.view.pending.is_some_and(|(waiting, _)| waiting == id)
            || self.view.measuring == Some(id)
            || self.view.watching == Some(id)
    }

    /// Takes an answer this screen was waiting on.
    pub(crate) fn answered(&mut self, id: u64, result: Result<serde_json::Value, String>) {
        if let Load::Asking { id: waiting, .. } = self.load
            && waiting == id
        {
            let read = result.and_then(|value| {
                serde_json::from_value::<Atlas>(value).map_err(|_| {
                    "The lineups came back in a shape this build can't read.".to_owned()
                })
            });
            self.load = match read {
                Ok(mut atlas) => {
                    if let Some(saved) = self.view.late.take() {
                        atlas.lineups.retain(|l| l.id != saved.id);
                        atlas.lineups.push(saved);
                    }
                    self.view.settle(&atlas);
                    Load::Have(Box::new(atlas))
                }
                Err(why) => Load::Refused(why),
            };
            return;
        }
        if self.view.measuring == Some(id) {
            self.view.measuring = None;
            self.view.measured(result);
            return;
        }
        if self.view.watching == Some(id) {
            self.view.watching = None;
            self.view.watched(result);
            return;
        }
        let Some((waiting, job)) = self.view.pending else {
            return;
        };
        if waiting != id {
            return;
        }
        self.view.pending = None;
        let saving = self.view.saving.take();
        match result {
            Err(why) => {
                self.view.reopen(saving);
                self.view.said = Some((why, true));
            }
            Ok(value) => self.done(job, value),
        }
    }

    /// The lineup being written, for a test.
    #[cfg(test)]
    pub(crate) fn placed(&self) -> Option<&Lineup> {
        match &self.view.mode {
            Mode::Edit(draft) => Some(&draft.lineup),
            Mode::Browse | Mode::Draw(_) => None,
        }
    }

    /// The clip's From and To as written, for a test.
    #[cfg(test)]
    pub(crate) fn trimmed(&self) -> Option<(&str, &str)> {
        match &self.view.mode {
            Mode::Edit(draft) => Some((&draft.from, &draft.to)),
            Mode::Browse | Mode::Draw(_) => None,
        }
    }

    /// The shapes being drawn, for a test.
    #[cfg(test)]
    pub(crate) fn sketched(&self) -> Option<&[Drawing]> {
        match &self.view.mode {
            Mode::Draw(sketch) => Some(&sketch.shapes),
            Mode::Browse | Mode::Edit(_) => None,
        }
    }

    /// A screen already holding `atlas` on its first map, for the snapshots:
    /// `browse` with `picked` selected, `write` a new lineup, `any` the same
    /// for any agent, `add` one with nothing placed, `draw`, or `text` with a
    /// label picked.
    #[cfg(test)]
    pub(crate) fn showing(atlas: serde_json::Value, picked: Option<&str>, mode: &str) -> Self {
        let atlas: Atlas = serde_json::from_value(atlas).unwrap_or_default();
        let mut view = View::default();
        view.settle(&atlas);
        view.selected = picked.map(ToOwned::to_owned);
        if mode == "draw" {
            let mut sketch = view.sketch(&atlas);
            sketch.kind = 3;
            sketch.colour = 4;
            sketch.opacity = 0.6;
            sketch.picked = sketch.shapes.iter().position(|s| s.kind == "cone");
            if let Some(cone) = sketch.picked.and_then(|i| sketch.shapes.get_mut(i)) {
                cone.opacity = 0.6;
            }
            view.mode = Mode::Draw(Box::new(sketch));
        }
        if mode == "erase" {
            let mut sketch = view.sketch(&atlas);
            sketch.kind = shapes::KINDS
                .iter()
                .position(|(kind, _)| *kind == "erase")
                .unwrap_or_default();
            view.mode = Mode::Draw(Box::new(sketch));
        }
        if mode == "text" {
            let mut sketch = view.sketch(&atlas);
            sketch.kind = shapes::KINDS
                .iter()
                .position(|(kind, _)| *kind == "text")
                .unwrap_or_default();
            if let Some(label) = sketch.shapes.iter().position(|s| s.kind == "text") {
                sketch.pick(label);
            }
            view.mode = Mode::Draw(Box::new(sketch));
        }
        if mode == "add" {
            view.mode = Mode::Edit(Box::new(view.draft()));
        }
        if mode == "write" || mode == "any" {
            let agent = if mode == "any" { ANY } else { "Viper" };
            if mode == "any" {
                view.main = Some("Brimstone".to_owned());
            }
            let mut draft = Draft::new(view.map.as_deref().unwrap_or_default(), Some(agent));
            draft.lineup.title = "B Main Snake Bite".to_owned();
            draft.lineup.ability = Some("Snake Bite".to_owned());
            draft.lineup.site = Some("B".to_owned());
            draft.lineup.stand = Some([0.42, 0.70]);
            draft.placing = Place::Land;
            draft.source = "https://www.youtube.com/watch?v=DVfq4OV4veg".to_owned();
            draft.from = "2:31".to_owned();
            draft.to = "2:38".to_owned();
            draft.volume = 60.0;
            draft.measure.of.clone_from(&draft.source);
            draft.measure.length = Some(806.0);
            view.mode = Mode::Edit(Box::new(draft));
        }
        Self {
            load: Load::Have(Box::new(atlas)),
            view,
        }
    }

    /// Adds pictures dropped onto the window to the lineup being written,
    /// up to the most one can carry.
    fn dropped(&mut self, ui: &Ui) {
        let Mode::Edit(draft) = &mut self.view.mode else {
            return;
        };
        let files = ui.input(|i| i.raw.dropped_files.clone());
        for path in files.iter().map(|f| f.path()) {
            if pictures::is_picture(path) && draft.lineup.images.len() < pictures::MOST {
                draft
                    .lineup
                    .images
                    .push(path.to_string_lossy().into_owned());
            }
        }
    }

    /// Stops the video, since the screen is going away, and lets the setting
    /// decide the names again next time.
    pub(crate) fn leave(&mut self) {
        self.view.player = None;
        self.view.names = None;
        self.view.turned = 0;
    }

    /// Drops questions the socket lost, going back to what was showing, then
    /// asks for everything again.
    pub(crate) fn reconnected(&mut self, bridge: &Bridge) {
        if let Load::Asking { kept, .. } = &mut self.load {
            let restored = kept.take().map_or(Load::Idle, Load::Have);
            self.load = restored;
        }
        let lost = self.view.measuring.take().is_some() | self.view.watching.take().is_some();
        if lost && let Mode::Edit(draft) = &mut self.view.mode {
            // Asked again once the backend is back.
            draft.measure = Measure::default();
        }
        if self.view.pending.take().is_some() {
            let saving = self.view.saving.take();
            self.view.reopen(saving);
            self.view.said = Some((
                "The backend restarted before it answered. Try again.".to_owned(),
                true,
            ));
        }
        // Asked straight away rather than when the screen opens, so it opens
        // on its maps, and one opened before the backend was there gets them.
        self.ask(bridge);
    }

    /// Puts a finished request's answer on screen.
    fn done(&mut self, job: Job, value: serde_json::Value) {
        let view = &mut self.view;
        // What it says goes up whatever the list is doing, or a save that
        // lands while the screen is away leaves "Saving" up for good.
        match job {
            Job::Save => {
                let title = value.get("title").and_then(serde_json::Value::as_str);
                view.said = Some((format!("Saved {}.", title.unwrap_or("the lineup")), false));
            }
            Job::Delete => {
                view.selected = None;
                view.said = Some(("Deleted.".to_owned(), false));
            }
            Job::Drawing => {}
        }
        let Load::Have(atlas) = &mut self.load else {
            if job == Job::Save {
                view.late = serde_json::from_value::<Lineup>(value).ok();
            }
            return;
        };
        match job {
            Job::Save => {
                if let Ok(saved) = serde_json::from_value::<Lineup>(value) {
                    atlas.lineups.retain(|l| l.id != saved.id);
                    atlas.lineups.push(saved);
                }
            }
            Job::Delete => {
                let gone = value.get("deleted").and_then(serde_json::Value::as_str);
                atlas.lineups.retain(|l| l.id.as_deref() != gone);
            }
            Job::Drawing => {
                let map = value.get("map").and_then(serde_json::Value::as_str);
                let kept = value
                    .get("shapes")
                    .and_then(|s| serde_json::from_value::<Vec<Drawing>>(s.clone()).ok());
                if let (Some(map), Some(kept)) = (map, kept) {
                    atlas.drawings.insert(map.to_owned(), kept);
                }
            }
        }
    }

    /// Draws the screen, sends whatever it asked for, and saves or copies
    /// the map once the window's picture of itself arrives.
    pub(crate) fn show(
        &mut self,
        ui: &mut Ui,
        (bridge, root): (&Bridge, &Path),
        live: bool,
        (opacity, prefs, look): (f32, Prefs, Look<'_>),
    ) {
        (self.view.prefs, self.view.turn_to) = (prefs, look.turn);
        self.view.names_by_default = look.names;
        self.view.area = look.area.map(shapes::ink);
        if let Some((shown, at)) = &self.view.viewing {
            self.view.viewing =
                pictures::viewer(ui.ctx(), shown, *at).map(|next| (shown.clone(), next));
        }
        self.dropped(ui);
        // T shows or hides every lineup's name, unless a box is being typed in.
        let typing = ui.memory(|m| m.focused().is_some());
        if !typing && ui.input(|i| i.key_pressed(egui::Key::T)) {
            self.view.names = Some(!self.view.naming());
        }
        if !typing && ui.input(|i| i.key_pressed(egui::Key::R)) {
            self.view.turned = (self.view.turned + 1) % 4;
        }
        let atlas = match &self.load {
            Load::Have(atlas)
            | Load::Asking {
                kept: Some(atlas), ..
            } => atlas,
            other => {
                CentralPanel::default()
                    .frame(egui::Frame::NONE.fill(colour::BG))
                    .show(ui, |ui| {
                        ui.multiply_opacity(opacity);
                        waiting(ui, other, live);
                    });
                return;
            }
        };
        let mut asked = None;
        // Wider or narrower by dragging its edge, since the form reads
        // better with room and the map with less.
        Panel::right("lineups-side")
            .resizable(true)
            .default_size(SIDE)
            .size_range(320.0..=640.0)
            .frame(egui::Frame::NONE.fill(colour::BG_RAISED))
            .show(ui, |ui| {
                ui.multiply_opacity(opacity);
                asked = side::show(ui, atlas, &mut self.view);
            });
        CentralPanel::default()
            .frame(egui::Frame::NONE.fill(colour::BG))
            .show(ui, |ui| {
                ui.multiply_opacity(opacity);
                plan::show(ui, atlas, &mut self.view);
            });
        // A video nothing drew this frame belongs to a form or a lineup
        // that has gone, and dropping it stops its sound.
        let pass = ui.ctx().cumulative_pass_nr();
        if self
            .view
            .player
            .as_ref()
            .is_some_and(|p| p.shown_in != pass)
        {
            self.view.player = None;
        }
        if let Some(request) = asked {
            self.send(bridge, live, request);
        }
        self.keep_shapes(bridge, live);
        if self.view.export.is_some() {
            let picture = ui.input(|i| {
                i.raw.events.iter().find_map(|e| match e {
                    egui::Event::Screenshot { image, .. } => Some(Arc::clone(image)),
                    _ => None,
                })
            });
            match picture {
                Some(image) => self.view.exported(ui.ctx(), &image, root),
                None => ui.ctx().request_repaint(),
            }
        }
    }

    /// Sends a map's changed shapes to be kept, once nothing else is waiting.
    fn keep_shapes(&mut self, bridge: &Bridge, live: bool) {
        if !live || self.view.pending.is_some() {
            return;
        }
        if let Some((map, shapes)) = self.view.unsaved.take() {
            let id = bridge.ask(
                "lineup_drawing",
                serde_json::json!({ "map": map, "shapes": shapes }),
            );
            self.view.pending = Some((id, Job::Drawing));
        }
    }

    /// Sends what the side panel asked for, one request at a time.
    fn send(&mut self, bridge: &Bridge, live: bool, request: Request) {
        let view = &mut self.view;
        if !live {
            view.said = Some(("Overseer isn't connected to its backend.".to_owned(), true));
            return;
        }
        if let (Mode::Edit(draft), Request::Measure) = (&mut view.mode, request) {
            let source = draft.source.trim().to_owned();
            draft.measure = Measure {
                of: source.clone(),
                asking: true,
                ..Measure::default()
            };
            view.measuring =
                Some(bridge.ask("lineup_probe", serde_json::json!({ "source": source })));
            return;
        }
        if let (Mode::Edit(draft), Request::Watch) = (&mut view.mode, request) {
            draft.measure.fetching = true;
            let source = draft.measure.of.clone();
            view.watching =
                Some(bridge.ask("lineup_watch", serde_json::json!({ "source": source })));
            return;
        }
        if view.pending.is_some() {
            return;
        }
        let map = view.map.clone().unwrap_or_default();
        let picked = serde_json::json!({ "map": map, "id": view.selected });
        let (name, params, job) = match (&view.mode, request) {
            (Mode::Edit(draft), Request::Save) => (
                "lineup_save",
                serde_json::json!({ "lineup": draft.sent(), "clip": draft.clip() }),
                Job::Save,
            ),
            (_, Request::Delete) => ("lineup_delete", picked, Job::Delete),
            _ => return,
        };
        view.pending = Some((bridge.ask(name, params), job));
        view.said = None;
        view.confirm = false;
        // Back to the list straight away rather than sitting in the editor
        // while the clip is cut, which can take a while.
        if job == Job::Save
            && let Mode::Edit(draft) = std::mem::take(&mut view.mode)
        {
            view.said = Some((format!("Saving {}.", draft.sent().title), false));
            view.saving = Some(draft);
        }
    }
}

impl View {
    /// Whether every lineup's name shows on the map.
    fn naming(&self) -> bool {
        self.names.unwrap_or(self.names_by_default)
    }

    /// Opens the editor again on a lineup whose save didn't go through, if
    /// the screen is back on the list and not on to something else.
    fn reopen(&mut self, saving: Option<Box<Draft>>) {
        if let Some(draft) = saving
            && matches!(self.mode, Mode::Browse)
        {
            self.mode = Mode::Edit(draft);
        }
    }

    /// Takes how long the clip's source runs, if it is still the source
    /// being written.
    fn measured(&mut self, result: Result<serde_json::Value, String>) {
        let Mode::Edit(draft) = &mut self.mode else {
            return;
        };
        draft.measure.asking = false;
        match result {
            Ok(value) => {
                let source = value.get("source").and_then(serde_json::Value::as_str);
                if source == Some(draft.measure.of.as_str()) {
                    draft.measure.length = value
                        .get("length")
                        .and_then(serde_json::Value::as_f64)
                        .filter(|l| *l > 0.0);
                }
            }
            Err(why) => draft.measure.failed = Some(why),
        }
    }

    /// Takes the file to play the clip's source from, if it is still the
    /// source being written.
    fn watched(&mut self, result: Result<serde_json::Value, String>) {
        let Mode::Edit(draft) = &mut self.mode else {
            return;
        };
        draft.measure.fetching = false;
        match result {
            Ok(value) => {
                let source = value.get("source").and_then(serde_json::Value::as_str);
                if source == Some(draft.measure.of.as_str()) {
                    draft.measure.file = value
                        .get("file")
                        .and_then(serde_json::Value::as_str)
                        .map(ToOwned::to_owned);
                }
            }
            Err(why) => draft.measure.failed = Some(why),
        }
    }

    /// Keeps the map on screen if it is still one of the maps, and otherwise
    /// opens the first. Takes the agent you play most from `atlas` when no
    /// board has said.
    fn settle(&mut self, atlas: &Atlas) {
        let known = self
            .map
            .as_ref()
            .is_some_and(|m| atlas.maps.iter().any(|p| &p.name == m));
        if !known {
            self.map = atlas.maps.first().map(|p| p.name.clone());
        }
        if self.main.is_none() {
            self.main.clone_from(&atlas.main);
        }
    }

    /// A new lineup on the map on screen, for the agent picked or else the
    /// one you play. The map and the panel both start one here.
    fn draft(&self) -> Draft {
        Draft::new(
            self.map.as_deref().unwrap_or_default(),
            self.agent.as_deref().or(self.you.as_deref()),
        )
    }

    /// Starts drawing on the map on screen, with its shapes as they are.
    fn sketch(&self, atlas: &Atlas) -> Sketch {
        Sketch {
            shapes: self.shapes(atlas).to_vec(),
            kind: 1,
            colour: 0,
            spread: 60.0,
            opacity: 1.0,
            width: 0.008,
            stroke: Vec::new(),
            picked: None,
            dragging: None,
            words: String::new(),
            moving: None,
            resizing: None,
            confirm: false,
        }
    }

    /// The shapes on the map on screen: the ones being drawn, or else any
    /// changed and on their way to be kept, or else the kept ones.
    fn shapes<'a>(&'a self, atlas: &'a Atlas) -> &'a [Drawing] {
        let map = self.map.as_deref().unwrap_or_default();
        if let Mode::Draw(sketch) = &self.mode {
            return &sketch.shapes;
        }
        if let Some((changed, shapes)) = &self.unsaved
            && changed == map
        {
            return shapes;
        }
        atlas.drawings.get(map).map_or(&[], Vec::as_slice)
    }

    /// The lineups on the map on screen, for the agent picked if there is one.
    fn shown<'a>(&self, atlas: &'a Atlas) -> Vec<&'a Lineup> {
        let mut shown: Vec<&Lineup> = atlas
            .lineups
            .iter()
            .filter(|l| Some(&l.map) == self.map.as_ref())
            .filter(|l| self.agent.as_ref().is_none_or(|a| &l.agent == a))
            .filter(|l| matches(l, &self.looking_for))
            .collect();
        shown.sort_by(|a, b| self.order.compare(a, b).then_with(|| a.title.cmp(&b.title)));
        shown
    }

    /// Cuts the map out of the window's picture of itself, then saves it
    /// as a PNG or copies it.
    fn exported(&mut self, ctx: &egui::Context, image: &egui::ColorImage, root: &Path) {
        let (Some(export), Some(rect)) = (self.export.take(), self.shot.take()) else {
            return;
        };
        let map = image.region(&rect, Some(ctx.pixels_per_point()));
        if export == Export::Copy {
            ctx.copy_image(map);
            self.said = Some(("Copied the map. Paste it anywhere.".to_owned(), false));
            return;
        }
        let folder = root.join("lineups").join("exports");
        let name: String = self
            .map
            .as_deref()
            .unwrap_or("map")
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .flat_map(char::to_lowercase)
            .collect();
        let path = (1..10_000)
            .map(|n| folder.join(format!("{name}-{n}.png")))
            .find(|p| !p.exists());
        let saved = path
            .ok_or_else(|| std::io::Error::other("no free file name"))
            .and_then(|path| {
                std::fs::create_dir_all(&folder)?;
                art::save_png(&path, &map)?;
                Ok(path)
            });
        self.said = Some(match saved {
            Ok(path) => {
                self.saved_to = Some(folder);
                (format!("Saved {}.", path.display()), false)
            }
            Err(why) => (format!("Couldn't save the picture: {why}."), true),
        });
    }
}

/// An agent's face in `rect`, or for [`ANY`] a blacked-out figure with a
/// question mark, the way agent select shows somebody who hasn't picked.
/// The figure is cut from `main`, the agent you play most, when known, and
/// only its outline is used: the tint is pure black, so none of the picture
/// shows through.
fn face(painter: &egui::Painter, rect: Rect, agent: &str, lit: f32, main: Option<&str>) {
    let whole = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    if agent == ANY {
        painter.rect_filled(rect, 0, colour::BG_INSET.gamma_multiply(lit));
        let figure = main
            .and_then(|m| art::agent(painter.ctx(), m))
            .or_else(|| art::stand_in_portrait(painter.ctx(), 3));
        if let Some(figure) = figure {
            painter.image(figure.id(), rect, whole, Color32::BLACK.gamma_multiply(lit));
        }
        let _mark = overseer_ui::caps_text(
            painter,
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "?",
            overseer_ui::Face::Heavy.at(rect.height() * 0.5),
            colour::TEXT_DIM.gamma_multiply(lit),
        );
    } else if let Some(texture) = art::agent(painter.ctx(), agent) {
        painter.image(
            texture.id(),
            rect,
            whole,
            Color32::WHITE.gamma_multiply(lit),
        );
    }
}

/// An ability by name, from the agent's own kit first and then anyone's,
/// since a lineup for any agent can be for any ability in the game.
fn ability<'a>(atlas: &'a Atlas, agent: &str, name: &str) -> Option<(&'a Kit, &'a Ability)> {
    let owns = |kit: &'a Kit| {
        kit.abilities
            .iter()
            .find(|a| a.name.as_deref() == Some(name))
            .map(|a| (kit, a))
    };
    atlas
        .agents
        .iter()
        .find(|k| k.name == agent)
        .and_then(owns)
        .or_else(|| atlas.agents.iter().find_map(owns))
}

/// What to say before there is anything to draw.
fn waiting(ui: &mut Ui, load: &Load, live: bool) {
    ui.add_space(overseer_ui::space::LG);
    view::title(
        ui,
        "Lineups",
        "Your own lineups, map by map. Press Esc to go back.",
    );
    let words = match load {
        Load::Refused(why) => why.clone(),
        Load::Asking { .. } => {
            "Loading the maps. The first time fetches every minimap, which takes a few seconds."
                .to_owned()
        }
        Load::Idle | Load::Have(_) if !live => "Waiting for the backend to connect.".to_owned(),
        Load::Idle | Load::Have(_) => String::new(),
    };
    if !words.is_empty() {
        view::note(ui, &words);
    }
}

/// Seconds as a clock, 70.5 as 1:10.5, the way a video player shows them.
/// Rounded to a tenth first, so 59.96 is 1:00 and never 0:60.
fn clock(seconds: f64) -> String {
    let tenths = (seconds.max(0.0) * 10.0).round();
    let minutes = (tenths / 600.0).floor();
    let rest = (tenths - minutes * 600.0) / 10.0;
    if rest.fract() == 0.0 {
        format!("{minutes}:{rest:02}")
    } else {
        format!("{minutes}:{rest:04.1}")
    }
}

/// A typed time as seconds: 75, 75.5, 1:15, 1:15.5 or 1:01:15. Only the last
/// part may have a fraction, the same as the backend reads it.
fn seconds(text: &str) -> Option<f64> {
    let parts: Vec<&str> = text.trim().split(':').collect();
    if parts.is_empty() || parts.len() > 3 {
        return None;
    }
    let last = parts.len() - 1;
    let mut total = 0.0;
    for (at, part) in parts.iter().enumerate() {
        let whole = part.chars().all(|c| c.is_ascii_digit());
        let fraction = at == last
            && part.split_once('.').is_some_and(|(a, b)| {
                !a.is_empty()
                    && !b.is_empty()
                    && (a.to_owned() + b).chars().all(|c| c.is_ascii_digit())
            });
        if part.is_empty() || !(whole || fraction) {
            return None;
        }
        total = total * 60.0 + part.parse::<f64>().ok()?;
    }
    Some(total)
}

/// How long a clip runs, for the panel.
fn length(clip: &Clip) -> Option<f64> {
    Some(clip.to? - clip.from?).filter(|s| *s > 0.0)
}

#[cfg(test)]
mod tests {
    use super::{ANY, Draft, Lineups, Load, Mode, Order, View, ability, clock, matches, seconds};
    use overseer_core::{Ability, Atlas, Clip, Kit, Lineup};

    /// Out of a lobby the figure is cut from the agent your recorded matches
    /// say you play most, and a board that knows still has the last word.
    #[test]
    fn the_main_comes_from_your_matches_when_no_board_says() {
        let atlas = Atlas {
            main: Some("Brimstone".to_owned()),
            ..Atlas::default()
        };
        let mut view = View::default();
        view.settle(&atlas);
        assert_eq!(view.main.as_deref(), Some("Brimstone"));

        let mut seen = View {
            main: Some("Jett".to_owned()),
            ..View::default()
        };
        seen.settle(&atlas);
        assert_eq!(seen.main.as_deref(), Some("Jett"));
    }

    /// Times read back the way they would be typed.
    #[test]
    fn a_clip_time_reads_like_a_video_player() {
        assert_eq!(clock(70.0), "1:10");
        assert_eq!(clock(70.5), "1:10.5");
        assert_eq!(clock(5.0), "0:05");
        assert_eq!(clock(3675.0), "61:15");
        assert_eq!(clock(59.96), "1:00", "never 0:60");
        assert_eq!(clock(0.04), "0:00");
    }

    /// Every way a time is typed reads as seconds, and anything else as
    /// nothing, the same as the backend reads them.
    #[test]
    fn a_typed_time_reads_as_seconds() {
        for (typed, read) in [
            ("75", Some(75.0)),
            ("75.5", Some(75.5)),
            ("1:15", Some(75.0)),
            (" 1:10.5 ", Some(70.5)),
            ("1:01:15", Some(3675.0)),
            ("", None),
            ("1.5:10", None),
            ("1::10", None),
            ("1:2:3:4", None),
            ("-5", None),
            ("abc", None),
        ] {
            assert_eq!(seconds(typed), read, "{typed:?}");
        }
    }

    /// A saved lineup opens with its clip's fields filled in as typed, and
    /// goes back without the notes box's blank lines or an empty clip.
    #[test]
    fn a_saved_lineup_round_trips_through_the_editor() {
        let saved = Lineup {
            id: Some("brim-1".to_owned()),
            map: "Ascent".to_owned(),
            agent: "Brimstone".to_owned(),
            title: "A Main".to_owned(),
            notes: Some("Jump throw".to_owned()),
            stand: Some([0.6, 0.5]),
            clip: Some(Clip {
                source: Some("https://example.com/v".to_owned()),
                from: Some(70.0),
                to: Some(77.5),
                volume: Some(60),
                file: Some("C:/clip.mp4".to_owned()),
            }),
            ..Lineup::default()
        };
        let mut draft = Draft::of(&saved);
        assert_eq!((draft.from.as_str(), draft.to.as_str()), ("1:10", "1:17.5"));
        assert_eq!(draft.clip().get("volume").unwrap(), 60.0);
        draft.notes = "  \n".to_owned();
        assert_eq!(draft.sent().notes, None);
        assert_eq!(draft.sent().clip, None);
        draft.source.clear();
        assert!(draft.clip().is_null());
    }

    /// A lineup nobody named is named for its ability and site, and it can
    /// be saved once it has an agent and both of its points.
    #[test]
    fn a_lineup_needs_its_points_and_not_a_name() {
        let mut draft = Draft::new("Ascent", Some("Brimstone"));
        assert!(draft.missing().is_some());
        draft.lineup.stand = Some([0.2, 0.3]);
        draft.lineup.land = Some([0.5, 0.1]);
        assert_eq!(draft.missing(), None);
        assert_eq!(draft.sent().title, "Brimstone Lineup");
        draft.lineup.ability = Some("Incendiary".to_owned());
        draft.lineup.site = Some("A".to_owned());
        assert_eq!(draft.sent().title, "A Site Incendiary");
        draft.lineup.title = " Default Molly ".to_owned();
        assert_eq!(draft.sent().title, "Default Molly");
        draft.lineup.agent.clear();
        assert!(draft.missing().is_some());
    }

    /// An ability is found in its own agent's kit, and for any agent in
    /// whoever's kit has it.
    #[test]
    fn any_agent_can_have_any_ability() {
        let kit = |name: &str, ability: &str| Kit {
            name: name.to_owned(),
            abilities: vec![Ability {
                key: "Q".to_owned(),
                name: Some(ability.to_owned()),
                icon: None,
            }],
        };
        let atlas = Atlas {
            agents: vec![kit("Brimstone", "Incendiary"), kit("Viper", "Snake Bite")],
            ..Atlas::default()
        };
        let owner =
            |agent: &str, name: &str| ability(&atlas, agent, name).map(|(k, _)| k.name.clone());
        assert_eq!(owner("Viper", "Snake Bite").as_deref(), Some("Viper"));
        assert_eq!(owner(ANY, "Incendiary").as_deref(), Some("Brimstone"));
        assert_eq!(owner(ANY, "Nothing"), None);
    }

    /// A saved picture is the map's square cut out of the window's picture,
    /// written as a PNG named for the map, and the next one doesn't write
    /// over it.
    #[test]
    fn a_saved_picture_is_the_map_alone() {
        let root = std::env::temp_dir().join("overseer-export-test");
        drop(std::fs::remove_dir_all(&root));
        let ctx = egui::Context::default();
        let window = egui::ColorImage::new([200, 120], vec![egui::Color32::RED; 200 * 120]);
        for n in 1..=2 {
            let mut view = View {
                map: Some("Ascent".to_owned()),
                export: Some(super::Export::Save),
                shot: Some(egui::Rect::from_min_size(
                    egui::pos2(10.0, 20.0),
                    egui::vec2(80.0, 80.0),
                )),
                ..View::default()
            };
            view.exported(&ctx, &window, &root);
            let path = root
                .join("lineups")
                .join("exports")
                .join(format!("ascent-{n}.png"));
            let bytes = std::fs::read(&path).unwrap();
            assert!(bytes.starts_with(b"\x89PNG"), "{}", path.display());
            // The header's width and height, each four bytes, big-endian.
            let size = bytes.get(16..24).unwrap();
            assert_eq!(size, [0, 0, 0, 80, 0, 0, 0, 80]);
            assert!(view.export.is_none() && !view.said.unwrap().1);
        }
        drop(std::fs::remove_dir_all(&root));
    }

    /// A save's answer replaces the lineup it was and leaves the list as it
    /// was, and a failed one says why and brings the editor back.
    #[test]
    fn a_save_lands_in_the_list_or_says_why_not() {
        let mut screen = Lineups {
            load: Load::Have(Box::default()),
            ..Lineups::default()
        };
        screen.view.mode = Mode::Edit(Box::new(Draft::new("Ascent", None)));
        screen.view.pending = Some((7, super::Job::Save));
        screen.answered(7, Err("A lineup needs a title and an agent.".to_owned()));
        assert!(matches!(screen.view.mode, Mode::Edit(_)));
        assert!(screen.view.said.as_ref().unwrap().1);

        // Pressing Save closes the editor and keeps the lineup aside.
        if let Mode::Edit(draft) = std::mem::take(&mut screen.view.mode) {
            screen.view.saving = Some(draft);
        }
        screen.view.pending = Some((8, super::Job::Save));
        screen.answered(
            8,
            Ok(serde_json::json!({ "id": "b-1", "map": "Ascent", "agent": "Brimstone", "title": "A Main" })),
        );
        let Load::Have(atlas) = &screen.load else {
            panic!("the atlas went away");
        };
        assert_eq!(atlas.lineups.len(), 1);
        assert_eq!(screen.view.selected, None, "the list, with nothing picked");
        assert!(matches!(screen.view.mode, Mode::Browse));

        // A save that fails after the screen went back to the list opens
        // the lineup again as it was.
        screen.view.mode = Mode::Browse;
        screen.view.saving = Some(Box::new(Draft::new("Ascent", Some("Viper"))));
        screen.view.pending = Some((9, super::Job::Save));
        screen.answered(9, Err("yt-dlp couldn't download that video.".to_owned()));
        assert!(matches!(screen.view.mode, Mode::Edit(_)));
    }

    /// A save that lands while the list is being asked for again still says
    /// it saved, and the lineup is in the list that comes back, even when
    /// the backend read that list before the save landed.
    #[test]
    fn a_save_that_lands_while_the_list_reloads_still_shows() {
        let mut screen = Lineups {
            load: Load::Asking { id: 20, kept: None },
            ..Lineups::default()
        };
        screen.view.said = Some(("Saving A Main.".to_owned(), false));
        screen.view.pending = Some((21, super::Job::Save));
        screen.answered(
            21,
            Ok(serde_json::json!({ "id": "b-1", "map": "Ascent", "agent": "Brimstone", "title": "A Main" })),
        );
        assert_eq!(
            screen.view.said.as_ref().map(|s| s.0.as_str()),
            Some("Saved A Main.")
        );
        screen.answered(20, Ok(serde_json::json!({ "lineups": [] })));
        let Load::Have(atlas) = &screen.load else {
            panic!("the list didn't come back");
        };
        assert_eq!(atlas.lineups.len(), 1, "the saved lineup is missing");
    }

    /// The filter wants every word somewhere in a lineup, in any case, and
    /// the orders put a site before none and fall back to the name.
    #[test]
    fn the_list_filters_by_words_and_sorts_by_site() {
        let lineup = |title: &str, site: Option<&str>| Lineup {
            map: "Ascent".to_owned(),
            agent: "Brimstone".to_owned(),
            title: title.to_owned(),
            site: site.map(ToOwned::to_owned),
            notes: Some("Jump throw".to_owned()),
            ..Lineup::default()
        };
        let a = lineup("Default", Some("A"));
        assert!(matches(&a, "brim JUMP"));
        assert!(!matches(&a, "viper"));
        assert!(matches(&a, "  "));
        let atlas = Atlas {
            lineups: vec![lineup("Zed", None), lineup("Retake", Some("B")), a],
            ..Atlas::default()
        };
        let mut view = View {
            map: Some("Ascent".to_owned()),
            ..View::default()
        };
        let titles = |view: &View| {
            view.shown(&atlas)
                .iter()
                .map(|l| l.title.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(titles(&view), ["Default", "Retake", "Zed"]);
        view.order = Order::Name;
        assert_eq!(titles(&view), ["Default", "Retake", "Zed"]);
        view.looking_for = "retake".to_owned();
        assert_eq!(titles(&view), ["Retake"]);
    }
}

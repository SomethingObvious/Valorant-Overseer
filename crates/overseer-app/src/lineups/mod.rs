//! The Lineups screen: every map with bomb sites, its plantable ground lit,
//! your own lineups on it, and shapes drawn over it to show ranges, with a
//! click on the map to add a lineup and a button to save the map as a picture.
//!
//! Lineups stay in this window and never reach the overlay, since Riot's
//! rules rule out telling somebody where to go in the middle of a round.

mod areas;
mod form;
mod pick;
mod pictures;
mod plan;
mod player;
mod shapes;
mod side;
mod sight;
mod sketch;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use egui::{CentralPanel, Color32, Panel, Rect, Ui, pos2};
use overseer_core::{Ability, Atlas, Bridge, Clip, Drawing, Kit, Lineup};
use overseer_ui::{art, colour};
use serde::Deserialize;

use crate::settings::{MapTurn, RedrawSettings};
use crate::view;

use pick::{ANY, SPIKE};
pub(crate) use player::{Prefs, SIZES, SPEEDS, speed_name};
pub(crate) use shapes::COLOURS;
pub(crate) use sketch::swatch;

const SIDE: f32 = 380.0;
const SIDE_SHARE: f32 = 0.6;
const NARROW: f32 = 760.0;
const POLL: f64 = 2.0;
const POLL_FOR: f64 = 900.0;
const PASTE_MOST: usize = 512 * 1024;

#[derive(Debug, Default)]
enum Load {
    #[default]
    Idle,
    Asking {
        id: u64,
        kept: Option<Box<Atlas>>,
    },
    Have(Box<Atlas>),
    Refused(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Job {
    Save,
    Delete,
    Drawing,
    Share,
    Import,
    Confirm,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Order {
    #[default]
    Site,
    Name,
    Agent,
    Side,
}

impl Order {
    const fn label(self) -> &'static str {
        match self {
            Self::Site => "By Site",
            Self::Name => "By Name",
            Self::Agent => "By Agent",
            Self::Side => "By Side",
        }
    }

    const fn next(self) -> Self {
        match self {
            Self::Site => Self::Name,
            Self::Name => Self::Agent,
            Self::Agent => Self::Side,
            Self::Side => Self::Site,
        }
    }

    fn compare(self, a: &Lineup, b: &Lineup) -> std::cmp::Ordering {
        let key = |v: Option<&String>| (v.is_none(), v.cloned().unwrap_or_default());
        match self {
            Self::Site => key(a.site.as_ref()).cmp(&key(b.site.as_ref())),
            Self::Name => std::cmp::Ordering::Equal,
            Self::Agent => a.agent.cmp(&b.agent),
            Self::Side => key(a.side.as_ref()).cmp(&key(b.side.as_ref())),
        }
    }
}

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

#[derive(Debug, Default)]
enum Mode {
    #[default]
    Browse,
    Edit(Box<Draft>),
    Draw(Box<Sketch>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place {
    Stand,
    Land,
    More,
    Point(usize),
}

#[derive(Debug, Clone)]
struct Draft {
    lineup: Lineup,
    placing: Place,
    grab: Option<Place>,
    thrown_from: Option<[f32; 2]>,
    source: String,
    /// Where the clip starts, as typed, like 1:10.
    from: String,
    to: String,
    volume: f32,
    notes: String,
    description: String,
    find: String,
    find_ability: String,
    choosing: bool,
    clip_open: bool,
    notes_open: bool,
    measure: Measure,
    cut: Option<(String, f64)>,
    opened: serde_json::Value,
    discard: Option<f64>,
}

#[derive(Debug, Clone, Default)]
struct Measure {
    length: Option<f64>,
    of: String,
    asking: bool,
    failed: Option<String>,
    wanted: bool,
    held: Option<(f64, f64)>,
    last: Option<f64>,
    file: Option<String>,
    fetching: bool,
}

impl Draft {
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
            thrown_from: None,
            source: String::new(),
            from: String::new(),
            to: String::new(),
            volume: 100.0,
            notes: String::new(),
            description: String::new(),
            find: String::new(),
            find_ability: String::new(),
            choosing: agent.is_none(),
            clip_open: false,
            notes_open: false,
            measure: Measure::default(),
            cut: None,
            opened: serde_json::Value::Null,
            discard: None,
        }
    }

    fn started(&self) -> bool {
        self.lineup.id.is_some()
            || self.lineup.stand.is_some()
            || self.lineup.land.is_some()
            || [
                &self.lineup.title,
                &self.notes,
                &self.source,
                &self.description,
            ]
            .iter()
            .any(|t| !t.trim().is_empty())
    }

    fn of(lineup: &Lineup) -> Self {
        let clip = lineup.clip.clone().unwrap_or_default();
        let cut = clip
            .file
            .clone()
            .map(|file| (file, length(&clip).unwrap_or(form::LONGEST)));
        let mut draft = Self {
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
            thrown_from: None,
            source: clip.source.unwrap_or_default(),
            from: clip.from.map(clock).unwrap_or_default(),
            to: clip.to.map(clock).unwrap_or_default(),
            volume: clip.volume.map_or(100.0, |v| v as f32),
            notes: lineup.notes.clone().unwrap_or_default(),
            description: lineup.description.clone().unwrap_or_default(),
            find: String::new(),
            find_ability: String::new(),
            choosing: false,
            clip_open: lineup.clip.is_some(),
            notes_open: lineup.notes.is_some(),
            measure: Measure::default(),
            cut,
            opened: serde_json::Value::Null,
            discard: None,
        };
        draft.opened = draft.clip();
        draft
    }

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

    fn named(&self) -> String {
        let what = if self.lineup.agent == SPIKE {
            "Spike Plant".to_owned()
        } else {
            self.lineup
                .ability
                .clone()
                .unwrap_or_else(|| format!("{} Lineup", self.lineup.agent))
        };
        match self.lineup.site.as_deref() {
            Some(site) => format!("{site} Site {what}"),
            None => what,
        }
    }

    fn missing(&self) -> Option<&'static str> {
        if self.lineup.agent == SPIKE {
            return self
                .lineup
                .land
                .is_none()
                .then_some("Click the map where the Spike is planted.");
        }
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

    fn clip(&self) -> serde_json::Value {
        if self.source.trim().is_empty() {
            return if self.opened.is_object() {
                serde_json::json!({ "source": "" })
            } else {
                serde_json::Value::Null
            };
        }
        let clip = serde_json::json!({
            "source": self.source.trim(),
            "from": self.from.trim(),
            "to": self.to.trim(),
            "volume": self.volume.round(),
        });
        if clip == self.opened && self.cut.is_some() {
            serde_json::Value::Null
        } else {
            clip
        }
    }

    fn same_source(&self) -> bool {
        self.opened
            .get("source")
            .and_then(serde_json::Value::as_str)
            == Some(self.source.trim())
    }
}

pub(crate) const ARMED: f64 = 5.0;

const UNDO_STEPS: usize = 100;

#[derive(Debug, Clone)]
struct Sketch {
    shapes: Vec<Drawing>,
    kind: usize,
    colour: usize,
    spread: f32,
    /// How solid the next shape is, from 0.1 to 1.
    opacity: f32,
    width: f32,
    stroke: Vec<[f32; 2]>,
    picked: Option<usize>,
    dragging: Option<[f32; 2]>,
    words: String,
    moving: Option<(usize, [f32; 2], Drawing)>,
    resizing: Option<(usize, usize, Drawing)>,
    confirm: Option<f64>,
    history: Vec<Vec<Drawing>>,
    /// The shapes as of the last change that finished, which the next one
    /// is measured against.
    settled: Vec<Drawing>,
    scratch: bool,
    stamp: Option<String>,
}

impl Sketch {
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
                "stamp" => self.stamp.clone().unwrap_or_default(),
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

    fn tool(&self) -> &'static str {
        if self.stamp.is_some() {
            return "stamp";
        }
        shapes::KINDS
            .get(self.kind)
            .map_or("brush", |(kind, _)| kind)
    }

    fn lettering(&self) -> bool {
        self.tool() == "text"
    }

    const fn stamping(&self) -> bool {
        self.stamp.is_some()
    }

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Request {
    Save,
    Delete,
    Measure,
    Watch,
    Share,
    ShareMap,
    Paste,
    Confirm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Export {
    Save,
    Copy,
}

#[derive(Debug, Default)]
struct View {
    map: Option<String>,
    agent: Option<String>,
    looking_for: String,
    order: Order,
    selected: Option<String>,
    cutting: Vec<String>,
    /// When the list started being asked for again for them, and when it
    /// last was, on egui's clock.
    polled: Option<(f64, f64)>,
    widen: Option<f32>,
    widened: Option<String>,
    mode: Mode,
    pending: Option<(u64, Job)>,
    saving: Option<Box<Draft>>,
    failed: Option<Box<Draft>>,
    late: Option<Lineup>,
    measuring: Option<u64>,
    watching: Option<u64>,
    player: Option<player::Player>,
    prefs: Prefs,
    viewing: Option<(Vec<String>, usize)>,
    turn_to: MapTurn,
    names_by_default: bool,
    area: Option<Color32>,
    names: Option<bool>,
    /// Quarter turns clockwise asked for with Turn since Lineups opened, on
    /// top of the way the settings turn each map.
    turned: u8,
    said: Option<(String, bool)>,
    confirm: Option<f64>,
    default_agent: String,
    new_default: Option<String>,
    main: Option<String>,
    unsaved: std::collections::BTreeMap<String, Vec<Drawing>>,
    drawing_out: Option<(String, Vec<Drawing>)>,
    export: Option<Export>,
    shot: Option<Rect>,
    saved_to: Option<PathBuf>,
    pasting: Option<u64>,
    pasted: Option<String>,
    copy: Option<String>,
    slow: bool,
    redraws: RedrawSettings,
    redraw_seen: Option<String>,
    new_looping: Option<bool>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Look<'a> {
    pub(crate) turn: MapTurn,
    pub(crate) names: bool,
    pub(crate) area: Option<&'a str>,
    pub(crate) agent: &'a str,
    /// Whether this PC counts as slow, which cuts clips at 720p.
    pub(crate) slow: bool,
    pub(crate) redraws: &'a RedrawSettings,
}

#[derive(Debug, Default)]
pub(crate) struct Lineups {
    load: Load,
    view: View,
}

impl Lineups {
    pub(crate) const fn new_default(&mut self) -> Option<String> {
        self.view.new_default.take()
    }

    pub(crate) const fn new_looping(&mut self) -> Option<bool> {
        self.view.new_looping.take()
    }

    pub(crate) const fn redraw_seen(&mut self) -> Option<String> {
        self.view.redraw_seen.take()
    }

    pub(crate) fn open(
        &mut self,
        bridge: &Bridge,
        live: bool,
        map: Option<&str>,
        main: Option<&str>,
    ) {
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
        self.ask(bridge);
    }

    fn ask(&mut self, bridge: &Bridge) {
        let kept = match std::mem::take(&mut self.load) {
            Load::Have(atlas) => Some(atlas),
            Load::Asking { kept, .. } => kept,
            Load::Idle | Load::Refused(_) => None,
        };
        let id = bridge.ask("lineups", serde_json::json!({}));
        self.load = Load::Asking { id, kept };
    }

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

    fn take_look(&mut self, prefs: Prefs, look: Look<'_>) {
        (self.view.prefs, self.view.turn_to) = (prefs, look.turn);
        self.view.names_by_default = look.names;
        self.view.area = look.area.map(shapes::ink);
        look.agent.clone_into(&mut self.view.default_agent);
        self.view.slow = look.slow;
        if self.view.redraws != *look.redraws {
            self.view.redraws.clone_from(look.redraws);
        }
    }

    pub(crate) fn step_down(&mut self) {
        self.step(plan::Way::Down);
    }

    fn step(&mut self, way: plan::Way) {
        if let Load::Have(atlas)
        | Load::Asking {
            kept: Some(atlas), ..
        } = &self.load
            && matches!(self.view.mode, Mode::Browse)
            && let Some(next) = plan::stepped(atlas, &self.view, way)
        {
            self.view.selected = Some(next);
            self.view.confirm = None;
        }
    }

    /// The arrows go from lineup to lineup, taken before the clip in the
    /// panel sees them. A clip or picture full screen, an open menu and the
    /// lineup form keep them.
    fn arrows(&mut self, ui: &Ui) {
        if !matches!(self.view.mode, Mode::Browse)
            || self.fullscreen()
            || egui::Popup::is_any_open(ui.ctx())
        {
            return;
        }
        let pressed = ui.input_mut(|i| {
            [
                (egui::Key::ArrowUp, plan::Way::Up),
                (egui::Key::ArrowDown, plan::Way::Down),
                (egui::Key::ArrowLeft, plan::Way::Left),
                (egui::Key::ArrowRight, plan::Way::Right),
            ]
            .map(|(key, way)| (i.consume_key(egui::Modifiers::NONE, key), way))
        });
        for (hit, way) in pressed {
            if hit {
                self.step(way);
            }
        }
    }

    /// Whether a clip or a picture is showing full screen.
    pub(crate) fn fullscreen(&self) -> bool {
        self.view.viewing.is_some() || self.view.player.as_ref().is_some_and(player::Player::big)
    }

    pub(crate) fn waiting_on(&self, id: u64) -> bool {
        matches!(self.load, Load::Asking { id: waiting, .. } if waiting == id)
            || self.view.pending.is_some_and(|(waiting, _)| waiting == id)
            || self.view.measuring == Some(id)
            || self.view.watching == Some(id)
    }

    pub(crate) fn answered(&mut self, id: u64, result: Result<serde_json::Value, String>) {
        if let Load::Asking { id: waiting, .. } = self.load
            && waiting == id
        {
            let read = result.and_then(|value| {
                serde_json::from_value::<Atlas>(value).map_err(|_| {
                    "The lineups came back in a shape this version can't read. Updating Overseer usually fixes this.".to_owned()
                })
            });
            if let (Err(why), Load::Asking { kept: Some(_), .. }) = (&read, &self.load) {
                self.view.said = Some((format!("Couldn't refresh the lineups. {why}"), true));
                if let Load::Asking {
                    kept: Some(atlas), ..
                } = std::mem::take(&mut self.load)
                {
                    self.load = Load::Have(atlas);
                }
                return;
            }
            self.load = match read {
                Ok(mut atlas) => {
                    if let Some(saved) = self.view.late.take() {
                        replace(&mut atlas.lineups, saved);
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
        let drawing = self.view.drawing_out.take();
        match result {
            Err(why) => {
                self.view.unsent(drawing);
                self.view.reopen(saving);
                self.view.said = Some((why, true));
            }
            Ok(value) => self.done(job, &value),
        }
    }

    #[cfg(test)]
    pub(crate) fn placed(&self) -> Option<&Lineup> {
        match &self.view.mode {
            Mode::Edit(draft) => Some(&draft.lineup),
            Mode::Browse | Mode::Draw(_) => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn trimmed(&self) -> Option<(&str, &str)> {
        match &self.view.mode {
            Mode::Edit(draft) => Some((&draft.from, &draft.to)),
            Mode::Browse | Mode::Draw(_) => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn sketched(&self) -> Option<&[Drawing]> {
        match &self.view.mode {
            Mode::Draw(sketch) => Some(&sketch.shapes),
            Mode::Browse | Mode::Edit(_) => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn unsaved_maps(&self) -> usize {
        self.view.unsaved.len()
    }

    #[cfg(test)]
    pub(crate) fn showing(atlas: serde_json::Value, picked: Option<&str>, mode: &str) -> Self {
        let atlas: Atlas = serde_json::from_value(atlas).unwrap_or_default();
        let mut view = View::default();
        view.settle(&atlas);
        view.selected = picked.map(ToOwned::to_owned);
        if mode == "draw" {
            let mut sketch = view.sketch(&atlas);
            sketch.kind = shapes::KINDS
                .iter()
                .position(|(kind, _)| *kind == "cone")
                .unwrap_or_default();
            sketch.colour = 4;
            sketch.opacity = 0.6;
            sketch.picked = sketch.shapes.iter().position(|s| s.kind == "cone");
            if let Some(cone) = sketch.picked.and_then(|i| sketch.shapes.get_mut(i)) {
                cone.opacity = 0.6;
            }
            view.mode = Mode::Draw(Box::new(sketch));
        }
        if mode == "sandbox" {
            let mut sketch = view.sandbox(&atlas);
            sketch.stamp = Some(SPIKE.to_owned());
            sketch
                .shapes
                .push(sketch.drawing([0.42, 0.38], [0.42, 0.38]));
            sketch.stamp = Some("Viper".to_owned());
            sketch.colour = 6;
            sketch.shapes.push(sketch.drawing([0.62, 0.7], [0.62, 0.7]));
            sketch.settled = sketch.shapes.clone();
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

    fn dropped(&mut self, ui: &Ui) {
        let Mode::Edit(draft) = &mut self.view.mode else {
            return;
        };
        let mut files: Vec<PathBuf> = ui.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect()
        });
        files.extend(overseer_native::take_pictures());
        for path in &files {
            if pictures::is_picture(path) && draft.lineup.images.len() < pictures::MOST {
                draft
                    .lineup
                    .images
                    .push(path.to_string_lossy().into_owned());
            }
        }
    }

    pub(crate) fn leave(&mut self) {
        self.view.player = None;
        // Ends the video process, which gives back what Windows' media
        // engine and its drivers took.
        overseer_native::let_go();
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
            draft.measure = Measure::default();
        }
        if self.view.pending.take().is_some() {
            let saving = self.view.saving.take();
            let drawing = self.view.drawing_out.take();
            self.view.unsent(drawing);
            self.view.reopen(saving);
            self.view.said = Some((
                "The backend restarted before it answered. Try again.".to_owned(),
                true,
            ));
        }
        self.ask(bridge);
    }

    fn done(&mut self, job: Job, value: &serde_json::Value) {
        let view = &mut self.view;
        let added: Vec<Lineup> = value
            .get("added")
            .filter(|_| job == Job::Import)
            .and_then(|a| serde_json::from_value(a.clone()).ok())
            .unwrap_or_default();
        let saved = matches!(job, Job::Save | Job::Confirm)
            .then(|| Lineup::deserialize(value).ok())
            .flatten();
        let later = view.wait_for(added.iter().chain(&saved));
        match job {
            Job::Save => {
                let title = saved.as_ref().map_or("the lineup", |l| l.title.as_str());
                let later = if later {
                    " Its clip plays once it's cut."
                } else {
                    ""
                };
                view.said = Some((format!("Saved {title}.{later}"), false));
            }
            Job::Delete => {
                view.selected = None;
                view.said = Some(("Deleted.".to_owned(), false));
            }
            Job::Share | Job::Import => {
                let said = value.get("said").and_then(serde_json::Value::as_str);
                view.said = said.map(|s| (s.to_owned(), false));
                let text = |key| value.get(key).and_then(serde_json::Value::as_str);
                if job == Job::Share {
                    view.copy = text("code").map(ToOwned::to_owned);
                } else if let Some(map) = text("map")
                    && matches!(view.mode, Mode::Browse)
                {
                    view.map = Some(map.to_owned());
                    view.selected = None;
                }
            }
            Job::Drawing | Job::Confirm => {}
        }
        let Load::Have(atlas) = &mut self.load else {
            if job == Job::Save {
                view.late = saved;
            }
            return;
        };
        match job {
            Job::Save | Job::Confirm => {
                if let Some(saved) = saved {
                    replace(&mut atlas.lineups, saved);
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
            Job::Import => {
                for lineup in added {
                    replace(&mut atlas.lineups, lineup);
                }
            }
            Job::Share => {}
        }
    }

    pub(crate) fn show(
        &mut self,
        ui: &mut Ui,
        (bridge, root): (&Bridge, &Path),
        live: bool,
        (opacity, prefs, look): (f32, Prefs, Look<'_>),
    ) {
        self.take_look(prefs, look);
        self.view.reopen_failed();
        if let Some((shown, at)) = &self.view.viewing {
            self.view.viewing =
                pictures::viewer(ui.ctx(), shown, *at).map(|next| (shown.clone(), next));
        }
        self.dropped(ui);
        if ui.memory(|m| m.focused().is_none()) {
            if ui.input(|i| i.key_pressed(egui::Key::T)) {
                self.view.names = Some(!self.view.naming());
            }
            if ui.input(|i| i.key_pressed(egui::Key::R)) {
                self.view.turned = (self.view.turned + 1) % 4;
            }
            self.arrows(ui);
        }
        self.poll(ui, bridge, live);
        let atlas = match &self.load {
            Load::Have(atlas)
            | Load::Asking {
                kept: Some(atlas), ..
            } => atlas,
            other => {
                // Nothing draws the video now, and Windows' engine would
                // stay where it was over this.
                self.view.player = None;
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
        self.view.side_panel(ui).show(ui, |ui| {
            ui.multiply_opacity(opacity);
            asked = side::show(ui, atlas, &mut self.view);
        });
        let mut retry = false;
        CentralPanel::default()
            .frame(egui::Frame::NONE.fill(colour::BG))
            .show(ui, |ui| {
                ui.multiply_opacity(opacity);
                retry = plan::show(ui, atlas, &mut self.view);
            });
        if let Some(on) = self.view.player.as_mut().and_then(|p| p.looped.take()) {
            self.view.prefs.looping = on;
            self.view.new_looping = Some(on);
        }
        let pass = ui.ctx().cumulative_pass_nr();
        if self
            .view
            .player
            .as_ref()
            .is_some_and(|p| p.shown_in != pass)
        {
            self.view.player = None;
        }
        if retry && live {
            self.ask(bridge);
        }
        if let Some(request) = asked {
            self.send(bridge, live, request);
        }
        self.keep_shapes(bridge, live);
        self.paste(ui, bridge, live);
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

    fn paste(&mut self, ui: &Ui, bridge: &Bridge, live: bool) {
        if let Some(code) = self.view.copy.take() {
            ui.ctx().copy_text(code);
        }
        let Some(asked_on) = self.view.pasting else {
            return;
        };
        let pasted = ui.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Paste(text) => Some(text.clone()),
                _ => None,
            })
        });
        if let Some(text) = pasted {
            self.view.pasting = None;
            if text.len() > PASTE_MOST {
                self.view.said = Some((
                    "That's far too much text to be a lineup code.".to_owned(),
                    true,
                ));
                return;
            }
            self.view.pasted = Some(text);
            self.send(bridge, live, Request::Paste);
        } else if ui.ctx().cumulative_pass_nr() > asked_on + 3 {
            self.view.pasting = None;
            self.view.said = Some((
                "There's no text on the clipboard. Copy a lineup code first.".to_owned(),
                true,
            ));
        } else {
            ui.ctx().request_repaint();
        }
    }

    fn poll(&mut self, ui: &Ui, bridge: &Bridge, live: bool) {
        if self.view.cutting.is_empty() || !live {
            return;
        }
        let now = ui.input(|i| i.time);
        let (began, last) = *self.view.polled.get_or_insert((now, now));
        if now - began > POLL_FOR {
            self.view.cutting.clear();
            self.view.polled = None;
            return;
        }
        if now - last >= POLL && !matches!(self.load, Load::Asking { .. }) {
            self.view.polled = Some((began, now));
            self.ask(bridge);
        }
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs_f64(POLL));
    }

    fn keep_shapes(&mut self, bridge: &Bridge, live: bool) {
        if !live || self.view.pending.is_some() {
            return;
        }
        if let Some((map, shapes)) = self.view.unsaved.pop_first() {
            let id = bridge.ask(
                "lineup_drawing",
                serde_json::json!({ "map": &map, "shapes": &shapes }),
            );
            self.view.pending = Some((id, Job::Drawing));
            self.view.drawing_out = Some((map, shapes));
        }
    }

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
            view.said = Some((
                "Overseer is still on the last one. Try again once it's done.".to_owned(),
                true,
            ));
            return;
        }
        let map = view.map.clone().unwrap_or_default();
        let picked = serde_json::json!({ "map": map, "id": view.selected });
        let (name, params, job) = match (&view.mode, request) {
            (Mode::Edit(draft), Request::Save) => (
                "lineup_save",
                serde_json::json!({
                    "lineup": draft.sent(),
                    "clip": draft.clip(),
                    "small": view.slow,
                }),
                Job::Save,
            ),
            (_, Request::Delete) => ("lineup_delete", picked, Job::Delete),
            (_, Request::Confirm) => ("lineup_confirm", picked, Job::Confirm),
            (_, Request::Share) => (
                "lineup_export",
                serde_json::json!({ "ids": [view.selected] }),
                Job::Share,
            ),
            (_, Request::ShareMap) => (
                "lineup_export",
                serde_json::json!({ "map": map }),
                Job::Share,
            ),
            (_, Request::Paste) => (
                "lineup_import",
                serde_json::json!({ "code": view.pasted.take(), "small": view.slow }),
                Job::Import,
            ),
            _ => return,
        };
        if matches!(job, Job::Save | Job::Delete) {
            // Ends the player's ffmpeg, which has the clip open, before the
            // backend replaces or deletes it.
            view.player = None;
        }
        view.pending = Some((bridge.ask(name, params), job));
        view.said = (job == Job::Import).then(|| ("Bringing in the lineups.".to_owned(), false));
        view.confirm = None;
        if job == Job::Save
            && let Mode::Edit(draft) = std::mem::take(&mut view.mode)
        {
            view.said = Some((format!("Saving {}.", draft.sent().title), false));
            view.saving = Some(draft);
        }
    }
}

impl View {
    fn marked(&self, atlas: &Atlas, lineup: &Lineup) -> bool {
        self.redraws.mark_redrawn && from_before(atlas, lineup)
    }

    fn wait_for<'a>(&mut self, lineups: impl Iterator<Item = &'a Lineup>) -> bool {
        let before = self.cutting.len();
        self.cutting.extend(
            lineups
                .filter(|l| l.clip.as_ref().is_some_and(uncut))
                .filter_map(|l| l.id.clone()),
        );
        let more = self.cutting.len() > before;
        if more {
            self.polled = None;
        }
        more
    }

    fn naming(&self) -> bool {
        self.names.unwrap_or(self.names_by_default)
    }

    fn side_panel(&mut self, ui: &Ui) -> Panel {
        let room = ui.available_width();
        let (least, default, most) = if room < NARROW {
            (room * 0.45, room * 0.5, room * 0.55)
        } else {
            (320.0, SIDE, (room * SIDE_SHARE).max(640.0))
        };
        let id = egui::Id::new("lineups-side");
        let side = Panel::right(id)
            .resizable(true)
            .default_size(default)
            .size_range(least..=most)
            .frame(egui::Frame::NONE.fill(colour::BG_RAISED));
        let Some(more) = self.widen.take() else {
            return side;
        };
        let now = egui::PanelState::load(ui.ctx(), id).map_or(default, |s| s.size().x);
        let wanted = (now + more).min(most);
        if wanted <= now + 1.0 {
            return side;
        }
        ui.ctx().data_mut(|d| d.remove::<egui::PanelState>(id));
        side.default_size(wanted)
    }

    fn unsent(&mut self, drawing: Option<(String, Vec<Drawing>)>) {
        if let Some((map, shapes)) = drawing {
            self.unsaved.entry(map).or_insert(shapes);
        }
    }

    fn reopen(&mut self, saving: Option<Box<Draft>>) {
        if let Some(draft) = saving {
            if matches!(self.mode, Mode::Browse) {
                self.mode = Mode::Edit(draft);
            } else {
                self.failed = Some(draft);
            }
        }
    }

    fn reopen_failed(&mut self) {
        if matches!(self.mode, Mode::Browse)
            && let Some(draft) = self.failed.take()
        {
            self.mode = Mode::Edit(draft);
        }
    }

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
        self.cutting.retain(|id| {
            atlas
                .lineups
                .iter()
                .any(|l| l.id.as_ref() == Some(id) && l.clip.as_ref().is_some_and(uncut))
        });
        if self.cutting.is_empty() {
            self.polled = None;
        }
    }

    fn draft(&self) -> Draft {
        Draft::new(
            self.map.as_deref().unwrap_or_default(),
            self.agent.as_deref().or(Some(&self.default_agent)),
        )
    }

    fn sketch(&self, atlas: &Atlas) -> Sketch {
        let shapes = self.shapes(atlas).to_vec();
        Sketch {
            settled: shapes.clone(),
            history: Vec::new(),
            shapes,
            kind: 0,
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
            confirm: None,
            scratch: false,
            stamp: None,
        }
    }

    fn sandbox(&self, atlas: &Atlas) -> Sketch {
        Sketch {
            shapes: Vec::new(),
            settled: Vec::new(),
            scratch: true,
            ..self.sketch(atlas)
        }
    }

    fn shapes<'a>(&'a self, atlas: &'a Atlas) -> &'a [Drawing] {
        let map = self.map.as_deref().unwrap_or_default();
        if let Mode::Draw(sketch) = &self.mode {
            return &sketch.shapes;
        }
        if let Some(shapes) = self.unsaved.get(map) {
            return shapes;
        }
        if let Some((changed, shapes)) = &self.drawing_out
            && changed == map
        {
            return shapes;
        }
        atlas.drawings.get(map).map_or(&[], Vec::as_slice)
    }

    fn shown<'a>(&self, atlas: &'a Atlas) -> Vec<&'a Lineup> {
        if matches!(&self.mode, Mode::Draw(sketch) if sketch.scratch) {
            return Vec::new();
        }
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

fn face(painter: &egui::Painter, rect: Rect, agent: &str, lit: f32, main: Option<&str>) {
    let whole = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    if agent == SPIKE {
        spike(
            painter,
            rect.center(),
            rect.height() * 0.4,
            Color32::WHITE.gamma_multiply(lit),
        );
    } else if agent == ANY {
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

fn spike(painter: &egui::Painter, at: egui::Pos2, half: f32, ink: Color32) {
    if let Some(icon) = art::spike(painter.ctx()) {
        let [x, y] = art::SPIKE_CORE;
        let shift = egui::vec2(0.5 - x, 0.5 - y) * (half * 2.0);
        painter.image(
            icon.id(),
            Rect::from_center_size(at + shift, egui::vec2(half, half) * 2.0),
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            ink,
        );
    }
}

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

fn from_before(atlas: &Atlas, lineup: &Lineup) -> bool {
    let changed = atlas
        .maps
        .iter()
        .find(|m| m.name == lineup.map)
        .and_then(|m| m.changed);
    changed.is_some_and(|at| lineup.saved.is_some_and(|saved| saved < at))
}

fn replace(lineups: &mut Vec<Lineup>, lineup: Lineup) {
    lineups.retain(|l| l.id != lineup.id);
    lineups.push(lineup);
}

const fn uncut(clip: &Clip) -> bool {
    clip.file.is_none() && clip.failed.is_none()
}

fn length(clip: &Clip) -> Option<f64> {
    Some(clip.to? - clip.from?).filter(|s| *s > 0.0)
}

#[cfg(test)]
mod tests {
    use super::{Draft, Lineups, Load, Mode, View, seconds};
    use overseer_core::{Clip, Lineup};

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
                failed: None,
            }),
            ..Lineup::default()
        };
        let mut draft = Draft::of(&saved);
        assert_eq!((draft.from.as_str(), draft.to.as_str()), ("1:10", "1:17.5"));
        assert_eq!(draft.cut, Some(("C:/clip.mp4".to_owned(), 7.5)));
        assert!(!draft.measure.wanted, "its source is fetched unasked");
        assert!(
            draft.clip().is_null(),
            "an untouched clip is sent to be cut"
        );
        draft.volume = 80.0;
        assert_eq!(draft.clip().get("volume").unwrap(), 80.0);
        draft.notes = "  \n".to_owned();
        assert_eq!(draft.sent().notes, None);
        assert_eq!(draft.sent().clip, None);
        assert!(draft.same_source());
        draft.source.clear();
        assert_eq!(draft.clip(), serde_json::json!({ "source": "" }));
        assert!(Draft::new("Ascent", None).clip().is_null());
        let uncut = Lineup {
            clip: saved.clip.clone().map(|c| Clip { file: None, ..c }),
            ..saved
        };
        assert!(!Draft::of(&uncut).clip().is_null());
    }

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
            let size = bytes.get(16..24).unwrap();
            assert_eq!(size, [0, 0, 0, 80, 0, 0, 0, 80]);
            assert!(view.export.is_none() && !view.said.unwrap().1);
        }
        drop(std::fs::remove_dir_all(&root));
    }

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

        screen.view.mode = Mode::Browse;
        screen.view.saving = Some(Box::new(Draft::new("Ascent", Some("Viper"))));
        screen.view.pending = Some((9, super::Job::Save));
        screen.answered(9, Err("yt-dlp couldn't download that video.".to_owned()));
        assert!(matches!(screen.view.mode, Mode::Edit(_)));
    }

    #[test]
    fn a_failed_refresh_keeps_the_lineups_on_screen() {
        let mut screen = Lineups {
            load: Load::Asking {
                id: 30,
                kept: Some(Box::default()),
            },
            ..Lineups::default()
        };
        screen.answered(30, Err("The backend went away.".to_owned()));
        assert!(
            matches!(screen.load, Load::Have(_)),
            "the lineups went away"
        );
        assert!(
            screen.view.said.as_ref().is_some_and(|s| s.1),
            "it didn't say so"
        );

        screen.load = Load::Asking { id: 31, kept: None };
        screen.answered(31, Err("The backend went away.".to_owned()));
        assert!(matches!(screen.load, Load::Refused(_)));
    }

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
}

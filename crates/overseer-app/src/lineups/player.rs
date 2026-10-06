//! A video played inside the window. In the lineup panel Windows' own media
//! engine plays it in a child window, decoded by the low-power GPU and
//! composited by Windows, so playing costs a few percent of a core and the
//! window only redraws to move the timeline. Everywhere else, and wherever
//! that engine can't open the file, ffmpeg decodes it to raw frames on a
//! thread of its own, the window shows each one when its time comes, and
//! ffplay plays the sound with no window of its own. Over the bottom of the
//! video sits a bar like a video site's: play and pause, a timeline to skim
//! along, the volume, the speed and full screen.

use std::io::{BufRead, BufReader, Read};
use std::os::windows::process::CommandExt;
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError, sync_channel};
use std::time::Duration;

use egui::{
    Align2, Color32, ColorImage, CursorIcon, FontId, Id, Key, Rect, Sense, Shape, Stroke,
    TextureHandle, TextureOptions, Ui, pos2, vec2,
};
use overseer_ui::{colour, size, space};

/// The most pixels a frame has on its longest side, which is 1080p either
/// way up. Each keeps the video's own shape, so a short is as tall as a wide
/// video is wide. Frames are made the size they are drawn up to this.
const LONGEST_SIDE: usize = 1920;
/// The steps the frame size goes up in, so resizing the panel doesn't
/// restart ffmpeg on every frame of the drag.
const SIDE_STEP: f32 = 240.0;
/// Windows' `BELOW_NORMAL_PRIORITY_CLASS`, so the game gets the CPU before
/// the clip does.
const BELOW_NORMAL: u32 = 0x0000_4000;
/// Frames a second of the video, whatever the speed.
const FPS: f64 = 30.0;
/// How many decoded frames wait for their turn. A few is enough to ride out
/// a slow one, and more would only hold memory.
const AHEAD: usize = 4;
/// The sizes there are to pick from: how much of the window's height the
/// video may take in the panel, by name. Full screen is for seeing it big.
pub(crate) const SIZES: [(f32, &str); 4] = [
    (0.5, "Small"),
    (0.63, "Medium"),
    (0.8, "Large"),
    (0.95, "Largest"),
];
/// The speeds there are to pick from.
pub(crate) const SPEEDS: [f32; 7] = [0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 2.0];
/// How tall the bar over the bottom of the video is.
const STRIP: f32 = 46.0;
/// How often the timeline moves while Windows' engine plays, in seconds.
/// Each move redraws the whole window, so it is a few times a second
/// rather than every frame.
const TICK: f64 = 0.25;
/// How long a looping part holds its last frame before it starts over, so
/// the end of the throw reads before the start of the next.
const REST: f64 = 0.4;
/// How big a scrub strip's frames are on their longest side. Small, since
/// they only show while a handle is dragged, and many of them are kept.
const STRIP_SIDE: usize = 360;
/// The most frames one scrub strip keeps, about 50 MB at its size.
const STRIP_MOST: f64 = 240.0;
/// The least height the video keeps when little of the panel is left under
/// it.
const SHORTEST: f32 = 180.0;
/// How far the arrow keys skip, in seconds.
const SKIP: f64 = 5.0;

/// A decoded frame: its width and height, and its pixels as RGB.
type Frame = ([usize; 2], Vec<u8>);

/// How a clip plays when it first shows, from the settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Prefs {
    /// How fast, 1 being as recorded.
    pub(crate) speed: f32,
    /// How loud, from 0 to 100.
    pub(crate) volume: f32,
    /// Whether it starts playing on its own.
    pub(crate) autoplay: bool,
    /// Whether it starts over when it ends.
    pub(crate) looping: bool,
    /// How much of the window's height it may take in the panel.
    pub(crate) size: f32,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            speed: 1.25,
            volume: 0.0,
            autoplay: true,
            looping: true,
            size: 0.63,
        }
    }
}

/// What a speed is called on its button and in the settings.
pub(crate) fn speed_name(speed: f32) -> String {
    if (speed - 1.0).abs() < f32::EPSILON {
        "Normal".to_owned()
    } else {
        format!("{speed}x")
    }
}

/// A video file, and whatever is decoding or playing it. Dropping it stops
/// both programs.
pub(crate) struct Player {
    /// The file it plays.
    file: String,
    /// The frame on screen.
    texture: Option<TextureHandle>,
    /// The ffmpeg decoding now, and the frames it has made.
    run: Option<Run>,
    /// The next pass of a looping part, started in its last second so its
    /// first frame is waiting when the part ends, rather than the picture
    /// freezing while a new ffmpeg seeks back.
    ahead: Option<Run>,
    /// When a looping part that has reached its end starts over, while it
    /// holds its last frame.
    rest: Option<f64>,
    /// ffplay, while the sound plays.
    sound: Option<Child>,
    /// A still asked for while another was still being made, made next.
    next_still: Option<f64>,
    /// Where in the video the frame on screen is, in seconds.
    at: f64,
    /// The part it plays, from and to.
    span: (f64, f64),
    /// Where the part playing now ends.
    till: f64,
    /// The clip's own volume as a percentage, which the listening level
    /// scales.
    gain: f32,
    /// The loudest the volume bar goes: 100, or while a clip is being cut
    /// the 200 its volume slider goes to.
    top: f32,
    /// The clip volume the bar last took its place from, while a clip is
    /// being cut.
    followed: Option<f32>,
    /// When to start the sound again at a level the volume slider moved it
    /// to, once the slider has sat still for a moment.
    resound: Option<f64>,
    /// How it plays: its speed, how loud (its `volume`), whether it
    /// loops or starts on its own, and how much of the window it takes.
    /// They start as the settings say and change as it is played.
    prefs: Prefs,
    /// Whether the sound is off, leaving the level where it was.
    muted: bool,
    /// Whether it is showing full screen.
    big: bool,
    /// How many pixels its frames have on their longest side, to fit where
    /// it is drawn.
    side: usize,
    /// The frame to show when it is first drawn, once it knows how big to
    /// make it.
    first: Option<f64>,
    /// While the timeline is held: whether it was playing when taken, and
    /// the time under the pointer.
    scrub: Option<(bool, f64)>,
    /// Small frames of the stretch being cut, decoded ahead.
    strip: Option<Strip>,
    /// The pass it was last drawn in, so the screen can drop one nobody sees.
    pub(super) shown_in: u64,
    /// Windows' engine playing it, once [`Player::native`] asked for it and
    /// it opened. While it is here, nothing below starts an ffmpeg.
    native: Option<overseer_video::Video>,
    /// Whether to use Windows' engine: asked for, and not failed yet.
    engine: Engine,
    /// Where the engine's video goes this frame: the box, the part of the
    /// screen that can be seen, and the layer it is drawn on.
    spot: Option<(Rect, Rect, egui::LayerId)>,
}

/// Which engine plays a clip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Engine {
    /// ffmpeg, which the trimming form needs for its stills and strip.
    Ffmpeg,
    /// Windows' media engine, for the lineup panel.
    Native,
    /// Windows' engine couldn't open or play it, so ffmpeg does.
    Failed,
}

/// Small frames of a stretch of the video, decoded ahead on a thread of their
/// own, so dragging a trim handle or the timeline shows the frame under it at
/// once. Each full size frame is its own ffmpeg seeking and decoding, which
/// takes a fifth of a second or more.
struct Strip {
    /// Where the stretch starts and ends, in seconds.
    from: f64,
    /// See `from`.
    to: f64,
    /// Frames a second it keeps.
    fps: f64,
    /// The frames so far, in order.
    frames: std::sync::Arc<std::sync::Mutex<Vec<Frame>>>,
    /// The ffmpeg making them, while it does.
    child: Option<Child>,
}

impl std::fmt::Debug for Player {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Player")
            .field("file", &self.file)
            .field("at", &self.at)
            .field("playing", &self.playing())
            .finish_non_exhaustive()
    }
}

/// One ffmpeg, and the frames its thread has read off it.
struct Run {
    /// The process.
    child: Child,
    /// Its frames, in order.
    frames: Receiver<Frame>,
    /// Where in the video its first frame is.
    from: f64,
    /// When the first frame went on screen, on egui's clock, while playing.
    /// A still has none.
    clock: Option<Clock>,
    /// Frames taken off it so far.
    taken: u32,
}

/// The clock a playing run keeps time by.
#[derive(Debug, Clone, Copy)]
struct Clock {
    /// When its first frame went on screen, once it has.
    started: Option<f64>,
    /// How fast it plays.
    speed: f32,
}

/// The player in `slot` for `file`, made new with `prefs` and showing the
/// frame at `at` when the slot is empty or holds another file's.
pub(super) fn of<'slot>(
    slot: &'slot mut Option<Player>,
    file: &str,
    at: f64,
    prefs: Prefs,
) -> &'slot mut Player {
    if slot.as_ref().is_none_or(|p| p.file != file) {
        let mut fresh = Player::new(file, prefs);
        fresh.first = Some(at);
        *slot = Some(fresh);
    }
    let player = slot.get_or_insert_with(|| Player::new(file, prefs));
    (player.prefs.looping, player.prefs.size) = (prefs.looping, prefs.size);
    player
}

impl Player {
    /// A player for `file`, showing nothing until it is asked for a frame.
    fn new(file: &str, prefs: Prefs) -> Self {
        Self {
            file: file.to_owned(),
            texture: None,
            run: None,
            ahead: None,
            rest: None,
            sound: None,
            next_still: None,
            at: 0.0,
            span: (0.0, 0.0),
            till: 0.0,
            gain: 100.0,
            prefs: Prefs {
                volume: prefs.volume.clamp(0.0, 100.0),
                ..prefs
            },
            muted: false,
            top: 100.0,
            followed: None,
            resound: None,
            big: false,
            side: 720,
            first: None,
            scrub: None,
            strip: None,
            shown_in: 0,
            native: None,
            engine: Engine::Ffmpeg,
            spot: None,
        }
    }

    /// Plays it with Windows' media engine, unless that has already failed
    /// for this file.
    pub(super) fn native(&mut self) -> &mut Self {
        if self.engine == Engine::Ffmpeg {
            self.engine = Engine::Native;
        }
        self
    }

    /// Opens Windows' engine the first time it is wanted, falling back to
    /// ffmpeg for good when it can't open the file or later can't play it.
    fn open_native(&mut self, ctx: &egui::Context) {
        if self
            .native
            .as_ref()
            .is_some_and(overseer_video::Video::failed)
        {
            self.native = None;
            self.engine = Engine::Failed;
            self.first = Some(self.at);
        }
        if self.engine != Engine::Native || self.native.is_some() {
            return;
        }
        let ctx = ctx.clone();
        match overseer_video::Video::open(&self.file, move || ctx.request_repaint()) {
            Ok(video) => {
                self.native = Some(video);
                self.sound();
            }
            Err(_) => self.engine = Engine::Failed,
        }
    }

    /// Whether it is playing.
    pub(super) fn playing(&self) -> bool {
        self.rest.is_some()
            || self.run.as_ref().is_some_and(|r| r.clock.is_some())
            || self
                .native
                .as_ref()
                .is_some_and(overseer_video::Video::playing)
    }

    /// Decodes small frames of `from` to `to` ahead, unless the strip it
    /// has covers them already. One that couldn't start is not tried again
    /// for the same stretch.
    pub(super) fn prepare(&mut self, (from, to): (f64, f64)) {
        if to <= from
            || self
                .strip
                .as_ref()
                .is_some_and(|s| s.from <= from + 0.05 && s.to >= to - 0.05)
        {
            return;
        }
        if let Some(mut child) = self.strip.take().and_then(|s| s.child) {
            end(&mut child);
        }
        let fps = (STRIP_MOST / (to - from)).clamp(2.0, 10.0);
        let frames = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let child = decode(
            &self.file,
            (from, Some(to - from), false),
            (STRIP_SIDE, fps),
        )
        .ok()
        .map(|(child, made)| {
            let kept = std::sync::Arc::clone(&frames);
            drop(std::thread::spawn(move || {
                for frame in made {
                    let Ok(mut list) = kept.lock() else {
                        return;
                    };
                    list.push(frame);
                }
            }));
            child
        });
        self.strip = Some(Strip {
            from,
            to,
            fps,
            frames,
            child,
        });
    }

    /// Shows the frame nearest `at` from the strip straight away, while a
    /// handle or the timeline is dragged, or the exact one the slow way when
    /// the strip hasn't got that far.
    pub(super) fn preview(&mut self, ctx: &egui::Context, at: f64) {
        let image = self.strip.as_ref().and_then(|strip| {
            let index = ((at - strip.from) * strip.fps).round();
            if at < strip.from || at > strip.to || index < 0.0 {
                return None;
            }
            strip
                .frames
                .lock()
                .ok()?
                .get(index as usize)
                .map(|(size, pixels)| ColorImage::from_rgb(*size, pixels))
        });
        let Some(image) = image else {
            self.still(at);
            return;
        };
        // A still on its way would land after this one and cover it.
        self.stop();
        self.at = at;
        match &mut self.texture {
            Some(texture) => texture.set(image, TextureOptions::LINEAR),
            None => {
                self.texture =
                    Some(ctx.load_texture("lineup-video", image, TextureOptions::LINEAR));
            }
        }
    }

    /// Puts the volume bar where the clip's own volume slider is, from 0 to
    /// 200, while a clip is being cut, and moves it whenever the slider
    /// moves. Dragging the bar changes only what plays, not what is saved.
    pub(super) fn follow(&mut self, volume: f32, now: f64) {
        self.top = 200.0;
        if self.followed == Some(volume) {
            return;
        }
        self.followed = Some(volume);
        (self.prefs.volume, self.muted) = (volume, false);
        if self.playing() {
            // Once the slider sits still, rather than one ffplay a step.
            self.resound = Some(now + 0.25);
        }
    }

    /// Whether it is showing full screen.
    pub(super) const fn big(&self) -> bool {
        self.big
    }

    /// Where it has got to, while it plays.
    pub(super) fn playhead(&self) -> Option<f64> {
        self.playing().then_some(self.at)
    }

    /// Shows the frame at `at`, stopping anything playing. While another
    /// still is being made this one waits for it, so dragging a handle runs
    /// one ffmpeg at a time and not one a frame.
    pub(super) fn still(&mut self, at: f64) {
        if self.playing() {
            self.stop();
        }
        if let Some(video) = &self.native {
            video.seek(at);
            self.at = at;
            return;
        }
        if self.run.is_some() {
            self.next_still = Some(at);
            return;
        }
        self.start(at, None);
    }

    /// Plays `from` to `to`.
    fn play(&mut self, from: f64, to: f64) {
        self.stop();
        self.till = to;
        if let Some(video) = &self.native {
            video.set_rate(f64::from(self.prefs.speed));
            video.seek(from);
            video.play();
            self.at = from;
            self.sound();
            return;
        }
        self.start(from, Some(to - from));
        self.sound();
    }

    /// Pauses it, or plays its part on from where it was paused, or from the
    /// start when it was outside the part or had got to the end.
    fn toggle(&mut self) {
        if self.playing() {
            self.stop();
            return;
        }
        let (from, to) = self.span;
        let on = (from..to - 1.0 / FPS).contains(&self.at);
        self.play(if on { self.at } else { from }, to);
    }

    /// Makes its frames the size of `rect` on screen, a step above it at
    /// most, rather than 1080p for a clip a few hundred pixels wide. The
    /// frame on screen is made again at the new size from where it is.
    fn fit(&mut self, rect: Rect, pixels_per_point: f32) {
        if self.native.is_some() {
            // Windows' engine scales the picture itself.
            return;
        }
        let longest = rect.width().max(rect.height()) * pixels_per_point;
        let steps = (longest / SIDE_STEP).ceil().max(1.0) as usize;
        let side = (steps * SIDE_STEP as usize).min(LONGEST_SIDE);
        if side == self.side {
            return;
        }
        self.side = side;
        if self.run.is_some() || self.texture.is_some() {
            self.seek(self.at);
        }
    }

    /// Goes to `at` within its part, playing on from there if it was playing.
    fn seek(&mut self, at: f64) {
        let (from, to) = self.span;
        let at = at.clamp(from, to);
        if self.playing() {
            self.play(at, to);
        } else {
            self.still(at);
        }
    }

    /// Starts the sound from where the video is, in place of any playing.
    fn sound(&mut self) {
        if let Some(mut sound) = self.sound.take() {
            end(&mut sound);
        }
        let volume = if self.muted {
            0.0
        } else {
            self.gain * self.prefs.volume / 100.0
        };
        if let Some(video) = &self.native {
            video.set_volume(f64::from(volume) / 100.0);
            return;
        }
        if self.playing() && volume > 0.0 {
            let length = self.till - self.at;
            self.sound = sound(&self.file, self.at, length, volume, self.prefs.speed).ok();
        }
    }

    /// Stops both programs, leaving the frame on screen.
    pub(super) fn stop(&mut self) {
        for mut run in [self.run.take(), self.ahead.take()].into_iter().flatten() {
            end(&mut run.child);
        }
        self.rest = None;
        if let Some(mut sound) = self.sound.take() {
            end(&mut sound);
        }
        self.next_still = None;
        if let Some(video) = &self.native {
            video.pause();
        }
    }

    /// Starts an ffmpeg at `from`, for `length` seconds or else one frame.
    fn start(&mut self, from: f64, length: Option<f64>) {
        self.run = self.spawn(from, length);
    }

    /// An ffmpeg at `from`, for `length` seconds or else one frame, not yet
    /// on screen.
    fn spawn(&self, from: f64, length: Option<f64>) -> Option<Run> {
        let from = from.max(0.0);
        let speed = self.prefs.speed;
        decode(
            &self.file,
            (from, length, length.is_some()),
            (self.side, FPS),
        )
        .ok()
        .map(|(child, frames)| Run {
            child,
            frames,
            from,
            clock: length.map(|_| Clock {
                started: None,
                speed,
            }),
            taken: 0,
        })
    }

    /// Takes whatever frames are due and puts the newest on screen. A part
    /// that played to its end starts over when it loops.
    fn advance(&mut self, ctx: &egui::Context) {
        if let Some(until) = self.rest {
            let now = ctx.input(|i| i.time);
            if now < until {
                ctx.request_repaint_after(Duration::from_secs_f64(until - now));
                return;
            }
            self.rest = None;
            let (from, to) = self.span;
            match self.ahead.take() {
                Some(ahead) => {
                    (self.run, self.at, self.till) = (Some(ahead), from, to);
                    self.sound();
                }
                None => self.play(from, to),
            }
        }
        let Some(run) = &mut self.run else {
            return;
        };
        let now = ctx.input(|i| i.time);
        let mut newest = None;
        let mut ended = false;
        loop {
            // Playing, a frame waits until its time. A still takes its one
            // frame the moment it is there.
            if let Some(Clock {
                started: Some(started),
                speed,
            }) = run.clock
                && f64::from(run.taken) > (now - started) * FPS * f64::from(speed)
            {
                break;
            }
            match run.frames.try_recv() {
                Ok(frame) => {
                    if let Some(clock) = &mut run.clock {
                        clock.started.get_or_insert(now);
                    }
                    run.taken = run.taken.saturating_add(1);
                    newest = Some(frame);
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    ended = true;
                    break;
                }
            }
        }
        // Only a run that showed something starts over, or a file ffmpeg
        // can't read would start a new ffmpeg every frame.
        let played = run.clock.is_some() && run.taken > 0;
        if let Some((size, pixels)) = newest {
            self.at = run.from + f64::from(run.taken.saturating_sub(1)) / FPS;
            let image = ColorImage::from_rgb(size, &pixels);
            match &mut self.texture {
                Some(texture) => texture.set(image, TextureOptions::LINEAR),
                None => {
                    self.texture =
                        Some(ctx.load_texture("lineup-video", image, TextureOptions::LINEAR));
                }
            }
        }
        let (from, to) = self.span;
        if played && self.prefs.looping && self.ahead.is_none() && to - self.at < 1.0 {
            self.ahead = self.spawn(from, Some(to - from));
        }
        if ended {
            let next = self.next_still.take();
            let ahead = self.ahead.take();
            self.stop();
            if let Some(next) = next {
                self.start(next, None);
            } else if played && self.prefs.looping {
                // The last frame stays up for a moment, with the next pass
                // already waiting behind it.
                self.ahead = ahead;
                self.rest = Some(ctx.input(|i| i.time) + REST);
                ctx.request_repaint_after(Duration::from_secs_f64(REST));
            } else if let Some(mut ahead) = ahead {
                end(&mut ahead.child);
            }
        } else {
            ctx.request_repaint_after(Duration::from_secs_f64(1.0 / FPS));
        }
    }

    /// Follows Windows' engine: where it has got to, and a loop held on its
    /// last frame for [`REST`] before it starts over, as the ffmpeg one does.
    fn advance_native(&mut self, ctx: &egui::Context) {
        let Some(video) = &self.native else {
            return;
        };
        let now = ctx.input(|i| i.time);
        let (from, _) = self.span;
        if let Some(until) = self.rest {
            if now < until {
                ctx.request_repaint_after(Duration::from_secs_f64(until - now));
                return;
            }
            self.rest = None;
            video.seek(from);
            video.play();
        }
        if video.take_ended() && self.prefs.looping {
            self.rest = Some(now + REST);
            ctx.request_repaint_after(Duration::from_secs_f64(REST));
            return;
        }
        self.at = video.time();
        if video.playing() {
            ctx.request_repaint_after(Duration::from_secs_f64(TICK));
        }
    }

    /// The video's width over its height, 16:9 until a frame says.
    fn aspect(&self) -> f32 {
        if let Some([w, h]) = self.native.as_ref().and_then(overseer_video::Video::size) {
            return w as f32 / h.max(1) as f32;
        }
        self.texture.as_ref().map_or(16.0 / 9.0, |t| {
            let [w, h] = t.size();
            w as f32 / h.max(1) as f32
        })
    }

    /// The frame on screen, on black, filling `rect`. The first frame of a
    /// video fades up out of the black rather than snapping on.
    fn picture(&mut self, painter: &egui::Painter, rect: Rect) {
        painter.rect_filled(rect, 0, Color32::BLACK);
        if self.native.is_some() {
            // Windows' engine draws over this, once it is placed.
            self.spot = Some((rect, painter.clip_rect(), painter.layer_id()));
            return;
        }
        if let Some(texture) = &self.texture {
            let id = Id::new(("lineup-video-in", &self.file));
            let shown = overseer_ui::motion::enter(painter.ctx(), id, 0.0);
            let full = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
            painter.image(
                texture.id(),
                rect,
                full,
                Color32::WHITE.gamma_multiply(shown),
            );
        }
    }

    /// Draws the video in its own shape, as wide as the panel unless that
    /// would make it taller than its share of the window, with its bar
    /// over the bottom of it. `span` is the part it plays and `gain` the
    /// clip's own volume. Full screen, the panel keeps the frame and the
    /// controls go to the big one.
    pub(super) fn show(&mut self, ui: &mut Ui, span: (f64, f64), gain: f32, room: f32) {
        (self.span, self.gain) = (span, gain);
        self.open_native(ui.ctx());
        if self.native.is_some() {
            self.advance_native(ui.ctx());
        } else {
            self.advance(ui.ctx());
        }
        self.shown_in = ui.ctx().cumulative_pass_nr();
        if let Some(at) = self.resound {
            if ui.input(|i| i.time) >= at {
                self.resound = None;
                self.sound();
            } else {
                ui.ctx().request_repaint();
            }
        }
        self.keys(ui);
        let aspect = self.aspect();
        let wide = ui.available_width();
        // Never taller than what is left on screen under it, less `room`
        // for what comes after it, so the clip, its bar and that are all in
        // view without scrolling.
        let left = ui.clip_rect().bottom() - ui.cursor().top() - space::SM - room;
        // Nothing can be drawn over Windows' engine, so its bar goes under it.
        let under = self.bar_under();
        let high = (wide / aspect)
            .min(ui.ctx().content_rect().height() * self.prefs.size)
            .min(left.max(SHORTEST) - under);
        let (row, _) = ui.allocate_exact_size(vec2(wide, high + under), Sense::hover());
        let rect = Rect::from_center_size(
            row.center() - vec2(0.0, under / 2.0),
            vec2(high * aspect, high),
        );
        if !self.big {
            self.fit(rect, ui.ctx().pixels_per_point());
        }
        // After the size is known, so the first frames are made that size.
        // Playing straight away needs no still to replace a moment later.
        let first = self.first.take();
        if std::mem::take(&mut self.prefs.autoplay) {
            self.toggle();
        } else if let Some(at) = first {
            self.still(at);
        }
        if self.big {
            if ui.is_rect_visible(rect) {
                self.picture(ui.painter(), rect);
            }
            self.fullscreen(ui.ctx());
        } else {
            self.screen(ui, rect, "panel");
        }
        let spot = self.spot.take();
        if let Some(video) = &mut self.native {
            let shown = spot.and_then(|(rect, seen, layer)| placed(ui.ctx(), rect, seen, layer));
            // Hidden under a menu, it looks again shortly, since egui only
            // knows a menu has gone a frame after it has.
            let in_sight = spot.is_some_and(|(rect, seen, _)| rect.intersect(seen).is_positive());
            if in_sight && shown.is_none() {
                ui.ctx().request_repaint_after(Duration::from_millis(100));
            }
            video.place(shown);
        }
    }

    /// How much room the bar takes under the video, which is none unless
    /// Windows' engine is playing it.
    const fn bar_under(&self) -> f32 {
        if self.native.is_some() { STRIP } else { 0.0 }
    }

    /// Space plays or pauses, F goes full screen and back, and the arrows
    /// skip [`SKIP`] seconds, while no text box has the keyboard.
    fn keys(&mut self, ui: &Ui) {
        if ui.memory(|m| m.focused().is_some()) {
            return;
        }
        let (space, f, left, right) = ui.input(|i| {
            (
                i.key_pressed(Key::Space),
                i.key_pressed(Key::F) && !i.modifiers.command,
                i.key_pressed(Key::ArrowLeft),
                i.key_pressed(Key::ArrowRight),
            )
        });
        if space {
            self.toggle();
        }
        if f {
            self.big = !self.big;
        }
        if left || right {
            let skip = if right { SKIP } else { -SKIP };
            self.seek(self.at + skip);
        }
    }

    /// The video big in the middle of the window over a dark backdrop, with
    /// an X in its corner. The X, a click on the backdrop or Escape go back.
    fn fullscreen(&mut self, ctx: &egui::Context) {
        let room = ctx.content_rect().shrink(40.0);
        let aspect = self.aspect();
        let under = self.bar_under();
        let high = (room.height() - under).min(room.width() / aspect);
        let mut close = false;
        let shown = egui::Modal::new(Id::new("lineup-video-big"))
            .frame(egui::Frame::NONE)
            .backdrop_color(Color32::from_black_alpha(215))
            .show(ctx, |ui| {
                let (whole, _) =
                    ui.allocate_exact_size(vec2(high * aspect, high + under), Sense::hover());
                let rect = Rect::from_min_size(whole.min, vec2(whole.width(), high));
                self.fit(rect, ctx.pixels_per_point());
                self.screen(ui, rect, "big");
                // Above the video when Windows' engine would cover it.
                let corner = if under > 0.0 {
                    vec2(-17.0, -22.0)
                } else {
                    vec2(-26.0, 26.0)
                };
                let spot = Rect::from_center_size(rect.right_top() + corner, vec2(34.0, 34.0));
                let x = ui
                    .interact(spot, ui.id().with("lineup-video-close"), Sense::click())
                    .on_hover_cursor(CursorIcon::PointingHand);
                cross(ui.painter(), spot, x.hovered());
                close = x.clicked();
            });
        if close || shown.should_close() {
            self.big = false;
        }
    }

    /// The video in `rect` with its controls over the bottom of it. They
    /// show while the pointer is over it, and always while it is paused. A
    /// click on the video plays or pauses it.
    fn screen(&mut self, ui: &Ui, rect: Rect, salt: &str) {
        let id = ui.id().with(("lineup-video", salt));
        let video = ui
            .interact(rect, id, Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        let near = ui.rect_contains_pointer(rect)
            || self.native.is_some()
            || !self.playing()
            || self.scrub.is_some()
            || egui::Popup::is_id_open(ui.ctx(), id.with("speed"));
        let fade = ui.ctx().animate_bool_with_time(id.with("fade"), near, 0.2);
        if ui.is_rect_visible(rect) {
            self.picture(ui.painter(), rect);
            if !self.playing() && self.native.is_none() {
                mark(ui.painter(), rect.center(), video.hovered());
            }
        }
        if video.clicked() {
            self.toggle();
        }
        if fade > 0.0 {
            self.controls(ui, rect, id, fade);
        }
    }

    /// The bar over the bottom of the video: the timeline along its top,
    /// then play and the time on the left, and the volume, the speed and
    /// full screen on the right.
    fn controls(&mut self, ui: &Ui, rect: Rect, id: Id, fade: f32) {
        let under = self.bar_under();
        let rect = Rect::from_min_max(rect.min, rect.max + vec2(0.0, under));
        let painter = ui.painter_at(rect);
        let strip = Rect::from_min_max(pos2(rect.left(), rect.bottom() - STRIP), rect.max);
        if under > 0.0 {
            painter.rect_filled(strip, 0, Color32::BLACK);
        } else {
            shade(
                &painter,
                Rect::from_min_max(strip.min - vec2(0.0, 24.0), strip.max),
                fade,
            );
        }
        let line = Rect::from_min_max(
            pos2(strip.left() + 10.0, strip.top()),
            pos2(strip.right() - 10.0, strip.top() + 14.0),
        );
        self.timeline(ui, &painter, line, id, fade);
        let y = f32::midpoint(line.bottom(), strip.bottom());
        let ink = |hot: bool| {
            if hot {
                colour::TEXT_STRONG
            } else {
                colour::TEXT
            }
            .gamma_multiply(fade)
        };

        let spot = Rect::from_center_size(pos2(strip.left() + 22.0, y), vec2(28.0, 28.0));
        let play = button(ui, spot, id.with("play"));
        if self.playing() {
            for dx in [-4.0, 4.0] {
                let bar = Rect::from_center_size(spot.center() + vec2(dx, 0.0), vec2(4.0, 13.0));
                painter.rect_filled(bar, 1.0, ink(play.hovered()));
            }
        } else {
            triangle(
                &painter,
                spot.center() + vec2(-1.0, 0.0),
                0.65,
                ink(play.hovered()),
            );
        }
        if play.clicked() {
            self.toggle();
        }

        // The rest from the right edge in.
        let mut x = strip.right() - 8.0;
        let spot = Rect::from_min_max(pos2(x - 28.0, y - 14.0), pos2(x, y + 14.0));
        let full = button(ui, spot, id.with("big"));
        corners(&painter, spot, self.big, ink(full.hovered()));
        if full.clicked() {
            self.big = !self.big;
        }
        x = spot.left() - 4.0;

        x = self.speed_button(ui, &painter, pos2(x, y), id, fade) - 8.0;

        let reach = (rect.width() * 0.22).clamp(36.0, 84.0);
        let bar = Rect::from_min_max(pos2(x - reach, y - 10.0), pos2(x, y + 10.0));
        self.volume(ui, &painter, bar, id, fade);
        let spot = Rect::from_min_max(
            pos2(bar.left() - 30.0, y - 13.0),
            pos2(bar.left() - 4.0, y + 13.0),
        );
        let speaker = button(ui, spot, id.with("mute"));
        speaker_mark(
            &painter,
            spot,
            self.muted || self.prefs.volume <= 0.0,
            ink(speaker.hovered()),
        );
        if speaker.clicked() {
            self.muted = !self.muted;
            self.sound();
        }

        self.time(&painter, (strip.left() + 42.0, spot.left() - 8.0), y, fade);
    }

    /// How far in it is and how long the part runs, between `room`'s two
    /// ends when it fits there.
    fn time(&self, painter: &egui::Painter, room: (f32, f32), y: f32, fade: f32) {
        let at = self.scrub.map_or(self.at, |(_, t)| t);
        let (from, to) = self.span;
        let time = format!(
            "{} / {}",
            super::clock((at - from).clamp(0.0, to - from)),
            super::clock(to - from)
        );
        let font = FontId::proportional(size::LABEL);
        let galley = painter.layout_no_wrap(time, font, colour::TEXT.gamma_multiply(fade));
        if room.0 + galley.size().x < room.1 {
            painter.galley(
                pos2(room.0, y - galley.size().y / 2.0),
                galley,
                colour::TEXT,
            );
        }
    }

    /// The timeline across the part playing, with what has played filled
    /// in. Pressing or dragging on it shows the frame under the pointer, and
    /// letting go plays on from there if it was playing.
    fn timeline(&mut self, ui: &Ui, painter: &egui::Painter, rect: Rect, id: Id, fade: f32) {
        let (from, to) = self.span;
        let response = ui
            .interact(rect, id.with("timeline"), Sense::click_and_drag())
            .on_hover_cursor(CursorIcon::PointingHand);
        let held = response.is_pointer_button_down_on();
        if held && let Some(pointer) = response.interact_pointer_pos() {
            let share = f64::from(((pointer.x - rect.left()) / rect.width()).clamp(0.0, 1.0));
            let t = (to - from).mul_add(share, from);
            let was = self.scrub.map_or_else(|| self.playing(), |(was, _)| was);
            self.scrub = Some((was, t));
            self.preview(ui.ctx(), t);
        } else if let Some((was, t)) = self.scrub.take() {
            // The exact frame once let go, since the strip's are only near.
            if was {
                self.play(t, to);
            } else {
                self.still(t);
            }
        }
        let at = self.scrub.map_or(self.at, |(_, t)| t);
        let share = ((at - from) / (to - from).max(0.001)).clamp(0.0, 1.0) as f32;
        let x = rect.width().mul_add(share, rect.left());
        let hot = held || response.hovered();
        let track = Rect::from_center_size(
            rect.center(),
            vec2(rect.width(), if hot { 5.0 } else { 3.0 }),
        );
        painter.rect_filled(
            track,
            2.0,
            Color32::from_white_alpha(70).gamma_multiply(fade),
        );
        painter.rect_filled(
            Rect::from_x_y_ranges(track.left()..=x, track.y_range()),
            2.0,
            colour::ENEMY.gamma_multiply(fade),
        );
        if hot {
            painter.circle_filled(
                pos2(x, rect.center().y),
                6.0,
                colour::ENEMY.gamma_multiply(fade),
            );
        }
    }

    /// How loud, as a bar filled to the level with a knob at its end.
    /// Pressing or dragging along it sets the level, and the sound starts
    /// again at it once let go.
    fn volume(&mut self, ui: &Ui, painter: &egui::Painter, rect: Rect, id: Id, fade: f32) {
        let response = ui
            .interact(rect, id.with("volume"), Sense::click_and_drag())
            .on_hover_cursor(CursorIcon::PointingHand);
        if response.is_pointer_button_down_on()
            && let Some(pointer) = response.interact_pointer_pos()
        {
            self.prefs.volume =
                ((pointer.x - rect.left()) / rect.width()).clamp(0.0, 1.0) * self.top;
            self.muted = false;
        }
        if response.drag_stopped() || response.clicked() {
            self.sound();
        }
        let share = if self.muted {
            0.0
        } else {
            (self.prefs.volume / self.top).clamp(0.0, 1.0)
        };
        let x = rect.width().mul_add(share, rect.left());
        let track = Rect::from_center_size(rect.center(), vec2(rect.width(), 4.0));
        painter.rect_filled(
            track,
            2.0,
            Color32::from_white_alpha(70).gamma_multiply(fade),
        );
        painter.rect_filled(
            Rect::from_x_y_ranges(track.left()..=x, track.y_range()),
            2.0,
            colour::TEXT_STRONG.gamma_multiply(fade),
        );
        let knob = if response.hovered() || response.dragged() {
            6.0
        } else {
            5.0
        };
        painter.circle_filled(
            pos2(x, rect.center().y),
            knob,
            colour::TEXT_STRONG.gamma_multiply(fade),
        );
    }

    /// The speed's name as a button whose right end is at `end`, opening the
    /// speed menu. Returns where its left end is.
    fn speed_button(
        &mut self,
        ui: &Ui,
        painter: &egui::Painter,
        end: egui::Pos2,
        id: Id,
        fade: f32,
    ) -> f32 {
        let font = FontId::proportional(size::LABEL);
        let name = format!("{}x", self.prefs.speed);
        let wide = painter
            .layout_no_wrap(name.clone(), font.clone(), Color32::WHITE)
            .size()
            .x;
        let spot = Rect::from_min_max(end - vec2(wide + 14.0, 11.0), end + vec2(0.0, 11.0));
        let speed = button(ui, spot, id.with("speed-button"));
        let ink = if speed.hovered() {
            painter.rect_filled(spot, 4.0, Color32::from_white_alpha(28));
            colour::TEXT_STRONG
        } else {
            colour::TEXT
        };
        painter.text(
            spot.center(),
            Align2::CENTER_CENTER,
            name,
            font,
            ink.gamma_multiply(fade),
        );
        self.speed_menu(&speed, id.with("speed"));
        spot.left()
    }

    /// The menu of [`SPEEDS`] that `button` opens, above it so it stays on
    /// the video: a dark card with a row for each speed and the one playing
    /// marked in red. A new speed plays on from where it is at that speed.
    fn speed_menu(&mut self, button: &egui::Response, id: Id) {
        let mut picked = None;
        let card = egui::Frame::NONE
            .fill(colour::BG_RAISED)
            .stroke(Stroke::new(1.0, colour::LINE))
            .corner_radius(6)
            .inner_margin(4);
        let _menu = egui::Popup::menu(button)
            .id(id)
            .align(egui::RectAlign::TOP_END)
            .gap(6.0)
            .frame(card)
            .show(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let (head, _) = ui.allocate_exact_size(vec2(120.0, 22.0), Sense::hover());
                ui.painter().text(
                    head.left_center() + vec2(10.0, 0.0),
                    Align2::LEFT_CENTER,
                    "Playback speed",
                    FontId::proportional(size::MICRO),
                    colour::TEXT_FAINT,
                );
                for speed in SPEEDS {
                    let on = (speed - self.prefs.speed).abs() < f32::EPSILON;
                    let (rect, row) = ui.allocate_exact_size(vec2(120.0, 26.0), Sense::click());
                    let row = row.on_hover_cursor(CursorIcon::PointingHand);
                    let painter = ui.painter();
                    if row.hovered() {
                        painter.rect_filled(rect, 4.0, colour::BG_HOVER);
                    } else if on {
                        painter.rect_filled(rect, 4.0, colour::BG_SELECTED);
                    }
                    if on {
                        let mark = Rect::from_center_size(
                            pos2(rect.left() + 4.0, rect.center().y),
                            vec2(3.0, 14.0),
                        );
                        painter.rect_filled(mark, 1.5, colour::ENEMY);
                    }
                    painter.text(
                        rect.left_center() + vec2(12.0, 0.0),
                        Align2::LEFT_CENTER,
                        speed_name(speed),
                        FontId::proportional(size::BODY),
                        if on || row.hovered() {
                            colour::TEXT_STRONG
                        } else {
                            colour::TEXT
                        },
                    );
                    if row.clicked() {
                        picked = Some(speed);
                        ui.close();
                    }
                }
            });
        if let Some(speed) = picked {
            self.prefs.speed = speed;
            if self.playing() {
                self.play(self.at, self.till);
            }
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.stop();
        if let Some(mut child) = self.strip.take().and_then(|s| s.child) {
            end(&mut child);
        }
    }
}

/// Where Windows' engine puts the video for `rect` on screen, in the
/// window's pixels, with the part of it inside `seen` that can be seen.
/// Nothing while it is out of sight, or while a menu or a dialog on another
/// layer is over it, since the engine's window would cover that.
fn placed(
    ctx: &egui::Context,
    rect: Rect,
    seen: Rect,
    layer: egui::LayerId,
) -> Option<(overseer_video::Area, overseer_video::Area)> {
    let seen = rect.intersect(seen);
    if seen.width() < 1.0 || seen.height() < 1.0 {
        return None;
    }
    let inside = seen.shrink(1.0);
    let covered = (0..=4_u8).any(|row| {
        (0..=4_u8).any(|col| {
            let at = pos2(
                inside.width().mul_add(f32::from(col) / 4.0, inside.left()),
                inside.height().mul_add(f32::from(row) / 4.0, inside.top()),
            );
            ctx.layer_id_at(at).is_some_and(|top| top != layer)
        })
    });
    if covered {
        return None;
    }
    let scale = ctx.pixels_per_point();
    let pixels =
        |r: Rect| [r.left(), r.top(), r.right(), r.bottom()].map(|v| (v * scale).round() as i32);
    Some((pixels(rect), pixels(seen)))
}

/// A click target in `rect` with a pointing hand over it.
fn button(ui: &Ui, rect: Rect, id: Id) -> egui::Response {
    ui.interact(rect, id, Sense::click())
        .on_hover_cursor(CursorIcon::PointingHand)
}

/// A dark wash up from the bottom of `rect`, so the controls read over any
/// picture.
fn shade(painter: &egui::Painter, rect: Rect, fade: f32) {
    let dark = Color32::from_black_alpha((200.0 * fade) as u8);
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), Color32::TRANSPARENT);
    mesh.colored_vertex(rect.right_top(), Color32::TRANSPARENT);
    mesh.colored_vertex(rect.right_bottom(), dark);
    mesh.colored_vertex(rect.left_bottom(), dark);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(mesh);
}

/// A play mark: a dark disc with a cream triangle in it, brighter under the
/// pointer.
fn mark(painter: &egui::Painter, centre: egui::Pos2, hot: bool) {
    painter.circle_filled(
        centre,
        22.0,
        Color32::from_black_alpha(if hot { 200 } else { 150 }),
    );
    let ink = if hot {
        colour::TEXT_STRONG
    } else {
        colour::TEXT
    };
    triangle(painter, centre, 1.0, ink);
}

/// A play triangle around `centre`, `scale` times the size of the big one.
fn triangle(painter: &egui::Painter, centre: egui::Pos2, scale: f32, ink: Color32) {
    painter.add(Shape::convex_polygon(
        vec![
            centre + vec2(-7.0, -10.0) * scale,
            centre + vec2(11.0, 0.0) * scale,
            centre + vec2(-7.0, 10.0) * scale,
        ],
        ink,
        Stroke::NONE,
    ));
}

/// The full screen mark: four corners pointing out, or pointing in to leave
/// full screen.
fn corners(painter: &egui::Painter, rect: Rect, leave: bool, ink: Color32) {
    let stroke = Stroke::new(2.0, ink);
    for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        let (corner, arm) = if leave {
            (rect.center() + vec2(sx * 3.0, sy * 3.0), 4.0)
        } else {
            (rect.center() + vec2(sx * 7.0, sy * 6.0), -4.0)
        };
        painter.line_segment([corner, corner + vec2(sx * arm, 0.0)], stroke);
        painter.line_segment([corner, corner + vec2(0.0, sy * arm)], stroke);
    }
}

/// The X that leaves full screen: a dark disc with a cross, brighter under
/// the pointer.
pub(super) fn cross(painter: &egui::Painter, rect: Rect, hot: bool) {
    painter.circle_filled(
        rect.center(),
        rect.width() / 2.0,
        Color32::from_black_alpha(if hot { 230 } else { 170 }),
    );
    let ink = if hot {
        colour::TEXT_STRONG
    } else {
        colour::TEXT
    };
    let c = rect.center();
    let stroke = Stroke::new(2.0, ink);
    painter.line_segment([c + vec2(-6.0, -6.0), c + vec2(6.0, 6.0)], stroke);
    painter.line_segment([c + vec2(-6.0, 6.0), c + vec2(6.0, -6.0)], stroke);
}

/// A speaker in `rect`, with sound waves or else a cross when it is silent.
fn speaker_mark(painter: &egui::Painter, rect: Rect, silent: bool, ink: Color32) {
    let c = rect.center() + vec2(-4.0, 0.0);
    painter.add(Shape::convex_polygon(
        vec![
            c + vec2(-5.0, -3.0),
            c + vec2(-1.0, -3.0),
            c + vec2(4.0, -7.0),
            c + vec2(4.0, 7.0),
            c + vec2(-1.0, 3.0),
            c + vec2(-5.0, 3.0),
        ],
        ink,
        Stroke::NONE,
    ));
    let stroke = Stroke::new(1.5, ink);
    if silent {
        let x = c + vec2(10.0, 0.0);
        painter.line_segment([x + vec2(-3.0, -3.0), x + vec2(3.0, 3.0)], stroke);
        painter.line_segment([x + vec2(-3.0, 3.0), x + vec2(3.0, -3.0)], stroke);
    } else {
        painter.line_segment([c + vec2(7.0, -3.0), c + vec2(7.0, 3.0)], stroke);
        painter.line_segment([c + vec2(10.0, -6.0), c + vec2(10.0, 6.0)], stroke);
    }
}

/// Starts ffmpeg on `file` at `from`, for `length` seconds or else one frame,
/// with a thread reading its frames into a channel a few frames deep. Each
/// Where `name`, like `ffmpeg`, is: winget's copy when it has one, the same
/// one the backend picks, or else whichever is first on PATH. winget keeps a
/// link to it, or without Developer Mode leaves it in its package folder,
/// where a newer build sits in a newer folder.
fn program(name: &str) -> std::path::PathBuf {
    let exe = format!("{name}.exe");
    let winget = std::env::var_os("LOCALAPPDATA").map(|local| {
        std::path::PathBuf::from(local)
            .join("Microsoft")
            .join("WinGet")
    });
    let Some(winget) = winget else {
        return std::path::PathBuf::from(name);
    };
    let linked = winget.join("Links").join(&exe);
    if linked.is_file() {
        return linked;
    }
    let packages = std::fs::read_dir(winget.join("Packages"))
        .into_iter()
        .flatten()
        .flatten();
    packages
        .flat_map(|package| {
            let builds = std::fs::read_dir(package.path())
                .into_iter()
                .flatten()
                .flatten();
            let nested: Vec<_> = builds.map(|b| b.path().join("bin").join(&exe)).collect();
            std::iter::once(package.path().join(&exe)).chain(nested)
        })
        .filter(|path| path.is_file())
        .max_by_key(|path| path.metadata().and_then(|m| m.modified()).ok())
        .unwrap_or_else(|| std::path::PathBuf::from(name))
}

/// An ffmpeg decoding `file` from `from` for `length` seconds, or one frame
/// without one, at `fps` and no more than `side` pixels on its longest side,
/// with a thread reading its frames off it. Each frame comes out as a PPM,
/// whose header says how big it is. A `paced` one is read no faster than it
/// plays.
fn decode(
    file: &str,
    (from, length, paced): (f64, Option<f64>, bool),
    (side, fps): (usize, f64),
) -> std::io::Result<(Child, Receiver<Frame>)> {
    let mut command = Command::new(program("ffmpeg"));
    command.args(["-v", "error", "-nostdin"]);
    if paced {
        // One thread still decodes a 1080x1920 clip four times faster than
        // it plays at 1.25x, and spreading it over every core took 20 to 50%
        // more CPU time for the same frames.
        command.args(["-threads", "1"]);
    }
    command.args(["-ss", &format!("{from:.3}"), "-i", file]);
    match length {
        Some(length) => command.args(["-t", &format!("{length:.3}")]),
        None => command.args(["-frames:v", "1"]),
    };
    // Frames are dropped to `fps` before they are resized, not after, and
    // bilinear costs a third less than the default bicubic with no jagged
    // edges, which the cheapest one left on a crosshair.
    let fit = format!(
        "fps={fps},scale='min({side},iw)':'min({side},ih)':\
         force_original_aspect_ratio=decrease:force_divisible_by=2:flags=bilinear"
    );
    command.args([
        "-an",
        "-vf",
        &fit,
        "-f",
        "image2pipe",
        "-c:v",
        "ppm",
        "-pix_fmt",
        "rgb24",
        "pipe:1",
    ]);
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(crate::NO_WINDOW | BELOW_NORMAL)
        .spawn()?;
    let Some(out) = child.stdout.take() else {
        end(&mut child);
        return Err(std::io::Error::other("ffmpeg started without an output"));
    };
    let (sender, frames) = sync_channel(AHEAD);
    drop(std::thread::spawn(move || read_frames(out, &sender)));
    Ok((child, frames))
}

/// Reads whole frames until ffmpeg stops or nobody wants them any more.
fn read_frames(out: ChildStdout, sender: &SyncSender<Frame>) {
    let mut out = BufReader::new(out);
    while let Some(size) = header(&mut out) {
        let mut pixels = vec![0; size[0] * size[1] * 3];
        if out.read_exact(&mut pixels).is_err() || sender.send((size, pixels)).is_err() {
            return;
        }
    }
}

/// The width and height a PPM's header gives, like `P6\n640 360\n255\n`,
/// reading up to the one blank after it where the pixels start.
fn header(out: &mut impl BufRead) -> Option<[usize; 2]> {
    let mut words: Vec<Vec<u8>> = Vec::new();
    let mut word = Vec::new();
    while words.len() < 4 {
        let mut byte = [0];
        out.read_exact(&mut byte).ok()?;
        if byte[0].is_ascii_whitespace() {
            if !word.is_empty() {
                words.push(std::mem::take(&mut word));
            }
        } else {
            word.push(byte[0]);
        }
    }
    let number = |w: &[u8]| std::str::from_utf8(w).ok()?.parse::<usize>().ok();
    match words.as_slice() {
        [magic, w, h, max] if magic == b"P6" && max == b"255" => {
            Some([number(w)?, number(h)?]).filter(|[w, h]| *w > 0 && *h > 0)
        }
        _ => None,
    }
}

/// Plays `file`'s sound from `from` for `length` seconds of the video at
/// `volume` percent and `speed` times as fast, in an ffplay with no window.
fn sound(file: &str, from: f64, length: f64, volume: f32, speed: f32) -> std::io::Result<Child> {
    // atempo goes no slower than half speed, so a quarter takes two of them.
    let tempo = if speed < 0.5 {
        format!("atempo=0.5,atempo={:.3}", speed / 0.5)
    } else {
        format!("atempo={speed:.3}")
    };
    Command::new(program("ffplay"))
        .args(["-nodisp", "-autoexit", "-loglevel", "quiet"])
        .args([
            "-ss",
            &format!("{:.3}", from.max(0.0)),
            "-t",
            &format!("{length:.3}"),
        ])
        .args(["-af", &format!("{tempo},volume={:.2}", volume / 100.0)])
        .args(["-i", file])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(crate::NO_WINDOW | BELOW_NORMAL)
        .spawn()
}

/// Ends a program and waits for it to go, so nothing is left running.
fn end(child: &mut Child) {
    drop(child.kill());
    drop(child.wait());
}

#[cfg(test)]
mod tests {
    use super::{Player, Prefs, header, of, placed};

    /// A PPM header gives the frame's size and leaves the reader on its
    /// first pixel, and anything else gives nothing.
    #[test]
    fn a_frame_header_gives_its_size() {
        let mut read = &b"P6\n360 640\n255\nRGB"[..];
        assert_eq!(header(&mut read), Some([360, 640]));
        assert_eq!(read, b"RGB");
        assert_eq!(header(&mut &b"P5\n2 2\n255\n"[..]), None);
        assert_eq!(header(&mut &b"P6\n0 2\n255\n"[..]), None);
        assert_eq!(header(&mut &b"P6\n2"[..]), None);
    }

    /// Pausing keeps the place, and playing again goes on from it, but
    /// from the start once it is outside the part or at its end.
    #[test]
    fn play_after_a_pause_goes_on_from_where_it_was() {
        let mut player = Player::new("nothing.mp4", Prefs::default());
        player.span = (2.0, 5.0);
        player.gain = 0.0;
        let resumes = |player: &mut Player, at: f64| {
            player.at = at;
            player.toggle();
            let from = player.run.as_ref().map(|r| r.from);
            player.stop();
            from
        };
        // ffmpeg may be missing, in which case there is no run to look at.
        if let Some(from) = resumes(&mut player, 3.5) {
            assert!(
                (from - 3.5).abs() < 1e-9,
                "paused at 3.5 it went on from {from}"
            );
            assert_eq!(
                resumes(&mut player, 5.0),
                Some(2.0),
                "the end goes back to the start"
            );
            assert_eq!(
                resumes(&mut player, 0.5),
                Some(2.0),
                "before the part starts it"
            );
        }
    }

    /// Windows' engine puts the video where egui drew its box, in the
    /// window's pixels and cut to the part that can be seen, and hides it
    /// while it is scrolled away or a menu is over it.
    #[test]
    fn the_engines_video_follows_its_box_and_gives_way() {
        let ctx = egui::Context::default();
        let rect = egui::Rect::from_min_max(egui::pos2(10.0, 20.0), egui::pos2(110.0, 220.0));
        let seen = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(500.0, 100.0));
        let frame = |menu: bool| {
            let mut out = None;
            let mut drawn = ctx.run_ui(egui::RawInput::default(), |ui| {
                if menu {
                    let _menu = egui::Area::new(egui::Id::new("menu"))
                        .fixed_pos(egui::pos2(40.0, 40.0))
                        .show(ui.ctx(), |ui| {
                            ui.allocate_exact_size(egui::vec2(40.0, 40.0), egui::Sense::click())
                        });
                }
                out = placed(ui.ctx(), rect, seen, ui.layer_id());
            });
            drawn.textures_delta.clear();
            out
        };
        assert_eq!(frame(false), Some(([10, 20, 110, 220], [10, 20, 110, 100])));
        // A menu is known from the frame it was drawn in.
        let _first = frame(true);
        assert_eq!(frame(true), None);
        // And it goes from the next frame on.
        let _closed = frame(false);
        assert!(frame(false).is_some(), "back once the menu has gone");
        let away = egui::Rect::from_min_max(egui::pos2(0.0, 300.0), egui::pos2(500.0, 400.0));
        let gone = ctx.run_ui(egui::RawInput::default(), |ui| {
            assert_eq!(placed(ui.ctx(), rect, away, ui.layer_id()), None);
        });
        drop(gone);
    }

    /// While a clip is cut its volume bar sits where the clip's slider is,
    /// out of 200, and moves when the slider does, and only then.
    #[test]
    fn the_bar_follows_the_clip_volume() {
        let mut player = Player::new("nothing.mp4", Prefs::default());
        player.follow(60.0, 0.0);
        assert_eq!((player.prefs.volume, player.top), (60.0, 200.0));
        player.prefs.volume = 150.0;
        player.follow(60.0, 1.0);
        assert!(
            (player.prefs.volume - 150.0).abs() < f32::EPSILON,
            "a slider that didn't move leaves the bar"
        );
        player.follow(80.0, 2.0);
        assert!((player.prefs.volume - 80.0).abs() < f32::EPSILON);
    }

    /// Steps the harness until `done`, or until three real seconds have gone
    /// for ffmpeg to do its part.
    fn until(shot: &mut egui_kittest::Harness<'_, Option<Player>>, done: impl Fn(&Player) -> bool) {
        let began = std::time::Instant::now();
        while began.elapsed() < std::time::Duration::from_secs(3) {
            shot.step();
            if shot.state().as_ref().is_some_and(&done) {
                return;
            }
        }
    }

    /// A real video, made with ffmpeg's test pattern, which draws the time
    /// on every frame: a still shows the second asked for in the video's
    /// own shape, and playing moves on from where it starts. With
    /// `STRESS_OUT` set the frames are saved there to look at.
    /// A program winget doesn't have is left to PATH by its bare name.
    #[test]
    fn a_program_winget_lacks_is_found_on_path() {
        assert_eq!(
            super::program("no-such-tool"),
            std::path::PathBuf::from("no-such-tool")
        );
    }

    #[test]
    #[ignore = "needs ffmpeg"]
    fn a_video_shows_the_frame_asked_for_and_plays_on() {
        let folder = std::env::temp_dir().join("overseer-player-test");
        std::fs::create_dir_all(&folder).unwrap();
        let file = folder.join("short.mp4");
        let made = std::process::Command::new("ffmpeg")
            .args(["-y", "-v", "error", "-f", "lavfi", "-i"])
            .arg("testsrc=duration=4:size=720x1280:rate=30")
            .args(["-pix_fmt", "yuv420p"])
            .arg(&file)
            .status()
            .unwrap();
        assert!(made.success());
        let shown = file.to_string_lossy().into_owned();
        let still = Prefs {
            autoplay: false,
            looping: false,
            ..Prefs::default()
        };
        let mut shot = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1000.0, 800.0))
            .with_step_dt(1.0 / 60.0)
            .build_ui_state(
                move |ui, slot: &mut Option<Player>| {
                    ui.set_max_width(380.0);
                    of(slot, &shown, 1.0, still).show(ui, (0.0, 4.0), 0.0, 0.0);
                },
                None,
            );
        until(&mut shot, |p| p.texture.is_some() && p.run.is_none());
        let player = shot.state().as_ref().unwrap();
        let size = player.texture.as_ref().expect("no frame came").size();
        let [wide, high] = size;
        assert!(
            high < 1280,
            "made the size it is drawn, not the source's: {size:?}"
        );
        let shape = wide as f32 / high as f32;
        assert!(
            (shape - 0.5625).abs() < 0.01,
            "a short keeps its shape: {size:?}"
        );
        assert!(
            (player.at - 1.0).abs() < 0.05,
            "the still is at {}",
            player.at
        );
        let out = std::env::var("STRESS_OUT").ok();
        if let Some(out) = &out {
            shot.render()
                .unwrap()
                .save(format!("{out}/player-still.png"))
                .unwrap();
            shot.state_mut().as_mut().unwrap().big = true;
            shot.step();
            shot.step();
            shot.render()
                .unwrap()
                .save(format!("{out}/player-big.png"))
                .unwrap();
            shot.state_mut().as_mut().unwrap().big = false;
            shot.step();
        }

        shot.state_mut().as_mut().unwrap().play(2.0, 3.5);
        until(&mut shot, |p| p.at > 2.4);
        let player = shot.state().as_ref().unwrap();
        assert!(player.playing(), "it stopped early");
        assert!((2.4..3.5).contains(&player.at), "it got to {}", player.at);
        if let Some(out) = &out {
            shot.render()
                .unwrap()
                .save(format!("{out}/player-playing.png"))
                .unwrap();
        }
        until(&mut shot, |p| !p.playing());
        assert!(
            !shot.state().as_ref().unwrap().playing(),
            "it played past the end"
        );
    }
}

//! The app's side of the video process. Windows' media engine runs in a
//! second copy of the exe started with `--video`, so the memory its drivers
//! take goes back when that process ends, on leaving Lineups. A [`Video`]
//! tells it what to do and keeps what it last said.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::windows::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Instant;

use crate::Area;

/// Windows' `CREATE_NO_WINDOW` and `BELOW_NORMAL_PRIORITY_CLASS`, so the
/// game gets the CPU before the video process does.
const FLAGS: u32 = 0x0800_0000 | 0x0000_4000;

/// The video process, while there is one.
static PROCESS: Mutex<Option<Process>> = Mutex::new(None);

/// The id the next clip gets.
static NEXT: AtomicU32 = AtomicU32::new(1);

/// The video process, the line it reads its orders from, and the clips it
/// plays.
struct Process {
    /// The process itself.
    child: Child,
    /// Sends it one order a line, through a thread of its own so a busy
    /// process never holds up the app's window.
    say: Sender<String>,
    /// What it last said about each clip, by id.
    clips: Clips,
}

/// What the video process last said about each clip, by id.
type Clips = Arc<Mutex<HashMap<u32, Arc<Slot>>>>;

/// One clip as the app knows it.
struct Slot {
    /// What the video process last said about it.
    heard: Mutex<Heard>,
    /// Asks the app's window for a frame.
    wake: Box<dyn Fn() + Send + Sync>,
}

/// What the video process last said about a clip.
#[derive(Debug)]
struct Heard {
    /// Where it was, in seconds.
    time: f64,
    /// When that was said, to count on from while it plays.
    since: Instant,
    /// Whether it was playing.
    playing: bool,
    /// How fast it plays.
    rate: f64,
    /// Its width and height, once known.
    size: Option<[u32; 2]>,
    /// It played to the end since the app last asked.
    ended: bool,
    /// It couldn't be played, or the video process has gone.
    failed: bool,
}

impl Heard {
    /// A clip nothing has been said about yet, paused at its start.
    fn new() -> Self {
        Self {
            time: 0.0,
            since: Instant::now(),
            playing: false,
            rate: 1.0,
            size: None,
            ended: false,
            failed: false,
        }
    }

    /// Where it is now, counted on from what was last said while it plays.
    fn now(&self) -> f64 {
        if self.playing {
            self.since
                .elapsed()
                .as_secs_f64()
                .mul_add(self.rate, self.time)
        } else {
            self.time
        }
    }

    /// Takes `time` as where it is from now on.
    fn at(&mut self, time: f64) {
        (self.time, self.since) = (time, Instant::now());
    }
}

/// Locks `mutex`, taking the value as it is when a thread panicked holding it.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A clip played by Windows' media engine, in the video process, in a child
/// of the app's window. Dropping it stops the clip and removes the window.
pub struct Video {
    /// Its id with the video process.
    id: u32,
    /// What the video process last said about it.
    slot: Arc<Slot>,
    /// Sends the video process its orders.
    say: Sender<String>,
    /// Every clip's slot, to take this one's out on drop.
    clips: Clips,
    /// Where it was last put, so an unchanged place isn't sent again.
    placed: Option<(Area, Area)>,
    /// Whether it has been put anywhere yet, or hidden.
    told: bool,
}

impl std::fmt::Debug for Video {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Video")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

impl Video {
    /// Opens `file`, paused at its first frame and hidden until placed,
    /// starting the video process if it isn't running. `wake` is called, from
    /// another thread, whenever there is news about the clip.
    ///
    /// # Errors
    ///
    /// Why it couldn't: no window was set, or the video process wouldn't
    /// start. A clip the process can't play says so later, through
    /// [`Video::failed`].
    pub fn open(file: &str, wake: impl Fn() + Send + Sync + 'static) -> Result<Self, String> {
        let parent = crate::window::window();
        if parent.is_invalid() {
            return Err("there is no window to play the video in".to_owned());
        }
        let (say, clips) = running(parent.0 as isize)?;
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let slot = Arc::new(Slot {
            heard: Mutex::new(Heard::new()),
            wake: Box::new(wake),
        });
        lock(&clips).insert(id, Arc::clone(&slot));
        let video = Self {
            id,
            slot,
            say,
            clips,
            placed: None,
            told: false,
        };
        video.order(&format!("open\t{file}"));
        Ok(video)
    }

    /// Sends the video process `what` for this clip.
    fn order(&self, what: &str) {
        let (kind, rest) = what.split_once('\t').unwrap_or((what, ""));
        let line = if rest.is_empty() {
            format!("{kind}\t{}", self.id)
        } else {
            format!("{kind}\t{}\t{rest}", self.id)
        };
        drop(self.say.send(line));
    }

    /// What the video process last said, to read or change.
    fn heard(&self) -> MutexGuard<'_, Heard> {
        lock(&self.slot.heard)
    }

    /// Plays on from where it is.
    pub fn play(&self) {
        self.order("play");
        let mut heard = self.heard();
        let now = heard.now();
        heard.at(now);
        (heard.playing, heard.ended) = (true, false);
    }

    /// Stops where it is, keeping that frame on screen.
    pub fn pause(&self) {
        self.order("pause");
        let mut heard = self.heard();
        let now = heard.now();
        heard.at(now);
        heard.playing = false;
    }

    /// Whether it is playing.
    #[must_use]
    pub fn playing(&self) -> bool {
        self.heard().playing
    }

    /// Where it has got to, in seconds.
    #[must_use]
    pub fn time(&self) -> f64 {
        self.heard().now()
    }

    /// Goes to `at` seconds, playing on from there if it was playing.
    pub fn seek(&self, at: f64) {
        self.order(&format!("seek\t{at:.3}"));
        self.heard().at(at.max(0.0));
    }

    /// Goes to `at` as [`Video::seek`] does, unless the last seek is still
    /// going, so following a dragged handle doesn't queue a seek a frame.
    pub fn scrub(&self, at: f64) {
        self.order(&format!("scrub\t{at:.3}"));
        self.heard().at(at.max(0.0));
    }

    /// Plays `rate` times as fast as recorded, the sound kept at its pitch.
    pub fn set_rate(&self, rate: f64) {
        self.order(&format!("rate\t{rate}"));
        let mut heard = self.heard();
        let now = heard.now();
        heard.at(now);
        heard.rate = rate;
    }

    /// How loud, from 0 for silent to 1 for as recorded.
    pub fn set_volume(&self, share: f64) {
        self.order(&format!("volume\t{}", share.clamp(0.0, 1.0)));
    }

    /// The video's width and height, once the engine has read them.
    #[must_use]
    pub fn size(&self) -> Option<[u32; 2]> {
        self.heard().size
    }

    /// Whether it played to the end since this was last asked.
    #[must_use]
    pub fn take_ended(&self) -> bool {
        std::mem::take(&mut self.heard().ended)
    }

    /// Whether the engine couldn't read or play the file, or the video
    /// process has gone.
    #[must_use]
    pub fn failed(&self) -> bool {
        self.heard().failed
    }

    /// Puts the video at `shown`'s first box in the window's client area,
    /// showing only the part of it inside the second, or hides it. The video
    /// process waits for a frame before it shows the window.
    pub fn place(&mut self, shown: Option<(Area, Area)>) {
        if self.told && self.placed == shown {
            return;
        }
        (self.placed, self.told) = (shown, true);
        match shown {
            Some((a, s)) => self.order(&format!(
                "place\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                a[0], a[1], a[2], a[3], s[0], s[1], s[2], s[3]
            )),
            None => self.order("hide"),
        }
    }
}

impl Drop for Video {
    fn drop(&mut self) {
        self.order("close");
        lock(&self.clips).remove(&self.id);
    }
}

/// The video process for the window `parent`, started when it isn't
/// running: what sends it orders, and its clips.
fn running(parent: isize) -> Result<(Sender<String>, Clips), String> {
    let mut process = lock(&PROCESS);
    if process
        .as_mut()
        .is_none_or(|p| !matches!(p.child.try_wait(), Ok(None)))
    {
        *process = Some(start(parent)?);
    }
    let found = process
        .as_ref()
        .map(|p| (p.say.clone(), Arc::clone(&p.clips)));
    drop(process);
    found.ok_or_else(|| "the video process didn't start".to_owned())
}

/// Starts the video process for the window `parent`, with a thread that
/// sends its orders and one that reads what it says.
fn start(parent: isize) -> Result<Process, String> {
    let exe = std::env::current_exe().map_err(|e| format!("can't find Overseer's exe: {e}"))?;
    let mut child = Command::new(exe)
        .args(["--video", &parent.to_string()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(FLAGS)
        .spawn()
        .map_err(|e| format!("the video process didn't start: {e}"))?;
    let (Some(mut input), Some(output)) = (child.stdin.take(), child.stdout.take()) else {
        drop(child.kill());
        return Err("the video process has no input or output".to_owned());
    };
    let (say, orders) = channel::<String>();
    drop(std::thread::spawn(move || {
        for line in orders {
            if writeln!(input, "{line}")
                .and_then(|()| input.flush())
                .is_err()
            {
                break;
            }
        }
    }));
    let clips: Clips = Arc::default();
    let heard = Arc::clone(&clips);
    drop(std::thread::spawn(move || {
        for line in BufReader::new(output).lines() {
            let Ok(line) = line else { break };
            hear(&heard, &line);
        }
        // Gone, so every clip it had fails and the app falls back to ffmpeg.
        for slot in lock(&heard).values() {
            lock(&slot.heard).failed = true;
            (slot.wake)();
        }
    }));
    Ok(Process { child, say, clips })
}

/// Takes one line the video process said: a clip's `state`, `size`, `ended`
/// or `failed`, with its id and then what it says, split by tabs.
fn hear(clips: &Clips, line: &str) {
    let mut parts = line.split('\t');
    let (Some(kind), Some(id)) = (
        parts.next(),
        parts.next().and_then(|i| i.parse::<u32>().ok()),
    ) else {
        return;
    };
    let Some(slot) = lock(clips).get(&id).cloned() else {
        return;
    };
    let rest: Vec<&str> = parts.collect();
    let mut heard = lock(&slot.heard);
    let news = match (kind, rest.as_slice()) {
        ("state", [time, playing]) => {
            if let Ok(time) = time.parse() {
                heard.at(time);
            }
            heard.playing = *playing == "1";
            false
        }
        ("size", [wide, tall]) => {
            heard.size = wide.parse().ok().zip(tall.parse().ok()).map(Into::into);
            true
        }
        ("ended", []) => {
            (heard.ended, heard.playing) = (true, false);
            true
        }
        ("failed", []) => {
            heard.failed = true;
            true
        }
        _ => false,
    };
    drop(heard);
    if news {
        (slot.wake)();
    }
}

/// Ends the video process, giving back everything Windows' media engine and
/// its drivers took, since Windows frees all a process held when it ends. The
/// next clip starts it again, which takes a moment.
pub fn let_go() {
    let process = lock(&PROCESS).take();
    if let Some(mut process) = process {
        drop(process.child.kill());
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::{Arc, Mutex};

    use super::{Clips, Heard, Slot, hear, lock};

    /// What the video process says lands on the clip it names, a playing
    /// clip's time counts on from what was said, and only news about its
    /// size, its end or its failure asks the window for a frame.
    #[test]
    fn what_the_video_process_says_is_kept() {
        let clips: Clips = Arc::default();
        let woken = Arc::new(AtomicU32::new(0));
        let counted = Arc::clone(&woken);
        let slot = Arc::new(Slot {
            heard: Mutex::new(Heard::new()),
            wake: Box::new(move || {
                counted.fetch_add(1, Ordering::Relaxed);
            }),
        });
        lock(&clips).insert(7, Arc::clone(&slot));
        hear(&clips, "size	7	1080	1920");
        hear(&clips, "state	7	2.500	1");
        hear(&clips, "state	8	9.000	0");
        hear(&clips, "not a line");
        let heard = lock(&slot.heard);
        assert_eq!(heard.size, Some([1080, 1920]));
        assert!(heard.playing && heard.now() >= 2.5, "{heard:?}");
        drop(heard);
        hear(&clips, "ended	7");
        let heard = lock(&slot.heard);
        assert!(heard.ended && !heard.playing, "{heard:?}");
        drop(heard);
        assert_eq!(woken.load(Ordering::Relaxed), 2);
    }
}

//! The video process: Overseer's own exe started with `--video` and the app's
//! window. It plays clips in child windows of that window for as long as the
//! app keeps its input open, one line per thing to do, and says on its
//! output what each clip is doing. When the app closes its input, or ends,
//! this ends too, and everything Windows' media engine loaded goes with it.

use std::collections::HashMap;
use std::ffi::c_void;
use std::io::{BufRead, Write};
use std::sync::mpsc::{TryRecvError, channel};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::Media::MediaFoundation::{MF_VERSION, MFSTARTUP_LITE, MFStartup};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
use windows::Win32::System::Threading::{GetCurrentThreadId, INFINITE};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, MSG, MsgWaitForMultipleObjects, PM_NOREMOVE, PM_REMOVE, PeekMessageW,
    PostThreadMessageW, QS_ALLINPUT, TranslateMessage, WM_APP,
};

use crate::Area;
use crate::video::Clip;

/// How often a playing clip says where it has got to. The app works out the
/// time in between from the speed.
const REPORT: Duration = Duration::from_millis(100);

/// How often the loop looks again while a clip plays or a window settles.
const BUSY: u32 = 50;

/// One clip and what the app last asked of it.
struct Shown {
    /// The clip itself.
    clip: Clip,
    /// Where the app wants it, or nothing for hidden.
    wanted: Option<(Area, Area)>,
    /// Whether its size has been said.
    sized: bool,
    /// Whether its failure has been said.
    failed: bool,
    /// When it last said where it had got to.
    reported: Option<Instant>,
    /// Whether its window is still clipped to nothing after being shown.
    settling: bool,
}

/// Plays clips in children of the app's window `parent` until the app closes
/// this process's input, and returns the exit code.
#[must_use]
pub fn serve(parent: isize) -> i32 {
    // SAFETY: the documented start of a process that uses COM, Media
    // Foundation and windows, on its main thread, before anything else. The
    // DPI context has to match the app's, or a window placed in its pixels
    // lands somewhere else.
    let thread = unsafe {
        let _aware = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let _apartment = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        if MFStartup(MF_VERSION, MFSTARTUP_LITE).is_err() {
            return 1;
        }
        // Makes the thread's message queue, so a post to it can't be lost.
        let mut message = MSG::default();
        let _peeked = PeekMessageW(&raw mut message, None, 0, 0, PM_NOREMOVE);
        GetCurrentThreadId()
    };
    let wake = move || {
        // SAFETY: a post to this process's main thread, whose queue is made
        // above. Nothing is passed with it.
        let _posted = unsafe { PostThreadMessageW(thread, WM_APP, WPARAM(0), LPARAM(0)) };
    };
    let (heard, orders) = channel::<String>();
    drop(std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if heard.send(line).is_err() {
                break;
            }
            wake();
        }
        drop(heard);
        wake();
    }));
    let parent = HWND(parent as *mut c_void);
    let mut out = std::io::stdout().lock();
    let mut clips: HashMap<u32, Shown> = HashMap::new();
    loop {
        let busy = clips.values().any(|c| c.settling || c.clip.playing());
        // SAFETY: waits on this thread's own queue, then hands each of its
        // messages to the window it is for, the documented message loop.
        unsafe {
            let _woke = MsgWaitForMultipleObjects(
                None,
                false,
                if busy { BUSY } else { INFINITE },
                QS_ALLINPUT,
            );
            let mut message = MSG::default();
            while PeekMessageW(&raw mut message, None, 0, 0, PM_REMOVE).as_bool() {
                let _translated = TranslateMessage(&raw const message);
                DispatchMessageW(&raw const message);
            }
        }
        loop {
            match orders.try_recv() {
                Ok(line) => obey(&line, &mut clips, parent, wake, &mut out),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return 0,
            }
        }
        for (id, shown) in &mut clips {
            report(*id, shown, &mut out);
        }
        if out.flush().is_err() {
            return 0;
        }
    }
}

/// Does one line the app sent: `open`, `place`, `hide`, `close`, `play`,
/// `pause`, `seek`, `scrub`, `rate` or `volume`, each with the clip's id and
/// then what it needs, all split by tabs.
fn obey(
    line: &str,
    clips: &mut HashMap<u32, Shown>,
    parent: HWND,
    wake: impl Fn() + Send + Sync + 'static,
    out: &mut impl Write,
) {
    let mut parts = line.split('\t');
    let (Some(kind), Some(id)) = (
        parts.next(),
        parts.next().and_then(|i| i.parse::<u32>().ok()),
    ) else {
        return;
    };
    if kind == "open" {
        let file = parts.collect::<Vec<_>>().join("\t");
        match Clip::open(parent, &file, wake) {
            Ok(clip) => {
                let shown = Shown {
                    clip,
                    wanted: None,
                    sized: false,
                    failed: false,
                    reported: None,
                    settling: false,
                };
                clips.insert(id, shown);
            }
            Err(_) => drop(writeln!(out, "failed\t{id}")),
        }
        return;
    }
    if kind == "close" {
        clips.remove(&id);
        return;
    }
    let Some(shown) = clips.get_mut(&id) else {
        return;
    };
    let numbers: Vec<f64> = parts.filter_map(|p| p.parse().ok()).collect();
    let clip = &shown.clip;
    match (kind, numbers.as_slice()) {
        ("play", []) => clip.play(),
        ("pause", []) => clip.pause(),
        ("seek", [at]) => clip.seek(*at),
        ("scrub", [at]) => clip.scrub(*at),
        ("rate", [rate]) => clip.set_rate(*rate),
        ("volume", [share]) => clip.set_volume(*share),
        ("place", [l, t, r, b, sl, st, sr, sb]) => {
            let area = |v: [f64; 4]| v.map(|n| n as i32);
            shown.wanted = Some((area([*l, *t, *r, *b]), area([*sl, *st, *sr, *sb])));
            shown.settling = shown.clip.place(shown.wanted);
            return;
        }
        ("hide", []) => {
            shown.wanted = None;
            shown.settling = shown.clip.place(None);
            return;
        }
        _ => return,
    }
    say_state(id, &shown.clip, out);
    shown.reported = Some(Instant::now());
}

/// Says what is new about one clip: its size once it is known, its end, its
/// failure, and where a playing one has got to. Places its window again too,
/// since it waits for the engine's first frame and then settles.
fn report(id: u32, shown: &mut Shown, out: &mut impl Write) {
    if !shown.sized
        && let Some([wide, tall]) = shown.clip.size()
    {
        shown.sized = true;
        drop(writeln!(out, "size\t{id}\t{wide}\t{tall}"));
    }
    if shown.clip.take_ended() {
        drop(writeln!(out, "ended\t{id}"));
    }
    if !shown.failed && shown.clip.failed() {
        shown.failed = true;
        drop(writeln!(out, "failed\t{id}"));
    }
    if shown.clip.playing() && shown.reported.is_none_or(|at| at.elapsed() >= REPORT) {
        say_state(id, &shown.clip, out);
        shown.reported = Some(Instant::now());
    }
    shown.settling = shown.clip.place(shown.wanted);
}

/// Says where a clip is and whether it is playing.
fn say_state(id: u32, clip: &Clip, out: &mut impl Write) {
    drop(writeln!(
        out,
        "state\t{id}\t{:.3}\t{}",
        clip.time(),
        u8::from(clip.playing())
    ));
}

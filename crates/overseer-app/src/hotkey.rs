//! One key combination the whole machine listens for.
//!
//! The overlay takes no clicks, on purpose, so there has to be a way to turn
//! it off that does not involve finding a window that is deliberately not
//! interactive. Ctrl+Alt+O is that way, and it works with VALORANT in the
//! foreground, which is the only time it matters.
//!
//! This is `RegisterHotKey`, the ordinary Windows API every screenshot tool
//! and voice chat uses. Nothing is injected into anything, no input is sent
//! anywhere, and nothing reads another process: the app asks Windows to tell
//! it when a key is pressed, and Windows tells it.
//!
//! Registration fails if something else already owns the combination, and
//! that is reported rather than swallowed. A hotkey that silently does not
//! work is worse than one that says why.

use std::sync::mpsc::{Receiver, TryIter, channel};

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

/// What the combination is called, for the settings screen to print.
pub(crate) const LABEL: &str = "Ctrl+Alt+O";

/// A registered hotkey, alive for as long as this is.
///
/// Dropping it unregisters, which is why the manager is kept rather than
/// leaked: an app that leaves a global hotkey behind after it exits is an
/// app that has taken a key combination off the machine.
pub(crate) struct Hotkey {
    /// Holds the registration open. Never read.
    _manager: GlobalHotKeyManager,
    /// Presses, put here by the handler Windows calls.
    presses: Receiver<()>,
}

impl std::fmt::Debug for Hotkey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Hotkey").field("label", &LABEL).finish()
    }
}

impl Hotkey {
    /// Every press since the last look.
    pub(crate) fn presses(&self) -> TryIter<'_, ()> {
        self.presses.try_iter()
    }
}

/// Registers the hotkey, and wakes the window when it fires.
///
/// The waker is the same trick the bridge uses: egui is asleep whenever
/// nothing is happening, so whatever wants a frame has to ask for one.
///
/// Must be called from the thread running the message loop. Windows delivers
/// `WM_HOTKEY` to the queue of the thread that created the listening window,
/// and on any other thread nothing would ever arrive.
pub(crate) fn start(wake: impl Fn() + Send + Sync + 'static) -> Result<Hotkey, String> {
    let manager = GlobalHotKeyManager::new().map_err(|why| why.to_string())?;
    let combination = HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyO);
    manager
        .register(combination)
        .map_err(|why| format!("{LABEL} is taken: {why}"))?;

    let (tx, presses) = channel();
    GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
        // Both halves of the press arrive. Acting on the release as well
        // would toggle the overlay on and straight back off again.
        if event.state != HotKeyState::Pressed {
            return;
        }
        if tx.send(()).is_ok() {
            wake();
        }
    }));
    Ok(Hotkey {
        _manager: manager,
        presses,
    })
}

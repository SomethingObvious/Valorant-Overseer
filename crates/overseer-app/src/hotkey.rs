//! The overlay's key, Ctrl+Alt+O unless another is chosen, registered for
//! the whole machine.
//!
//! This is how the overlay goes on and off while VALORANT has the
//! foreground. It is plain `RegisterHotKey`, the API every
//! screenshot tool uses: nothing is injected, no input is sent, and no other
//! process is read.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc::{Receiver, TryIter, channel};

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use serde::{Deserialize, Serialize};

/// The combinations the overlay can go on. Each holds Ctrl, so none is a key
/// VALORANT itself uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Key {
    /// The one it starts on.
    #[default]
    CtrlAltO,
    /// For when another program has Ctrl+Alt+O.
    CtrlShiftO,
    /// Under the left hand, beside the movement keys.
    CtrlAltV,
    /// Out of the way of anything typed.
    CtrlAltF8,
}

impl Key {
    /// Every combination, for the settings screen.
    pub(crate) const ALL: [Self; 4] = [
        Self::CtrlAltO,
        Self::CtrlShiftO,
        Self::CtrlAltV,
        Self::CtrlAltF8,
    ];

    /// What it is called on screen.
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::CtrlAltO => "Ctrl+Alt+O",
            Self::CtrlShiftO => "Ctrl+Shift+O",
            Self::CtrlAltV => "Ctrl+Alt+V",
            Self::CtrlAltF8 => "Ctrl+Alt+F8",
        }
    }

    /// What Windows is asked to register.
    const fn hotkey(self) -> (Modifiers, Code) {
        match self {
            Self::CtrlAltO => (Modifiers::CONTROL.union(Modifiers::ALT), Code::KeyO),
            Self::CtrlShiftO => (Modifiers::CONTROL.union(Modifiers::SHIFT), Code::KeyO),
            Self::CtrlAltV => (Modifiers::CONTROL.union(Modifiers::ALT), Code::KeyV),
            Self::CtrlAltF8 => (Modifiers::CONTROL.union(Modifiers::ALT), Code::F8),
        }
    }
}

/// Which of [`Key::ALL`] was registered last. There is one overlay key a
/// process, so the hints that print it read it from here.
static CHOSEN: AtomicU8 = AtomicU8::new(0);

/// What the combination in use is called.
pub(crate) fn label() -> &'static str {
    Key::ALL
        .get(usize::from(CHOSEN.load(Ordering::Relaxed)))
        .copied()
        .unwrap_or_default()
        .label()
}

/// A registered hotkey. Dropping it gives the combination back to Windows.
pub(crate) struct Hotkey {
    /// Holds the registration open. Never read.
    _registration: GlobalHotKeyManager,
    /// Presses, put here by the handler Windows calls.
    presses: Receiver<()>,
}

impl std::fmt::Debug for Hotkey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Hotkey").field("label", &label()).finish()
    }
}

impl Hotkey {
    /// Every press since the last look.
    pub(crate) fn presses(&self) -> TryIter<'_, ()> {
        self.presses.try_iter()
    }
}

/// Why there is no hotkey. Only one of these is another program's doing,
/// and the settings screen says which.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Failure {
    /// Another program registered the combination first.
    Taken,
    /// Windows said no for some other reason, such as not making the hidden
    /// window a registration needs.
    Refused(String),
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Taken => write!(f, "{} is taken", label()),
            Self::Refused(why) => f.write_str(why),
        }
    }
}

impl From<global_hotkey::Error> for Failure {
    fn from(why: global_hotkey::Error) -> Self {
        match why {
            global_hotkey::Error::AlreadyRegistered(_) => Self::Taken,
            other => Self::Refused(other.to_string()),
        }
    }
}

/// Registers the hotkey and calls `wake` on each press, or says why it could
/// not. Call it on the message loop's thread, because Windows posts
/// `WM_HOTKEY` to the thread that registered it.
pub(crate) fn start(key: Key, wake: impl Fn() + Send + Sync + 'static) -> Result<Hotkey, Failure> {
    CHOSEN.store(key as u8, Ordering::Relaxed);
    let manager = GlobalHotKeyManager::new()?;
    let (held, code) = key.hotkey();
    manager.register(HotKey::new(Some(held), code))?;

    let (tx, presses) = channel();
    GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
        // The release arrives too, and acting on it would toggle straight
        // back.
        if event.state != HotKeyState::Pressed {
            return;
        }
        if tx.send(()).is_ok() {
            wake();
        }
    }));
    Ok(Hotkey {
        _registration: manager,
        presses,
    })
}

//! Ctrl+Alt+O, registered for the whole machine.
//!
//! The overlay takes no clicks, so this is how it goes on and off while
//! VALORANT has the foreground. It is plain `RegisterHotKey`, the API every
//! screenshot tool uses: nothing is injected, no input is sent, and no other
//! process is read.

use std::sync::mpsc::{Receiver, TryIter, channel};

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

/// What the combination is called, for the settings screen to print.
pub(crate) const LABEL: &str = "Ctrl+Alt+O";

/// A registered hotkey. Dropping it gives the combination back to Windows.
pub(crate) struct Hotkey {
    /// Holds the registration open. Never read.
    _registration: GlobalHotKeyManager,
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
            Self::Taken => write!(f, "{LABEL} is taken"),
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
pub(crate) fn start(wake: impl Fn() + Send + Sync + 'static) -> Result<Hotkey, Failure> {
    let manager = GlobalHotKeyManager::new()?;
    manager.register(HotKey::new(
        Some(Modifiers::CONTROL | Modifiers::ALT),
        Code::KeyO,
    ))?;

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

#[cfg(test)]
mod tests {
    use global_hotkey::Error;
    use global_hotkey::hotkey::{Code, HotKey, Modifiers};

    use super::Failure;

    /// Only a combination somebody else registered is taken. Anything else
    /// Windows refuses is not the other program's fault.
    #[test]
    fn only_a_registered_combination_is_taken() {
        let combination = HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyO);
        assert_eq!(
            Failure::from(Error::AlreadyRegistered(combination)),
            Failure::Taken
        );
        assert_eq!(
            Failure::from(Error::OsError(std::io::Error::other("no window"))),
            Failure::Refused("no window".to_owned())
        );
    }
}

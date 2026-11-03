//! The overlay's button, Ctrl+Alt+O unless another is chosen. Any key or
//! mouse button but the left one can be, with Ctrl, Alt or Shift or without.
//!
//! This is how the overlay goes on and off while VALORANT has the
//! foreground. The presses come from Windows' raw input, which reads them
//! without taking them, so a key chosen here still does what it does in the
//! game. Nothing is injected, no input is sent, and no other process is read.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Mutex, PoisonError};

use overseer_native::{Buttons, Press};
use serde::{Deserialize, Serialize};

/// A key or mouse button, and the Ctrl, Alt and Shift held with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "Saved")]
pub(crate) struct Binding {
    /// Its virtual-key code, which the mouse buttons have too.
    pub(crate) key: u16,
    /// Whether Ctrl is part of it.
    pub(crate) ctrl: bool,
    /// Whether Alt is part of it.
    pub(crate) alt: bool,
    /// Whether Shift is part of it.
    pub(crate) shift: bool,
}

impl Binding {
    /// The one it starts on, Ctrl+Alt+O.
    pub(crate) const DEFAULT: Self = Self {
        key: 0x4F,
        ctrl: true,
        alt: true,
        shift: false,
    };

    /// Whether it is a mouse button, which needs the mouse read.
    const fn mouse(self) -> bool {
        matches!(self.key, 1 | 2 | 4 | 5 | 6)
    }

    /// Whether `press` is this one, with at least its Ctrl, Alt and Shift
    /// held. More is fine, since Shift walks and Ctrl crouches in VALORANT.
    const fn matches(self, press: Press) -> bool {
        press.key == self.key
            && (press.held.ctrl || !self.ctrl)
            && (press.held.alt || !self.alt)
            && (press.held.shift || !self.shift)
    }

    /// What it is called on screen, like Ctrl+Alt+O or Mouse 4.
    pub(crate) fn label(self) -> String {
        let mut out = String::new();
        for (held, name) in [
            (self.ctrl, "Ctrl+"),
            (self.alt, "Alt+"),
            (self.shift, "Shift+"),
        ] {
            if held {
                out.push_str(name);
            }
        }
        out + &key_name(self.key)
    }
}

impl Default for Binding {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// What the settings file holds: a binding, or one of the four named keys
/// from before any button could be picked.
#[derive(Deserialize)]
#[serde(untagged)]
enum Saved {
    /// A binding as it is written now.
    Bound {
        /// [`Binding::key`].
        key: u16,
        /// [`Binding::ctrl`].
        #[serde(default)]
        ctrl: bool,
        /// [`Binding::alt`].
        #[serde(default)]
        alt: bool,
        /// [`Binding::shift`].
        #[serde(default)]
        shift: bool,
    },
    /// One of the old four, like `ctrl-shift-o`.
    Named(String),
}

impl From<Saved> for Binding {
    fn from(saved: Saved) -> Self {
        match saved {
            Saved::Bound {
                key,
                ctrl,
                alt,
                shift,
            } => Self {
                key,
                ctrl,
                alt,
                shift,
            },
            Saved::Named(name) => match name.as_str() {
                "ctrl-shift-o" => Self {
                    alt: false,
                    shift: true,
                    ..Self::DEFAULT
                },
                "ctrl-alt-v" => Self {
                    key: 0x56,
                    ..Self::DEFAULT
                },
                "ctrl-alt-f8" => Self {
                    key: 0x77,
                    ..Self::DEFAULT
                },
                _ => Self::DEFAULT,
            },
        }
    }
}

/// What a key is called on screen, by its virtual-key code.
fn key_name(key: u16) -> String {
    let named = match key {
        1 => "Left Click",
        2 => "Right Click",
        4 => "Middle Click",
        5 => "Mouse 4",
        6 => "Mouse 5",
        0x08 => "Backspace",
        0x09 => "Tab",
        0x0D => "Enter",
        0x13 => "Pause",
        0x14 => "Caps Lock",
        0x1B => "Esc",
        0x20 => "Space",
        0x21 => "Page Up",
        0x22 => "Page Down",
        0x23 => "End",
        0x24 => "Home",
        0x25 => "Left",
        0x26 => "Up",
        0x27 => "Right",
        0x28 => "Down",
        0x2C => "Print Screen",
        0x2D => "Insert",
        0x2E => "Delete",
        0x5B | 0x5C => "Windows",
        0x5D => "Menu",
        0x6A => "Num *",
        0x6B => "Num +",
        0x6D => "Num -",
        0x6E => "Num .",
        0x6F => "Num /",
        0x90 => "Num Lock",
        0x91 => "Scroll Lock",
        0xAD => "Mute",
        0xAE => "Volume Down",
        0xAF => "Volume Up",
        0xB0 => "Next Track",
        0xB1 => "Previous Track",
        0xB3 => "Play Pause",
        0xBA => ";",
        0xBB => "=",
        0xBC => ",",
        0xBD => "-",
        0xBE => ".",
        0xBF => "/",
        0xC0 => "`",
        0xDB => "[",
        0xDC => "\\",
        0xDD => "]",
        0xDE => "'",
        _ => "",
    };
    if !named.is_empty() {
        return named.to_owned();
    }
    match key {
        0x30..=0x39 | 0x41..=0x5A => {
            u8::try_from(key).map_or_else(|_| String::new(), |k| char::from(k).to_string())
        }
        0x60..=0x69 => format!("Num {}", key - 0x60),
        0x70..=0x87 => format!("F{}", key - 0x6F),
        _ => format!("Key {key}"),
    }
}

/// The binding in use. There is one overlay button a process, so the hints
/// that print it and the thread that listens for it read it from here.
static CHOSEN: Mutex<Binding> = Mutex::new(Binding::DEFAULT);

/// Whether the next press becomes the binding, which Settings asks for.
static CATCHING: AtomicBool = AtomicBool::new(false);

/// The binding in use.
fn chosen() -> Binding {
    *CHOSEN.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What the binding in use is called.
pub(crate) fn label() -> String {
    chosen().label()
}

/// Whether Settings is waiting for a press to bind.
pub(crate) fn catching() -> bool {
    CATCHING.load(Ordering::Relaxed)
}

/// What the listening thread found since the last look.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Heard {
    /// Presses of the overlay's button.
    pub(crate) presses: usize,
    /// A press caught for Settings, which is the binding now.
    pub(crate) caught: Option<Binding>,
    /// Whether catching ended, caught or not.
    pub(crate) done: bool,
}

/// The thread listening for the overlay's button, and what it heard.
pub(crate) struct Listener {
    /// Starts and stops the mouse.
    buttons: Buttons,
    /// Presses that matter, put here by that thread.
    presses: Receiver<Press>,
}

impl std::fmt::Debug for Listener {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Listener").field("label", &label()).finish()
    }
}

impl Listener {
    /// Everything heard since the last look.
    pub(crate) fn heard(&self) -> Heard {
        let mut heard = Heard::default();
        for press in self.presses.try_iter() {
            if !catching() {
                heard.presses += 1;
                continue;
            }
            match press.key {
                0x1B => {
                    self.stop();
                    heard.done = true;
                }
                // The left button clicks Settings itself, and Ctrl, Alt and
                // Shift only count held with another key.
                1 | 0x10..=0x12 | 0xA0..=0xA5 => {}
                key => {
                    let caught = Binding {
                        key,
                        ctrl: press.held.ctrl,
                        alt: press.held.alt,
                        shift: press.held.shift,
                    };
                    self.choose(caught);
                    heard.caught = Some(caught);
                    heard.done = true;
                }
            }
        }
        heard
    }

    /// Makes the next press the binding.
    pub(crate) fn catch(&self) {
        CATCHING.store(true, Ordering::Relaxed);
        self.buttons.mouse(true);
    }

    /// Stops waiting for a press to bind.
    pub(crate) fn stop(&self) {
        CATCHING.store(false, Ordering::Relaxed);
        self.buttons.mouse(chosen().mouse());
    }

    /// Listens for `binding` from now on.
    pub(crate) fn choose(&self, binding: Binding) {
        *CHOSEN.lock().unwrap_or_else(PoisonError::into_inner) = binding;
        self.stop();
    }
}

/// Listens for `binding` anywhere and calls `wake` on each press that
/// matters, or says why it can't.
pub(crate) fn start(
    binding: Binding,
    wake: impl Fn() + Send + Sync + 'static,
) -> Result<Listener, String> {
    *CHOSEN.lock().unwrap_or_else(PoisonError::into_inner) = binding;
    let (tx, presses) = channel();
    // Filtered on that thread, so a round of typing and walking doesn't
    // draw a frame a key. A press while Overseer is in front is the window's
    // own, and O already does this there.
    let buttons = overseer_native::listen(move |press| {
        let wanted = if catching() {
            press.ours
        } else {
            !press.ours && chosen().matches(press)
        };
        if wanted && tx.send(press).is_ok() {
            wake();
        }
    })?;
    buttons.mouse(binding.mouse());
    Ok(Listener { buttons, presses })
}

#[cfg(test)]
mod tests {
    use overseer_native::Held;

    use super::{Binding, Press};

    /// A file from before any button could be picked keeps its key.
    #[test]
    fn the_old_named_keys_still_read() {
        let read = |text: &str| serde_json::from_str::<Binding>(text).unwrap();
        assert_eq!(read(r#""ctrl-alt-o""#), Binding::DEFAULT);
        assert_eq!(read(r#""ctrl-shift-o""#).label(), "Ctrl+Shift+O");
        assert_eq!(read(r#""ctrl-alt-f8""#).label(), "Ctrl+Alt+F8");
        let side = Binding {
            key: 5,
            ctrl: false,
            alt: false,
            shift: false,
        };
        let written = serde_json::to_string(&side).unwrap();
        assert_eq!(read(&written), side);
        assert_eq!(side.label(), "Mouse 4");
    }

    /// Walking or crouching while pressing it still counts, and missing one
    /// of its own doesn't.
    #[test]
    fn extra_modifiers_still_match() {
        let press = |key, ctrl, alt, shift| Press {
            key,
            held: Held { ctrl, alt, shift },
            ours: false,
        };
        let side = Binding {
            key: 5,
            ctrl: false,
            alt: false,
            shift: false,
        };
        assert!(side.matches(press(5, true, false, true)));
        assert!(!side.matches(press(6, false, false, false)));
        assert!(Binding::DEFAULT.matches(press(0x4F, true, true, true)));
        assert!(!Binding::DEFAULT.matches(press(0x4F, true, false, false)));
    }
}

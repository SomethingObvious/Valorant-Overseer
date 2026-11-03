//! Keys and mouse buttons pressed anywhere, for the overlay's button.
//!
//! This is Windows' raw input, read and never taken, so the game still gets
//! every press, nothing is injected and no other process is touched. A
//! process has one raw input target per kind of device, so this replaces the
//! one winit makes for its device events, which egui doesn't read.

use std::ffi::c_void;
use std::sync::mpsc::{SyncSender, sync_channel};

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::{
    GetRawInputData, HRAWINPUT, RAWINPUT, RAWINPUTDEVICE, RAWINPUTDEVICE_FLAGS, RAWINPUTHEADER,
    RID_INPUT, RIDEV_INPUTSINK, RIDEV_REMOVE, RIM_TYPEKEYBOARD, RIM_TYPEMOUSE,
    RegisterRawInputDevices,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DispatchMessageW, GetForegroundWindow, GetMessageW, GetWindowThreadProcessId,
    MSG, PostThreadMessageW, WM_APP, WM_INPUT, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
};
use windows_core::{PCWSTR, w};

/// A key or mouse button going down. Holding a key down reports it once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Press {
    /// Its virtual-key code. The mouse buttons have theirs too, from 1 for
    /// the left button to 6 for the second side button.
    pub key: u16,
    /// What was held with it.
    pub held: Held,
    /// Whether a window of this process had the foreground.
    pub ours: bool,
}

/// Ctrl, Alt and Shift, either one of each.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Held {
    /// Whether Ctrl was held.
    pub ctrl: bool,
    /// Whether Alt was held.
    pub alt: bool,
    /// Whether Shift was held.
    pub shift: bool,
}

/// The thread reading the presses, which runs until the process ends.
#[derive(Debug, Clone, Copy)]
pub struct Buttons {
    /// Where [`Buttons::mouse`] posts to.
    thread: u32,
}

impl Buttons {
    /// Starts or stops reading the mouse. Every move of it is a message, up
    /// to 8,000 a second on a gaming mouse, so it is only read while a mouse
    /// button is wanted.
    pub fn mouse(self, on: bool) {
        // SAFETY: a post to a thread whose queue was made before `listen`
        // returned. Nothing is passed with it but a flag.
        let _posted =
            unsafe { PostThreadMessageW(self.thread, WM_APP, WPARAM(usize::from(on)), LPARAM(0)) };
    }
}

/// Starts reading the keyboard on a thread of its own and hands each press
/// to `sink` there.
///
/// # Errors
///
/// Windows' own reason when the window or the registration fails.
pub fn listen(sink: impl Fn(Press) + Send + 'static) -> Result<Buttons, String> {
    let (told, heard) = sync_channel(1);
    let _thread = std::thread::Builder::new()
        .name("buttons".to_owned())
        .spawn(move || run(&told, &sink))
        .map_err(|why| why.to_string())?;
    heard.recv().map_err(|why| why.to_string())?
}

/// The generic desktop page, and its mouse and keyboard.
const DESKTOP: u16 = 0x01;
const MOUSE: u16 = 0x02;
const KEYBOARD: u16 = 0x06;

/// The thread's loop: a hidden window for the presses to arrive at, and
/// `told` once it is listening.
fn run(told: &SyncSender<Result<Buttons, String>>, sink: &impl Fn(Press)) {
    // SAFETY: a hidden window of a system class on this thread, never shown,
    // with no parent, menu or creation data.
    let made = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            w!("STATIC"),
            PCWSTR::null(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            None,
            None,
        )
    };
    let window = match made {
        Ok(window) => window,
        Err(why) => {
            let _told = told.send(Err(why.message()));
            return;
        }
    };
    if let Err(why) = register(KEYBOARD, Some(window)) {
        let _told = told.send(Err(why));
        return;
    }
    // SAFETY: always safe to call.
    let thread = unsafe { GetCurrentThreadId() };
    let _told = told.send(Ok(Buttons { thread }));
    let mut held = [false; 256];
    let mut message = MSG::default();
    // SAFETY: the documented message loop, on the thread that owns the
    // window. A result of -1 is an error and ends it like 0 does.
    while unsafe { GetMessageW(&raw mut message, None, 0, 0) }.0 > 0 {
        if message.message == WM_APP && message.hwnd.is_invalid() {
            let on = message.wParam.0 != 0;
            let _done = register(MOUSE, on.then_some(window));
            continue;
        }
        if message.message == WM_INPUT
            && let Some(raw) = read(HRAWINPUT(message.lParam.0 as *mut c_void))
        {
            for key in pressed(&raw, &mut held) {
                sink(Press {
                    key,
                    held: Held {
                        ctrl: held[0x11],
                        alt: held[0x12],
                        shift: held[0x10],
                    },
                    ours: ours(),
                });
            }
        }
        // SAFETY: a message this loop just took. The default handling of
        // WM_INPUT frees what Windows kept for it.
        let _handled = unsafe { DispatchMessageW(&raw const message) };
    }
}

/// Sends `usage`'s presses to `window` whatever has the foreground, or stops
/// them with `None`.
fn register(usage: u16, window: Option<HWND>) -> Result<(), String> {
    let flags: RAWINPUTDEVICE_FLAGS = if window.is_some() {
        RIDEV_INPUTSINK
    } else {
        RIDEV_REMOVE
    };
    let device = RAWINPUTDEVICE {
        usUsagePage: DESKTOP,
        usUsage: usage,
        dwFlags: flags,
        hwndTarget: window.unwrap_or_default(),
    };
    let size = u32::try_from(size_of::<RAWINPUTDEVICE>()).unwrap_or_default();
    // SAFETY: one device described by a local that outlives the call.
    unsafe { RegisterRawInputDevices(&[device], size) }.map_err(|why| why.message())
}

/// The press behind a `WM_INPUT`.
fn read(handle: HRAWINPUT) -> Option<RAWINPUT> {
    let mut raw = RAWINPUT::default();
    let mut size = u32::try_from(size_of::<RAWINPUT>()).ok()?;
    let header = u32::try_from(size_of::<RAWINPUTHEADER>()).ok()?;
    // SAFETY: the buffer is a whole RAWINPUT, which a mouse or keyboard's
    // input always fits, and `size` says how big it is.
    let got = unsafe {
        GetRawInputData(
            handle,
            RID_INPUT,
            Some((&raw mut raw).cast()),
            &raw mut size,
            header,
        )
    };
    (got != u32::MAX && got > 0).then_some(raw)
}

/// The mouse's button flags for each button going down, by its virtual-key
/// code. Going up is the flag after it.
const MOUSE_DOWN: [(u16, u16); 5] = [(1, 0x01), (2, 0x04), (4, 0x10), (5, 0x40), (6, 0x100)];

/// What went down in `raw`, keeping `held` up to date so a key held down
/// only counts once.
fn pressed(raw: &RAWINPUT, held: &mut [bool; 256]) -> Vec<u16> {
    let mut down = Vec::new();
    let mut set = |key: u16, now: bool| {
        if let Some(was) = held.get_mut(usize::from(key)) {
            if now && !*was {
                down.push(key);
            }
            *was = now;
        }
    };
    if raw.header.dwType == RIM_TYPEKEYBOARD.0 {
        // SAFETY: the header says this is the keyboard's half.
        let keyboard = unsafe { raw.data.keyboard };
        // 255 is a key Windows made up to go with another, like the Shift
        // sent around a number pad key.
        if keyboard.VKey != 0xFF {
            set(keyboard.VKey, keyboard.Flags & 1 == 0);
        }
    } else if raw.header.dwType == RIM_TYPEMOUSE.0 {
        // SAFETY: the header says this is the mouse's half.
        let flags = unsafe { raw.data.mouse.Anonymous.Anonymous.usButtonFlags };
        for (key, flag) in MOUSE_DOWN {
            if flags & flag != 0 {
                set(key, true);
            } else if flags & (flag << 1) != 0 {
                set(key, false);
            }
        }
    }
    down
}

/// Whether this process has the foreground window.
fn ours() -> bool {
    // SAFETY: documented calls with a local that outlives them.
    unsafe {
        let mut owner = 0;
        let _thread = GetWindowThreadProcessId(GetForegroundWindow(), Some(&raw mut owner));
        owner == std::process::id()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use windows::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput,
        VIRTUAL_KEY,
    };

    /// F24 typed twice, with the second held down, arrives once each time
    /// even though this process isn't in front. It types a real key, which
    /// nothing uses, so it is left out of the gate.
    #[test]
    #[ignore = "types F24 on the desktop"]
    fn a_key_pressed_anywhere_arrives_once() {
        let (tx, rx) = std::sync::mpsc::channel();
        let _buttons = super::listen(move |press| {
            let _sent = tx.send(press);
        })
        .expect("listening");
        let key = |flags| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(0x87),
                    dwFlags: flags,
                    ..KEYBDINPUT::default()
                },
            },
        };
        let size = i32::try_from(size_of::<INPUT>()).unwrap_or_default();
        let down = key(KEYBD_EVENT_FLAGS(0));
        let up = key(KEYEVENTF_KEYUP);
        // SAFETY: inputs made just above, of the size given.
        let _sent = unsafe { SendInput(&[down, up, down, down, up], size) };
        let mut got = Vec::new();
        while let Ok(press) = rx.recv_timeout(Duration::from_secs(1)) {
            got.push(press.key);
        }
        assert_eq!(got, [0x87, 0x87]);
    }
}

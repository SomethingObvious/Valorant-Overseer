//! The overlay's window, kept from taking the focus, and whether the app's
//! own window can be seen at all.

use std::ffi::c_void;
use std::sync::atomic::{AtomicIsize, Ordering};

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONULL, MONITORINFO, MonitorFromWindow,
};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, GWL_EXSTYLE, GetClassNameW, GetForegroundWindow, GetWindowLongPtrW, GetWindowRect,
    GetWindowThreadProcessId, IsIconic, SetWindowLongPtrW, WS_EX_NOACTIVATE,
};
use windows_core::PCWSTR;

/// The app's window, as its handle, which every video plays inside.
static WINDOW: AtomicIsize = AtomicIsize::new(0);

/// Tells the crate which window videos play in, by its Win32 handle.
pub fn set_window(hwnd: isize) {
    WINDOW.store(hwnd, Ordering::Relaxed);
}

/// The app's window, or an invalid handle before [`set_window`].
pub(crate) fn window() -> HWND {
    HWND(WINDOW.load(Ordering::Relaxed) as *mut c_void)
}

/// Stops the top-level window titled `title` from taking the focus on a click.
///
/// Clicking the overlay used to make it the foreground window, which pulls
/// VALORANT out of full screen. Does nothing while there is no such window,
/// and nothing to one that already has the style.
pub fn never_activate(title: &str) {
    let wide: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
    // SAFETY: the title is null terminated and outlives the call, and the
    // handle is found just above and checked before its style is touched.
    unsafe {
        let Ok(window) = FindWindowW(None, PCWSTR(wide.as_ptr())) else {
            return;
        };
        if window.is_invalid() {
            return;
        }
        let style = GetWindowLongPtrW(window, GWL_EXSTYLE);
        let wanted = style | isize::try_from(WS_EX_NOACTIVATE.0).unwrap_or_default();
        if style != wanted {
            let _was = SetWindowLongPtrW(window, GWL_EXSTYLE, wanted);
        }
    }
}

/// Whether nobody can see the app's window.
///
/// That is when it is minimized, or when a window of another program covers
/// its whole monitor, the way VALORANT does when it is played full screen.
/// The desktop doesn't count, though it covers the monitor too.
#[must_use]
pub fn hidden() -> bool {
    let ours = window();
    if ours.is_invalid() {
        return false;
    }
    // SAFETY: documented calls given handles Windows just returned, checked
    // before use, and locals that outlive each call.
    unsafe {
        if IsIconic(ours).as_bool() {
            return true;
        }
        let front = GetForegroundWindow();
        let mut owner = 0;
        let _thread = GetWindowThreadProcessId(front, Some(&raw mut owner));
        if front.is_invalid() || owner == std::process::id() {
            return false;
        }
        let mut class = [0_u16; 16];
        let length = usize::try_from(GetClassNameW(front, &mut class)).unwrap_or_default();
        let name = String::from_utf16_lossy(class.get(..length).unwrap_or_default());
        if name == "Progman" || name == "WorkerW" {
            return false;
        }
        let monitor = MonitorFromWindow(ours, MONITOR_DEFAULTTONULL);
        if monitor.is_invalid() || MonitorFromWindow(front, MONITOR_DEFAULTTONULL) != monitor {
            return false;
        }
        let mut info = MONITORINFO {
            cbSize: u32::try_from(size_of::<MONITORINFO>()).unwrap_or_default(),
            ..MONITORINFO::default()
        };
        let mut covers = RECT::default();
        if !GetMonitorInfoW(monitor, &raw mut info).as_bool()
            || GetWindowRect(front, &raw mut covers).is_err()
        {
            return false;
        }
        let screen = info.rcMonitor;
        covers.left <= screen.left
            && covers.top <= screen.top
            && covers.right >= screen.right
            && covers.bottom >= screen.bottom
    }
}

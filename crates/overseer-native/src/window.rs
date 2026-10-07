//! The overlay's window, kept from taking the focus.

use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, GWL_EXSTYLE, GetWindowLongPtrW, SetWindowLongPtrW, WS_EX_NOACTIVATE,
};
use windows_core::PCWSTR;

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

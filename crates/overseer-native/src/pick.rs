//! Windows' own Open dialog, for choosing a lineup's pictures.

use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::Mutex;

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    CoTaskMemFree, CoUninitialize,
};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{
    FOS_ALLOWMULTISELECT, FOS_FILEMUSTEXIST, FileOpenDialog, IFileOpenDialog, SIGDN_FILESYSPATH,
};
use windows_core::w;

/// What the dialog chose and nobody has taken yet.
static CHOSEN: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// Opens the dialog over the app's window, on a thread of its own so the
/// window keeps drawing, and calls `wake` once something was chosen. The
/// paths wait in [`take_pictures`].
pub fn pick_pictures(wake: impl FnOnce() + Send + 'static) {
    let owner = crate::window::window().0 as isize;
    let _dialog = std::thread::spawn(move || {
        let picked = chosen(owner);
        if picked.is_empty() {
            return;
        }
        if let Ok(mut kept) = CHOSEN.lock() {
            kept.extend(picked);
        }
        wake();
    });
}

/// The pictures chosen since the last look.
#[must_use]
pub fn take_pictures() -> Vec<PathBuf> {
    CHOSEN
        .lock()
        .map(|mut kept| std::mem::take(&mut *kept))
        .unwrap_or_default()
}

/// Shows the dialog and waits for it. Cancelling it, or any failure, chooses
/// nothing.
fn chosen(owner: isize) -> Vec<PathBuf> {
    let owner = HWND(owner as *mut c_void);
    // SAFETY: COM is started on this thread before the dialog is made and
    // stopped after the last interface is dropped. The filter strings are
    // static, and each path the dialog hands back is copied before it is
    // freed with the allocator it came from.
    unsafe {
        let _apartment = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let picked = (|| -> windows_core::Result<Vec<PathBuf>> {
            let dialog: IFileOpenDialog =
                CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
            dialog.SetTitle(w!("Add Pictures"))?;
            dialog.SetFileTypes(&[COMDLG_FILTERSPEC {
                pszName: w!("Pictures"),
                pszSpec: w!("*.png;*.jpg;*.jpeg;*.webp;*.bmp;*.gif"),
            }])?;
            dialog.SetOptions(dialog.GetOptions()? | FOS_ALLOWMULTISELECT | FOS_FILEMUSTEXIST)?;
            dialog.Show((!owner.is_invalid()).then_some(owner))?;
            let items = dialog.GetResults()?;
            let mut paths = Vec::new();
            for at in 0..items.GetCount()? {
                let name = items.GetItemAt(at)?.GetDisplayName(SIGDN_FILESYSPATH)?;
                let path = name.to_string();
                CoTaskMemFree(Some(name.0.cast_const().cast()));
                paths.push(PathBuf::from(path?));
            }
            Ok(paths)
        })()
        .unwrap_or_default();
        CoUninitialize();
        picked
    }
}

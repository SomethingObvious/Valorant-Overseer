//! A clip played by Windows' own media engine in a child of the app's window,
//! from the video process. The low-power GPU's video hardware decodes it and
//! Windows composites it, so neither the CPU nor the app's own frame touches
//! its pixels.

use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HMODULE, HWND, RECT};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_CREATE_DEVICE_VIDEO_SUPPORT, D3D11_SDK_VERSION,
    D3D11CreateDevice, ID3D11Device, ID3D11Multithread,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, DXGI_GPU_PREFERENCE_MINIMUM_POWER, IDXGIAdapter1, IDXGIFactory6,
};
use windows::Win32::Graphics::Gdi::{CreateRectRgn, SetWindowRgn};
use windows::Win32::Media::MediaFoundation::{
    CLSID_MFMediaEngineClassFactory, IMFAttributes, IMFDXGIDeviceManager, IMFMediaEngine,
    IMFMediaEngineClassFactory, IMFMediaEngineEx, IMFMediaEngineNotify, IMFMediaEngineNotify_Impl,
    MF_MEDIA_ENGINE_CALLBACK, MF_MEDIA_ENGINE_DXGI_MANAGER, MF_MEDIA_ENGINE_EVENT_ENDED,
    MF_MEDIA_ENGINE_EVENT_ERROR, MF_MEDIA_ENGINE_EVENT_FIRSTFRAMEREADY,
    MF_MEDIA_ENGINE_EVENT_LOADEDMETADATA, MF_MEDIA_ENGINE_PLAYBACK_HWND,
    MF_MEDIA_ENGINE_PRELOAD_AUTOMATIC, MF_MEDIA_ENGINE_VIDEO_OUTPUT_FORMAT, MFCreateAttributes,
    MFCreateDXGIDeviceManager,
};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::SystemServices::SS_BLACKRECT;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, SW_HIDE, SWP_DEFERERASE, SWP_NOACTIVATE, SWP_NOCOPYBITS,
    SWP_NOZORDER, SWP_SHOWWINDOW, SetWindowPos, ShowWindow, WINDOW_STYLE, WS_CHILD,
    WS_CLIPSIBLINGS, WS_DISABLED, WS_EX_NOPARENTNOTIFY,
};
use windows_core::{BSTR, Interface, PCWSTR, implement, w};

use crate::Area;

/// How long a window just shown stays clipped to nothing while the engine
/// draws into it at its size. Without it the window came up white on every
/// try on the laptop this was tested on, and with 150 ms it never did.
const SETTLE: Duration = Duration::from_millis(150);

thread_local! {
    /// The low-power GPU's device and the manager that shares it with the
    /// engine, made for the first clip and kept for every one after, since
    /// making them took a moment on the window's thread each time.
    static DEVICE: RefCell<Option<IMFDXGIDeviceManager>> = const { RefCell::new(None) };
}

/// What the engine has said since the app last asked. Set on Media
/// Foundation's own threads, read on the window's.
struct Events {
    /// It knows the video's size.
    loaded: AtomicBool,
    /// It has a frame to show.
    ready: AtomicBool,
    /// It played to the end.
    ended: AtomicBool,
    /// It couldn't read or play the file.
    failed: AtomicBool,
    /// Wakes the video process's loop, so it hears about each event.
    wake: Box<dyn Fn() + Send + Sync>,
}

/// The engine's events, passed on to [`Events`].
#[implement(IMFMediaEngineNotify)]
struct Notify(Arc<Events>);

impl IMFMediaEngineNotify_Impl for Notify_Impl {
    fn EventNotify(&self, event: u32, _param1: usize, _param2: u32) -> windows_core::Result<()> {
        let is = |kind: i32| u32::try_from(kind).is_ok_and(|kind| kind == event);
        let flag = if is(MF_MEDIA_ENGINE_EVENT_LOADEDMETADATA.0) {
            &self.0.loaded
        } else if is(MF_MEDIA_ENGINE_EVENT_FIRSTFRAMEREADY.0) {
            &self.0.ready
        } else if is(MF_MEDIA_ENGINE_EVENT_ENDED.0) {
            &self.0.ended
        } else if is(MF_MEDIA_ENGINE_EVENT_ERROR.0) {
            &self.0.failed
        } else {
            return Ok(());
        };
        flag.store(true, Ordering::Relaxed);
        (self.0.wake)();
        Ok(())
    }
}

/// A clip playing in a child window of the app's, shown where [`Clip::place`]
/// puts it. Dropping it stops the engine and removes the window.
pub(crate) struct Clip {
    /// The media engine, which decodes and presents into `child`.
    engine: IMFMediaEngine,
    /// The child window the video is drawn in.
    child: HWND,
    /// What the engine has said.
    events: Arc<Events>,
    /// Where it was last put and the part of that which could be seen, or
    /// nothing while hidden.
    placed: Option<(Area, Area)>,
    /// The size the picture was last fitted to.
    fitted: Option<[i32; 2]>,
    /// When the window was last shown after being hidden.
    shown_at: Option<Instant>,
    /// Whether it is clipped to nothing, as it is for [`SETTLE`] after
    /// it is shown.
    blank: bool,
}

impl std::fmt::Debug for Clip {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Clip")
            .field("placed", &self.placed)
            .finish_non_exhaustive()
    }
}

impl Clip {
    /// Opens `file` in a child of `parent`, paused at its first frame and
    /// hidden until placed. `wake` is called, from another thread, whenever
    /// the engine has news.
    pub(crate) fn open(
        parent: HWND,
        file: &str,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Result<Self, String> {
        let events = Arc::new(Events {
            loaded: AtomicBool::new(false),
            ready: AtomicBool::new(false),
            ended: AtomicBool::new(false),
            failed: AtomicBool::new(false),
            wake: Box::new(wake),
        });
        open_in(parent, file, Arc::clone(&events))
            .map(|(engine, child)| Self {
                engine,
                child,
                events,
                placed: None,
                fitted: None,
                shown_at: None,
                blank: false,
            })
            .map_err(|e| e.message())
    }

    /// Plays on from where it is.
    pub(crate) fn play(&self) {
        // SAFETY: `engine` is a live engine this value owns, used on the
        // thread that made it.
        let _played = unsafe { self.engine.Play() };
    }

    /// Stops where it is, keeping that frame on screen.
    pub(crate) fn pause(&self) {
        // SAFETY: as in `play`.
        let _paused = unsafe { self.engine.Pause() };
    }

    /// Whether it is playing.
    pub(crate) fn playing(&self) -> bool {
        // SAFETY: as in `play`.
        unsafe { !self.engine.IsPaused().as_bool() && !self.engine.IsEnded().as_bool() }
    }

    /// Where it has got to, in seconds.
    pub(crate) fn time(&self) -> f64 {
        // SAFETY: as in `play`.
        unsafe { self.engine.GetCurrentTime() }
    }

    /// Goes to `at` seconds, playing on from there if it was playing.
    pub(crate) fn seek(&self, at: f64) {
        // SAFETY: as in `play`.
        let _sought = unsafe { self.engine.SetCurrentTime(at.max(0.0)) };
    }

    /// Goes to `at` as [`Clip::seek`] does, unless the last seek is still
    /// going, so following a dragged handle doesn't queue a seek a frame.
    pub(crate) fn scrub(&self, at: f64) {
        // SAFETY: as in `play`.
        unsafe {
            if !self.engine.IsSeeking().as_bool() {
                let _sought = self.engine.SetCurrentTime(at.max(0.0));
            }
        }
    }

    /// Plays `rate` times as fast as recorded, the sound kept at its pitch.
    pub(crate) fn set_rate(&self, rate: f64) {
        // SAFETY: as in `play`.
        unsafe {
            let _default = self.engine.SetDefaultPlaybackRate(rate);
            let _now = self.engine.SetPlaybackRate(rate);
        }
    }

    /// How loud, from 0 for silent to 1 for as recorded.
    pub(crate) fn set_volume(&self, share: f64) {
        let share = share.clamp(0.0, 1.0);
        // SAFETY: as in `play`.
        unsafe {
            let _volume = self.engine.SetVolume(share);
            let _muted = self.engine.SetMuted(share <= 0.0);
        }
    }

    /// The video's width and height, once the engine has read them.
    pub(crate) fn size(&self) -> Option<[u32; 2]> {
        let (mut wide, mut tall) = (0, 0);
        // SAFETY: as in `play`, with both pointers to locals that outlive the call.
        let read = unsafe {
            self.engine
                .GetNativeVideoSize(Some(&raw mut wide), Some(&raw mut tall))
        };
        (read.is_ok() && wide > 0 && tall > 0).then_some([wide, tall])
    }

    /// Whether it played to the end since this was last asked.
    pub(crate) fn take_ended(&self) -> bool {
        self.events.ended.swap(false, Ordering::Relaxed)
    }

    /// Whether the engine couldn't read or play the file.
    pub(crate) fn failed(&self) -> bool {
        self.events.failed.load(Ordering::Relaxed)
    }

    /// Puts the video at `shown`'s first box in the window's client area,
    /// showing only the part of it inside the second, or hides it. Returns
    /// whether to call again shortly, while a window just shown is still
    /// clipped to nothing.
    pub(crate) fn place(&mut self, shown: Option<(Area, Area)>) -> bool {
        // Shown straight away, the window is white until the engine has drawn
        // into it at its new size. So it waits for the engine's first frame,
        // then stays clipped to nothing for `SETTLE`.
        let shown = shown.filter(|_| self.events.ready.load(Ordering::Relaxed));
        let Some(([left, top, right, bottom], [l, t, r, b])) = shown else {
            if self.placed.take().is_some() {
                // SAFETY: `child` is a window this value made and owns.
                let _was = unsafe { ShowWindow(self.child, SW_HIDE) };
            }
            self.shown_at = None;
            return false;
        };
        let settling = self.shown_at.get_or_insert_with(Instant::now).elapsed() < SETTLE;
        if shown != self.placed || settling != self.blank {
            // SAFETY: `child` is a window this value made and owns.
            // The region is handed to the window, which frees it.
            unsafe {
                let _moved = SetWindowPos(
                    self.child,
                    None,
                    left,
                    top,
                    right - left,
                    bottom - top,
                    // No waiting on the app's window to repaint behind it, which is
                    // another process, and no copying old pixels the engine redraws.
                    SWP_NOACTIVATE
                        | SWP_NOZORDER
                        | SWP_SHOWWINDOW
                        | SWP_DEFERERASE
                        | SWP_NOCOPYBITS,
                );
                let seen = if settling {
                    CreateRectRgn(0, 0, 0, 0)
                } else {
                    CreateRectRgn(l - left, t - top, r - left, b - top)
                };
                let _clipped = SetWindowRgn(self.child, Some(seen), true);
            }
            self.placed = shown;
            self.blank = settling;
        }
        self.fit();
        settling
    }

    /// Fits the picture to the window it is in, once the engine knows the
    /// video's size and whenever the window's size has changed.
    fn fit(&mut self) {
        let Some(([left, top, right, bottom], _)) = self.placed else {
            return;
        };
        let size = [right - left, bottom - top];
        if !self.events.loaded.load(Ordering::Relaxed) || self.fitted == Some(size) {
            return;
        }
        let whole = RECT {
            left: 0,
            top: 0,
            right: size[0],
            bottom: size[1],
        };
        // SAFETY: as in `play`, with `whole` a local that outlives the call.
        let fitted = unsafe {
            self.engine
                .cast::<IMFMediaEngineEx>()
                .and_then(|ex| ex.UpdateVideoStream(None, Some(&raw const whole), None))
        };
        if fitted.is_ok() {
            self.fitted = Some(size);
        }
    }
}

impl Drop for Clip {
    fn drop(&mut self) {
        // SAFETY: the engine and the window are this value's, and nothing
        // uses either after this.
        unsafe {
            let _stopped = self.engine.Shutdown();
            let _gone = DestroyWindow(self.child);
        }
    }
}

/// Makes the engine and the child window it draws in, for `file` inside
/// `parent`, on a device of the low-power GPU.
fn open_in(
    parent: HWND,
    file: &str,
    events: Arc<Events>,
) -> windows_core::Result<(IMFMediaEngine, HWND)> {
    // SAFETY: every call is a documented Win32, DXGI, Direct3D or Media
    // Foundation call on the window's thread, given handles and interfaces
    // made just above it, and pointers only to locals that outlive the call.
    unsafe {
        let instance = GetModuleHandleW(None)?;
        // A static control, black so there is no grey before the engine's
        // first frame, and disabled, since Windows passes over a disabled
        // child when it picks which window gets the mouse. The parent is
        // another process's window, so that is the only way a click on the
        // video reaches the app.
        let child = CreateWindowExW(
            WS_EX_NOPARENTNOTIFY,
            w!("STATIC"),
            PCWSTR::null(),
            WS_CHILD | WS_CLIPSIBLINGS | WS_DISABLED | WINDOW_STYLE(SS_BLACKRECT.0),
            0,
            0,
            1,
            1,
            Some(parent),
            None,
            Some(instance.into()),
            None,
        )?;
        let made = engine_for(child, file, events);
        if made.is_err() {
            let _gone = DestroyWindow(child);
        }
        made.map(|engine| (engine, child))
    }
}

/// The engine itself, drawing into `child`.
fn engine_for(
    child: HWND,
    file: &str,
    events: Arc<Events>,
) -> windows_core::Result<IMFMediaEngine> {
    let manager = shared_device()?;
    // SAFETY: as in `open_in`.
    unsafe {
        let mut attributes: Option<IMFAttributes> = None;
        MFCreateAttributes(&raw mut attributes, 4)?;
        let attributes = attributes.ok_or_else(windows_core::Error::empty)?;
        let notify: IMFMediaEngineNotify = Notify(events).into();
        attributes.SetUnknown(&MF_MEDIA_ENGINE_CALLBACK, &notify)?;
        attributes.SetUnknown(&MF_MEDIA_ENGINE_DXGI_MANAGER, &manager)?;
        attributes.SetUINT64(&MF_MEDIA_ENGINE_PLAYBACK_HWND, child.0 as u64)?;
        attributes.SetUINT32(
            &MF_MEDIA_ENGINE_VIDEO_OUTPUT_FORMAT,
            u32::try_from(DXGI_FORMAT_B8G8R8A8_UNORM.0).unwrap_or_default(),
        )?;
        let engines: IMFMediaEngineClassFactory =
            CoCreateInstance(&CLSID_MFMediaEngineClassFactory, None, CLSCTX_INPROC_SERVER)?;
        let engine = engines.CreateInstance(0, &attributes)?;
        engine.SetAutoPlay(false)?;
        engine.SetPreload(MF_MEDIA_ENGINE_PRELOAD_AUTOMATIC)?;
        engine.SetSource(&BSTR::from(file))?;
        Ok(engine)
    }
}

/// The device the engine decodes on and the manager that hands it over, on
/// the low-power GPU so decoding stays off the one games run on. Made once.
fn shared_device() -> windows_core::Result<IMFDXGIDeviceManager> {
    if let Some(kept) = DEVICE.with_borrow(Clone::clone) {
        return Ok(kept);
    }
    // SAFETY: as in `open_in`.
    let manager = unsafe {
        let factory: IDXGIFactory6 = CreateDXGIFactory1()?;
        let adapter: IDXGIAdapter1 =
            factory.EnumAdapterByGpuPreference(0, DXGI_GPU_PREFERENCE_MINIMUM_POWER)?;
        let mut device: Option<ID3D11Device> = None;
        D3D11CreateDevice(
            &adapter,
            D3D_DRIVER_TYPE_UNKNOWN,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_VIDEO_SUPPORT | D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&raw mut device),
            None,
            None,
        )?;
        let device = device.ok_or_else(windows_core::Error::empty)?;
        // The engine uses the device from threads of its own.
        let _was = device
            .cast::<ID3D11Multithread>()?
            .SetMultithreadProtected(true);
        let mut token = 0;
        let mut manager: Option<IMFDXGIDeviceManager> = None;
        MFCreateDXGIDeviceManager(&raw mut token, &raw mut manager)?;
        let manager = manager.ok_or_else(windows_core::Error::empty)?;
        manager.ResetDevice(&device, token)?;

        manager
    };
    DEVICE.with_borrow_mut(|kept| *kept = Some(manager.clone()));
    Ok(manager)
}

//! Plays a video file with Windows' own media engine, in a child of the
//! app's window. The low-power GPU's video hardware decodes it and Windows
//! composites it, so neither the CPU nor the app's own frame touches its
//! pixels, where decoding with ffmpeg and drawing each frame with egui took
//! most of a core and redrew the whole window for every frame.
//!
//! This is the one crate in the workspace that calls C APIs, which is why it
//! alone allows `unsafe`. Everything here runs on the thread that owns the
//! app's window, apart from [`Video`]'s events, which only set flags.

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Arc, Once};

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
    MF_MEDIA_ENGINE_EVENT_ERROR, MF_MEDIA_ENGINE_EVENT_LOADEDMETADATA,
    MF_MEDIA_ENGINE_PLAYBACK_HWND, MF_MEDIA_ENGINE_PRELOAD_AUTOMATIC,
    MF_MEDIA_ENGINE_VIDEO_OUTPUT_FORMAT, MF_VERSION, MFCreateAttributes, MFCreateDXGIDeviceManager,
    MFSTARTUP_LITE, MFStartup,
};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, SW_HIDE, SWP_NOACTIVATE, SWP_NOZORDER, SWP_SHOWWINDOW,
    SetWindowPos, ShowWindow, WS_CHILD, WS_CLIPSIBLINGS, WS_EX_NOPARENTNOTIFY,
};
use windows_core::{BSTR, Interface, PCWSTR, implement, w};

/// The app's window, as its handle, which every video plays inside.
static WINDOW: AtomicIsize = AtomicIsize::new(0);

/// Media Foundation's start, once a process.
static STARTED: Once = Once::new();

/// Tells the crate which window videos play in, by its Win32 handle.
pub fn set_window(hwnd: isize) {
    WINDOW.store(hwnd, Ordering::Relaxed);
}

/// A box in the window's client area, in physical pixels: left, top, right
/// and bottom.
pub type Area = [i32; 4];

/// What the engine has said since the app last asked. Set on Media
/// Foundation's own threads, read on the window's.
struct Events {
    /// It knows the video's size.
    loaded: AtomicBool,
    /// It played to the end.
    ended: AtomicBool,
    /// It couldn't read or play the file.
    failed: AtomicBool,
    /// Asks the window for a frame, so it hears about each event.
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

/// A video playing in a child window of the app's, shown where [`Video::place`]
/// puts it. Dropping it stops the engine and removes the window.
pub struct Video {
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
}

impl std::fmt::Debug for Video {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Video")
            .field("placed", &self.placed)
            .finish_non_exhaustive()
    }
}

impl Video {
    /// Opens `file`, paused at its first frame and hidden until placed.
    /// `wake` is called, from another thread, whenever the engine has news.
    ///
    /// # Errors
    ///
    /// Why it couldn't: no window was set, or Windows has no engine, device or
    /// window to give it.
    pub fn open(file: &str, wake: impl Fn() + Send + Sync + 'static) -> Result<Self, String> {
        let window = WINDOW.load(Ordering::Relaxed);
        if window == 0 {
            return Err("there is no window to play the video in".to_owned());
        }
        STARTED.call_once(|| {
            // SAFETY: called once, on the window's thread. A thread COM is
            // already set up on answers S_FALSE or RPC_E_CHANGED_MODE, and
            // either way COM works, so neither is an error worth stopping on.
            let _apartment = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
            // SAFETY: the documented start of Media Foundation, once a process.
            // Failing leaves every `open` after this to fail on its own.
            let _started = unsafe { MFStartup(MF_VERSION, MFSTARTUP_LITE) };
        });
        let events = Arc::new(Events {
            loaded: AtomicBool::new(false),
            ended: AtomicBool::new(false),
            failed: AtomicBool::new(false),
            wake: Box::new(wake),
        });
        let parent = HWND(window as *mut c_void);
        open_in(parent, file, Arc::clone(&events))
            .map(|(engine, child)| Self {
                engine,
                child,
                events,
                placed: None,
                fitted: None,
            })
            .map_err(|e| e.message())
    }

    /// Plays on from where it is.
    pub fn play(&self) {
        // SAFETY: `engine` is a live engine this value owns, used on the
        // thread that made it.
        let _played = unsafe { self.engine.Play() };
    }

    /// Stops where it is, keeping that frame on screen.
    pub fn pause(&self) {
        // SAFETY: as in `play`.
        let _paused = unsafe { self.engine.Pause() };
    }

    /// Whether it is playing.
    #[must_use]
    pub fn playing(&self) -> bool {
        // SAFETY: as in `play`.
        unsafe { !self.engine.IsPaused().as_bool() && !self.engine.IsEnded().as_bool() }
    }

    /// Where it has got to, in seconds.
    #[must_use]
    pub fn time(&self) -> f64 {
        // SAFETY: as in `play`.
        unsafe { self.engine.GetCurrentTime() }
    }

    /// Goes to `at` seconds, playing on from there if it was playing.
    pub fn seek(&self, at: f64) {
        // SAFETY: as in `play`.
        let _sought = unsafe { self.engine.SetCurrentTime(at.max(0.0)) };
    }

    /// Plays `rate` times as fast as recorded, the sound kept at its pitch.
    pub fn set_rate(&self, rate: f64) {
        // SAFETY: as in `play`.
        unsafe {
            let _default = self.engine.SetDefaultPlaybackRate(rate);
            let _now = self.engine.SetPlaybackRate(rate);
        }
    }

    /// How loud, from 0 for silent to 1 for as recorded.
    pub fn set_volume(&self, share: f64) {
        let share = share.clamp(0.0, 1.0);
        // SAFETY: as in `play`.
        unsafe {
            let _volume = self.engine.SetVolume(share);
            let _muted = self.engine.SetMuted(share <= 0.0);
        }
    }

    /// The video's width and height, once the engine has read them.
    #[must_use]
    pub fn size(&self) -> Option<[u32; 2]> {
        let (mut wide, mut tall) = (0, 0);
        // SAFETY: as in `play`, with both pointers to locals that outlive the call.
        let read = unsafe {
            self.engine
                .GetNativeVideoSize(Some(&raw mut wide), Some(&raw mut tall))
        };
        (read.is_ok() && wide > 0 && tall > 0).then_some([wide, tall])
    }

    /// Whether it played to the end since this was last asked.
    #[must_use]
    pub fn take_ended(&self) -> bool {
        self.events.ended.swap(false, Ordering::Relaxed)
    }

    /// Whether the engine couldn't read or play the file.
    #[must_use]
    pub fn failed(&self) -> bool {
        self.events.failed.load(Ordering::Relaxed)
    }

    /// Puts the video at `shown`'s first box in the window's client area,
    /// showing only the part of it inside the second, or hides it.
    pub fn place(&mut self, shown: Option<(Area, Area)>) {
        if shown != self.placed {
            match shown {
                None => {
                    // SAFETY: `child` is a window this value made and owns.
                    let _was = unsafe { ShowWindow(self.child, SW_HIDE) };
                }
                Some(([left, top, right, bottom], [l, t, r, b])) => {
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
                            SWP_NOACTIVATE | SWP_NOZORDER | SWP_SHOWWINDOW,
                        );
                        let seen = CreateRectRgn(l - left, t - top, r - left, b - top);
                        let _clipped = SetWindowRgn(self.child, Some(seen), true);
                    }
                }
            }
            self.placed = shown;
        }
        self.fit();
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

impl Drop for Video {
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
        // A static control, which lets clicks through to the window under it,
        // so the app still hears a click on the video.
        let child = CreateWindowExW(
            WS_EX_NOPARENTNOTIFY,
            w!("STATIC"),
            PCWSTR::null(),
            WS_CHILD | WS_CLIPSIBLINGS,
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
    // SAFETY: as in `open_in`.
    unsafe {
        // The low-power GPU, so decoding stays off the one games run on.
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

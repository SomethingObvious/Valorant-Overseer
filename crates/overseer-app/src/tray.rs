//! The icon beside the clock, and the menu behind it.
//!
//! Two jobs. It is somewhere to put the window: closing the window hides it
//! instead of quitting, because an app you want up for a whole evening of
//! games should not have to be started again after every accidental click on
//! the wrong corner. And it is proof of life: while the overlay is on there
//! is no title bar and no ordinary window, and an app with no visible trace
//! of itself is an app people kill in Task Manager.
//!
//! Quit is in the menu and nowhere else, which is the price of the first
//! job. If the icon cannot be created the window goes back to quitting when
//! it is closed, because a window that cannot be closed and has nothing to
//! close it from is a bug rather than a feature.

use std::path::Path;
use std::sync::mpsc::{Receiver, TryIter, channel};

use tray_icon::menu::{IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder, TrayIconEvent};

/// What somebody asked the tray for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    /// Put the ordinary window back on screen.
    Window,
    /// Put the overlay back on screen.
    Overlay,
    /// Stop.
    Quit,
}

/// The icon, alive for as long as this is. Dropping it removes the icon.
pub(crate) struct Tray {
    /// Holds the icon on the taskbar. Never read.
    _icon: TrayIcon,
    /// What has been clicked since the last look.
    actions: Receiver<Action>,
}

impl std::fmt::Debug for Tray {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tray").finish_non_exhaustive()
    }
}

impl Tray {
    /// Everything clicked since the last look.
    pub(crate) fn actions(&self) -> TryIter<'_, Action> {
        self.actions.try_iter()
    }
}

/// Puts the icon on the taskbar, and wakes the window when it is used.
///
/// Must be called from the thread running the message loop, for the same
/// reason as the hotkey: the icon owns a window, and its messages arrive on
/// the queue of the thread that made it.
pub(crate) fn start(root: &Path, wake: impl Fn() + Send + Sync + 'static) -> Result<Tray, String> {
    let file = root.join("assets").join("overseer.ico");
    let icon = Icon::from_path(&file, None).map_err(|why| format!("{}: {why}", file.display()))?;

    let menu = Menu::new();
    let window = MenuItem::new("Window", true, None);
    let overlay = MenuItem::new("Overlay", true, None);
    let quit = MenuItem::new("Quit", true, None);
    let separator = PredefinedMenuItem::separator();
    let items: [&dyn IsMenuItem; 4] = [&window, &overlay, &separator, &quit];
    for item in items {
        menu.append(item).map_err(|why| why.to_string())?;
    }
    let (window_id, overlay_id, quit_id) =
        (window.id().clone(), overlay.id().clone(), quit.id().clone());

    let tray = TrayIconBuilder::new()
        .with_tooltip("Valorant Overseer")
        .with_icon(icon)
        .with_menu(Box::new(menu))
        .build()
        .map_err(|why| why.to_string())?;

    let (tx, actions) = channel();
    let clicks = tx.clone();
    let wake = std::sync::Arc::new(wake);
    let woken = std::sync::Arc::clone(&wake);
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let action = match &event.id {
            id if *id == window_id => Action::Window,
            id if *id == overlay_id => Action::Overlay,
            id if *id == quit_id => Action::Quit,
            _ => return,
        };
        if clicks.send(action).is_ok() {
            woken();
        }
    }));
    TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
        // Only a finished left click. The move and enter events arrive
        // whenever the cursor crosses the icon, and waking the window for
        // those would have the app drawing frames because somebody reached
        // for the clock.
        if !matches!(
            event,
            TrayIconEvent::Click {
                button: tray_icon::MouseButton::Left,
                button_state: tray_icon::MouseButtonState::Up,
                ..
            }
        ) {
            return;
        }
        if tx.send(Action::Window).is_ok() {
            wake();
        }
    }));

    Ok(Tray {
        _icon: tray,
        actions,
    })
}

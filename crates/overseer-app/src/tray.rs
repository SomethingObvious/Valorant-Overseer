//! The icon beside the clock, and its menu.
//!
//! With the overlay on it is the one sign the app is running, and the way
//! back to the window. Closing the window quits, the same as Quit here.

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

/// The tray icon. Dropping it removes the icon.
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

/// Adds the icon and calls `wake` when it is used. Call it on the message
/// loop's thread, because the icon owns a hidden window and its messages go
/// to the thread that made it.
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
        // Only a finished left click. Move and enter events fire whenever the
        // cursor crosses the icon, and none of them is worth a frame.
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

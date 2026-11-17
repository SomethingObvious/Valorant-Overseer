//! Offline chat: whether your friends see you, from the chat proxy the
//! backend runs. Settings asks how it is every few seconds while it shows.

use overseer_core::Bridge;
use serde::Deserialize;

/// How often Settings asks the proxy how it is, in seconds.
pub(crate) const POLL: f64 = 3.0;

/// What the proxy says about itself.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct State {
    /// Whether the Riot client was started through it.
    pub(crate) running: bool,
    pub(crate) enabled: bool,
    pub(crate) connected: bool,
}

#[derive(Debug, Default)]
pub(crate) struct Offline {
    pub(crate) state: Option<State>,
    pub(crate) said: Option<String>,
    pub(crate) wanted: Option<bool>,
    sent: Option<bool>,
    pub(crate) playing: bool,
    pub(crate) armed: Option<f64>,
    waiting: Option<u64>,
    asked_at: Option<f64>,
}

impl Offline {
    pub(crate) fn waiting_on(&self, id: u64) -> bool {
        self.waiting == Some(id)
    }

    pub(crate) fn answered(&mut self, result: Result<serde_json::Value, String>) {
        self.waiting = None;
        self.sent = None;
        match result {
            Ok(value) => {
                if let Some(message) = value.get("message").and_then(|m| m.as_str()) {
                    self.said = Some(message.to_owned());
                }
                if let Ok(state) = serde_json::from_value(value) {
                    self.state = Some(state);
                }
            }
            Err(why) => self.said = Some(why),
        }
    }

    pub(crate) fn reconnected(&mut self) {
        self.waiting = None;
        if self.sent.take().is_some() {
            self.said = Some(
                "Overseer lost its connection before that went through, so nothing changed."
                    .to_owned(),
            );
        }
    }

    /// Whether the switch shows on: the newest press, or else what the
    /// proxy said.
    pub(crate) fn on(&self) -> bool {
        self.wanted
            .or(self.sent)
            .unwrap_or_else(|| self.state.is_some_and(|s| s.running && s.enabled))
    }

    /// Sends a press of the switch, or else asks how the proxy is when the
    /// last answer is old.
    pub(crate) fn ask(&mut self, bridge: &Bridge, now: f64) {
        if self.waiting.is_some() {
            return;
        }
        if let Some(on) = self.wanted.take() {
            self.said = None;
            self.sent = Some(on);
            self.waiting = Some(bridge.ask("offline_set", serde_json::json!({ "on": on })));
        } else if self.asked_at.is_none_or(|at| now - at >= POLL) {
            self.waiting = Some(bridge.ask("offline", serde_json::Value::Null));
        } else {
            return;
        }
        self.asked_at = Some(now);
    }
}

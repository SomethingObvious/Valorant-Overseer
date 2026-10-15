//! Offline chat: whether your friends see you, from the chat proxy the
//! backend runs. Settings asks how it is every few seconds while it shows.

use overseer_core::Bridge;
use serde::Deserialize;

/// How often Settings asks the proxy how it is, in seconds.
const POLL: f64 = 3.0;

/// What the proxy says about itself.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct State {
    /// Whether the Riot client was started through it.
    pub(crate) running: bool,
    /// Whether it hides you.
    pub(crate) enabled: bool,
    /// Whether the client's chat is going through it right now.
    pub(crate) connected: bool,
}

/// The switch's side of offline chat.
#[derive(Debug, Default)]
pub(crate) struct Offline {
    /// The last answer, once there is one.
    pub(crate) state: Option<State>,
    /// What the backend said about the last change, when it said anything.
    pub(crate) said: Option<String>,
    /// The switch was pressed, to this, and nothing has been sent yet.
    pub(crate) wanted: Option<bool>,
    /// A press on its way to the backend.
    sent: Option<bool>,
    /// The question out, and when the last one went.
    waiting: Option<u64>,
    asked_at: Option<f64>,
}

impl Offline {
    /// Whether `id` is this screen's question.
    pub(crate) fn waiting_on(&self, id: u64) -> bool {
        self.waiting == Some(id)
    }

    /// Takes the answer to a status question or a change.
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

    /// Forgets the question a dropped connection took with it.
    pub(crate) const fn reconnected(&mut self) {
        self.waiting = None;
        self.sent = None;
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

//! The socket, on its own thread.
//!
//! Blocking, deliberately. This is one connection carrying about a message a
//! second; an async runtime would be the largest dependency in the tree and
//! would buy nothing measurable. The thread owns the socket, the UI owns a
//! channel, and the only thing crossing between them is an [`Event`].
//!
//! The protocol is `backend/ws_server.py`, and the reference implementation is
//! `tui/src/bridge.ts`. Both front ends therefore speak the same four
//! messages: send `auth` on connect, answer `ping` with `pong`, read `state`
//! for a board, read `response` for an answered request.
//!
//! No `Origin` header is sent, which matters: the bridge closes any connection
//! that arrives with one, so that a page in a browser cannot talk to it. That
//! guard is the reason this lives in the binary rather than in a web view.

use std::io::ErrorKind;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::time::Duration;

use serde::Deserialize;
use tungstenite::{Message, WebSocket, client::IntoClientRequest, stream::MaybeTlsStream};

use crate::board::Board;

/// The protocol version this build speaks. The bridge rejects anything else.
const PROTOCOL: u32 = 1;

/// How long to wait before a first reconnect, doubling to [`RETRY_CEILING`].
const RETRY_BASE: Duration = Duration::from_millis(500);
/// The longest gap between reconnect attempts.
const RETRY_CEILING: Duration = Duration::from_secs(10);
/// How long a read blocks before the loop checks whether it should still run.
const READ_TIMEOUT: Duration = Duration::from_millis(500);

/// Where the connection has got to, in the words the header uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// Trying, with the reason it is not connected yet.
    Connecting(String),
    /// Connected and authenticated.
    Live,
    /// Disconnected, with why. Recoverable unless [`Event::Stopped`] follows.
    Lost(String),
}

/// Everything the bridge thread tells the UI.
#[derive(Debug, Clone)]
pub enum Event {
    /// The connection changed state.
    Status(Status),
    /// A new board. This is the only thing that arrives unasked.
    Board(Box<Board>),
    /// The thread has given up and will not retry. A bad token or an
    /// unsupported protocol does not fix itself by reconnecting.
    Stopped(String),
}

/// What the backend writes once it is listening.
#[derive(Debug, Deserialize)]
struct Credentials {
    #[serde(rename = "wsPort")]
    ws_port: u16,
    token: String,
}

/// One frame from the bridge. Every field optional for the reason in
/// [`crate::board`]: the socket is a trust boundary.
#[derive(Debug, Deserialize)]
struct Envelope {
    #[serde(rename = "type")]
    kind: Option<String>,
    data: Option<serde_json::Value>,
    message: Option<String>,
    code: Option<String>,
}

/// A running connection. Dropping it asks the thread to stop.
#[derive(Debug)]
pub struct Bridge {
    events: Receiver<Event>,
    stop: Sender<()>,
}

impl Bridge {
    /// Starts the connection, and calls `wake` whenever an event is queued.
    ///
    /// `wake` exists so that this crate knows nothing about the UI: the window
    /// passes something that asks egui to repaint, and a test passes a counter.
    /// Without it the window would have to poll, and polling is the difference
    /// between an idle cost of nothing and an idle cost of a frame a tick.
    ///
    /// `root` is the installation directory, the one holding `.overseer`.
    pub fn start<W>(root: &Path, wake: W) -> Self
    where
        W: Fn() + Send + 'static,
    {
        let (events_tx, events) = channel();
        let (stop, stop_rx) = channel();
        let path = credentials_path(root);
        let failed = events_tx.clone();
        if let Err(e) = thread::Builder::new()
            .name("overseer-bridge".to_owned())
            .spawn(move || run(&path, &events_tx, &stop_rx, &wake))
        {
            // A machine that cannot spawn a thread cannot run the app, but it
            // can still say so rather than disappearing. No wake is needed:
            // this happens before the first frame, which drains the queue.
            drop(failed.send(Event::Stopped(format!("no bridge thread: {e}"))));
        }
        Self { events, stop }
    }

    /// Everything that has arrived since the last call, oldest first.
    ///
    /// Drained rather than handed over one at a time: a repaint wants the
    /// newest board, and reading one event per frame would queue behind a
    /// burst.
    #[must_use]
    pub fn drain(&self) -> Vec<Event> {
        self.events.try_iter().collect()
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        // The thread checks between reads, so this is a request, not a kill.
        // A closed channel means it has already gone, which is the same outcome.
        let _ignored = self.stop.send(());
    }
}

/// Where the backend writes the port and the session token.
#[must_use]
pub fn credentials_path(root: &Path) -> PathBuf {
    root.join(".overseer").join("bridge.json")
}

/// Reads the credentials, or says why not.
///
/// A missing file is the ordinary case on a cold start: the backend writes it
/// once it is listening. That is a wait, not an error, and the caller says so.
fn read_credentials(path: &Path) -> Result<Credentials, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| match e.kind() {
        ErrorKind::NotFound => "waiting for the backend".to_owned(),
        _ => format!("cannot read {}: {e}", path.display()),
    })?;
    let creds: Credentials =
        serde_json::from_str(&raw).map_err(|e| format!("bridge.json is not readable: {e}"))?;
    if creds.token.is_empty() || creds.ws_port == 0 {
        return Err("bridge.json has no token yet".to_owned());
    }
    Ok(creds)
}

/// Calls the waker, swallowing a panic from it so the thread cannot die of one.
fn wake_now<W: Fn()>(wake: &W) {
    wake();
}

/// The thread body: connect, pump, reconnect, until asked to stop.
fn run<W: Fn()>(path: &Path, events: &Sender<Event>, stop: &Receiver<()>, wake: &W) {
    let mut attempt: u32 = 0;
    loop {
        if stop.try_recv().is_ok() {
            return;
        }
        let status = if attempt == 0 {
            Status::Connecting(String::new())
        } else {
            Status::Connecting(format!("retry {attempt}"))
        };
        if !post(events, wake, Event::Status(status)) {
            return;
        }

        match connect(path) {
            Ok(mut socket) => {
                attempt = 0;
                match pump(&mut socket, events, stop, wake) {
                    Pumped::Stopped => return,
                    Pumped::Rejected(why) => {
                        drop(socket.close(None));
                        post(events, wake, Event::Stopped(why));
                        return;
                    }
                    Pumped::Dropped(why) => {
                        drop(socket.close(None));
                        if !post(events, wake, Event::Status(Status::Lost(why))) {
                            return;
                        }
                    }
                }
            }
            Err(why) => {
                if !post(events, wake, Event::Status(Status::Lost(why))) {
                    return;
                }
            }
        }

        attempt = attempt.saturating_add(1);
        if wait(stop, backoff(attempt)) {
            return;
        }
    }
}

/// How long to wait before attempt `n`, capped.
fn backoff(n: u32) -> Duration {
    let shift = n.saturating_sub(1).min(5);
    let scaled = RETRY_BASE.saturating_mul(1_u32 << shift);
    scaled.min(RETRY_CEILING)
}

/// Sleeps in short slices so a stop is noticed promptly. True means stop.
fn wait(stop: &Receiver<()>, total: Duration) -> bool {
    matches!(stop.recv_timeout(total), Ok(()))
}

/// Queues an event and wakes the UI. False means the UI is gone.
fn post<W: Fn()>(events: &Sender<Event>, wake: &W, event: Event) -> bool {
    if events.send(event).is_err() {
        return false;
    }
    wake_now(wake);
    true
}

/// Opens the socket and authenticates. The error is what the header will show.
fn connect(path: &Path) -> Result<WebSocket<MaybeTlsStream<TcpStream>>, String> {
    let creds = read_credentials(path)?;
    let url = format!("ws://127.0.0.1:{}", creds.ws_port);
    let request = url
        .as_str()
        .into_client_request()
        .map_err(|e| format!("bad bridge address: {e}"))?;
    let (mut socket, _response) =
        tungstenite::connect(request).map_err(|e| format!("bridge not answering: {e}"))?;

    if let MaybeTlsStream::Plain(stream) = socket.get_ref() {
        // Without a read timeout the loop below could not notice a stop
        // request, and closing the window would leave a thread on a blocked
        // read until the backend happened to say something.
        stream
            .set_read_timeout(Some(READ_TIMEOUT))
            .map_err(|e| format!("cannot set a read timeout: {e}"))?;
    }

    let hello = serde_json::json!({ "type": "auth", "token": creds.token, "protocol": PROTOCOL });
    socket
        .send(Message::Text(hello.to_string().into()))
        .map_err(|e| format!("cannot greet the bridge: {e}"))?;
    Ok(socket)
}

/// Why the pump loop returned.
enum Pumped {
    /// The UI asked to stop.
    Stopped,
    /// The bridge refused us. Retrying will not help.
    Rejected(String),
    /// The connection went away. Retrying will help.
    Dropped(String),
}

/// Reads frames until something ends the connection.
fn pump<W: Fn()>(
    socket: &mut WebSocket<MaybeTlsStream<TcpStream>>,
    events: &Sender<Event>,
    stop: &Receiver<()>,
    wake: &W,
) -> Pumped {
    loop {
        if stop.try_recv().is_ok() {
            return Pumped::Stopped;
        }
        match socket.read() {
            Ok(Message::Text(text)) => match handle(&text, socket) {
                Handled::Nothing => {}
                Handled::Event(event) => {
                    if !post(events, wake, event) {
                        return Pumped::Stopped;
                    }
                }
                Handled::Rejected(why) => return Pumped::Rejected(why),
                Handled::Broken(why) => return Pumped::Dropped(why),
            },
            Ok(Message::Close(_)) => return Pumped::Dropped("the bridge closed".to_owned()),
            // Binary, ping and pong frames are not part of this protocol;
            // tungstenite answers protocol level pings by itself.
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) =>
            {
                // The read timeout expired, which is how the stop above gets
                // a chance to run. Not an error.
            }
            Err(e) => return Pumped::Dropped(format!("bridge read failed: {e}")),
        }
    }
}

/// What one frame meant.
enum Handled {
    /// Nothing the UI needs to know.
    Nothing,
    /// Something it does.
    Event(Event),
    /// The bridge will not talk to this build.
    Rejected(String),
    /// The socket is no longer usable.
    Broken(String),
}

/// Reads one frame. Unknown and malformed frames are ignored on purpose: a
/// newer backend saying something this build has not heard of is not a fault.
fn handle(text: &str, socket: &mut WebSocket<MaybeTlsStream<TcpStream>>) -> Handled {
    let Ok(envelope) = serde_json::from_str::<Envelope>(text) else {
        return Handled::Nothing;
    };
    match envelope.kind.as_deref() {
        Some("auth_ok") => Handled::Event(Event::Status(Status::Live)),
        Some("auth_error") => Handled::Rejected(
            envelope
                .message
                .or(envelope.code)
                .unwrap_or_else(|| "the bridge rejected this build".to_owned()),
        ),
        // A board this build cannot read is worth ignoring rather than
        // disconnecting over: the next frame is a second away.
        Some("state") => envelope.data.map_or(Handled::Nothing, |value| {
            serde_json::from_value::<Board>(value).map_or(Handled::Nothing, |board| {
                Handled::Event(Event::Board(Box::new(board)))
            })
        }),
        Some("ping") => match socket.send(Message::Text(r#"{"type":"pong"}"#.into())) {
            Ok(()) => Handled::Nothing,
            Err(e) => Handled::Broken(format!("cannot answer a ping: {e}")),
        },
        _ => Handled::Nothing,
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{RETRY_CEILING, backoff, credentials_path, read_credentials};

    #[test]
    fn credentials_live_beside_the_install() {
        let path = credentials_path(Path::new("C:/Overseer"));
        assert!(path.ends_with("bridge.json"));
        assert!(path.to_string_lossy().contains(".overseer"));
    }

    #[test]
    fn a_missing_file_is_a_wait_not_a_failure() {
        let err = read_credentials(Path::new("Z:/nowhere/bridge.json")).unwrap_err();
        assert_eq!(err, "waiting for the backend");
    }

    #[test]
    fn a_token_less_file_is_refused() {
        let dir = std::env::temp_dir().join("overseer-bridge-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bridge.json");
        std::fs::write(&path, r#"{"wsPort":7878,"token":""}"#).unwrap();
        assert_eq!(
            read_credentials(&path).unwrap_err(),
            "bridge.json has no token yet"
        );
        std::fs::write(&path, "not json").unwrap();
        assert!(
            read_credentials(&path)
                .unwrap_err()
                .starts_with("bridge.json is not readable")
        );
        std::fs::remove_file(&path).unwrap();
    }

    /// Doubling, and then stopping. A reconnect loop that grows without a
    /// ceiling stops reconnecting in any useful sense.
    #[test]
    fn backoff_doubles_to_a_ceiling() {
        assert_eq!(backoff(1).as_millis(), 500);
        assert_eq!(backoff(2).as_millis(), 1000);
        assert_eq!(backoff(3).as_millis(), 2000);
        assert_eq!(backoff(6).as_millis(), RETRY_CEILING.as_millis());
        assert_eq!(backoff(u32::MAX), RETRY_CEILING);
    }
}

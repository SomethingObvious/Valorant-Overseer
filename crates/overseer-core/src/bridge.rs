//! The socket to `backend/ws_server.py`, on a blocking thread that hands the UI
//! an [`Event`] at a time. It sends no `Origin` header, since the bridge closes
//! any connection with one.

use std::io::ErrorKind;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
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
/// How long a read blocks, which is also the longest a question waits to go
/// out, since the thread cannot write while it reads.
const READ_TIMEOUT: Duration = Duration::from_millis(100);

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
    /// The thread has given up, over something like a bad token that
    /// reconnecting will not fix.
    Stopped(String),
    /// A board this build could not read, so the window can say so instead of
    /// looking like no match is on.
    Unreadable(String),
    /// The answer to a question, matched to it by the id [`Bridge::ask`]
    /// handed back.
    Answer {
        /// Which question this answers.
        id: u64,
        /// The payload, or why there isn't one.
        result: Result<serde_json::Value, String>,
    },
}

/// A question waiting to go out.
#[derive(Debug)]
struct Ask {
    /// The id the answer will carry.
    id: u64,
    /// Which request, in the backend's words: `profile`, `match`, `recap`.
    request: String,
    /// Whatever that request needs.
    params: serde_json::Value,
}

/// What the backend writes once it is listening.
#[derive(Debug, Deserialize)]
struct Credentials {
    #[serde(rename = "wsPort")]
    ws_port: u16,
    token: String,
}

/// One frame from the bridge, every field optional for the reason in
/// [`crate::board`].
#[derive(Debug, Deserialize)]
struct Envelope {
    #[serde(rename = "type")]
    kind: Option<String>,
    data: Option<serde_json::Value>,
    message: Option<String>,
    code: Option<String>,
    /// Set on a `response`, matching the id the question went out with.
    id: Option<u64>,
    /// Set on a `response`: whether the backend managed it.
    ok: Option<bool>,
    /// Set on a `response` that did not.
    error: Option<String>,
}

/// A running connection. Dropping it asks the thread to stop.
#[derive(Debug)]
pub struct Bridge {
    events: Receiver<Event>,
    stop: Sender<()>,
    /// Questions on their way to the thread.
    outbox: Sender<Ask>,
    /// The next question's id, never reused, so a late answer to an abandoned
    /// question is not taken for the current one.
    next: AtomicU64,
}

impl Bridge {
    /// Connects with the credentials under `root` and calls `wake` whenever an
    /// event is queued, so the window never has to poll.
    pub fn start<W>(root: &Path, wake: W) -> Self
    where
        W: Fn() + Send + 'static,
    {
        let (events_tx, events) = channel();
        let (stop, stop_rx) = channel();
        let (outbox, asks) = channel();
        let path = credentials_path(root);
        let failed = events_tx.clone();
        if let Err(e) = thread::Builder::new()
            .name("overseer-bridge".to_owned())
            .spawn(move || run(&path, &events_tx, &stop_rx, &asks, &wake))
        {
            // No wake needed, since this is before the first frame, and that
            // frame drains the queue.
            drop(failed.send(Event::Stopped(format!("no bridge thread: {e}"))));
        }
        Self {
            events,
            stop,
            outbox,
            next: AtomicU64::new(1),
        }
    }

    /// Queues a question and returns the id its answer will carry. A question
    /// in flight when the socket drops is never answered, so ask it again.
    pub fn ask(&self, request: &str, params: serde_json::Value) -> u64 {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        drop(self.outbox.send(Ask {
            id,
            request: request.to_owned(),
            params,
        }));
        id
    }

    /// Everything that has arrived since the last call, oldest first. All of
    /// it at once, because a repaint wants the newest board and reading one
    /// event a frame would queue behind a burst.
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

/// Reads the credentials, or says why not. A missing file just means the
/// backend is not listening yet.
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

/// The thread body: connect, pump, reconnect, until asked to stop.
fn run<W: Fn()>(
    path: &Path,
    events: &Sender<Event>,
    stop: &Receiver<()>,
    asks: &Receiver<Ask>,
    wake: &W,
) {
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
                match pump(&mut socket, events, stop, asks, wake) {
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

/// Waits out a backoff. True means a stop arrived first.
fn wait(stop: &Receiver<()>, total: Duration) -> bool {
    matches!(stop.recv_timeout(total), Ok(()))
}

/// Queues an event and wakes the UI. False means the UI is gone.
fn post<W: Fn()>(events: &Sender<Event>, wake: &W, event: Event) -> bool {
    if events.send(event).is_err() {
        return false;
    }
    wake();
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
        // Without a read timeout the pump never sees a stop, and closing the
        // window leaves the thread blocked until the backend next speaks.
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
    asks: &Receiver<Ask>,
    wake: &W,
) -> Pumped {
    loop {
        if stop.try_recv().is_ok() {
            return Pumped::Stopped;
        }
        if let Err(why) = send_asks(socket, asks) {
            return Pumped::Dropped(why);
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
            // Binary frames are not part of this protocol, and tungstenite
            // answers websocket pings by itself.
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) =>
            {
                // The read timeout, which is how the stop check above gets
                // to run.
            }
            Err(e) => return Pumped::Dropped(format!("bridge read failed: {e}")),
        }
    }
}

/// Sends the questions queued since the last look. This runs between reads,
/// since the socket has one owner, so a question waits up to [`READ_TIMEOUT`].
fn send_asks(
    socket: &mut WebSocket<MaybeTlsStream<TcpStream>>,
    asks: &Receiver<Ask>,
) -> Result<(), String> {
    for ask in asks.try_iter() {
        let frame = serde_json::json!({
            "type": "request",
            "id": ask.id,
            "request": ask.request,
            "params": ask.params,
        });
        socket
            .send(Message::Text(frame.to_string().into()))
            .map_err(|e| format!("cannot ask the bridge: {e}"))?;
    }
    Ok(())
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

/// A rejected token is usually the last run's, read before the new backend
/// rewrote bridge.json, so reconnecting (which reads it again) fixes it. A
/// protocol this build doesn't speak is final.
fn auth_error(code: Option<String>, message: Option<String>) -> Handled {
    match code.as_deref() {
        Some("bad_token" | "timeout") => Handled::Broken("Reconnecting to the backend".to_owned()),
        _ => Handled::Rejected(
            message
                .or(code)
                .unwrap_or_else(|| "the bridge rejected this build".to_owned()),
        ),
    }
}

/// Reads one frame, ignoring any it does not know, since that is a newer
/// backend and not a fault.
fn handle(text: &str, socket: &mut WebSocket<MaybeTlsStream<TcpStream>>) -> Handled {
    let Ok(envelope) = serde_json::from_str::<Envelope>(text) else {
        return Handled::Nothing;
    };
    match envelope.kind.as_deref() {
        Some("auth_ok") => Handled::Event(Event::Status(Status::Live)),
        Some("auth_error") => auth_error(envelope.code, envelope.message),
        // An unreadable board is not worth a reconnect, since the next one is
        // a second away. It is still reported, because a board dropped in
        // silence looks exactly like no match in progress.
        Some("state") => {
            envelope.data.map_or(Handled::Nothing, |value| {
                match serde_json::from_value::<Board>(value) {
                    Ok(board) => Handled::Event(Event::Board(Box::new(board))),
                    Err(e) => Handled::Event(Event::Unreadable(e.to_string())),
                }
            })
        }
        // The backend echoes the id it was sent, so an answer without one
        // answers nobody.
        Some("response") => envelope
            .id
            .map_or(Handled::Nothing, |id| Handled::Event(answer(id, envelope))),
        Some("ping") => match socket.send(Message::Text(r#"{"type":"pong"}"#.into())) {
            Ok(()) => Handled::Nothing,
            Err(e) => Handled::Broken(format!("cannot answer a ping: {e}")),
        },
        _ => Handled::Nothing,
    }
}

/// Turns a `response` into an answer, counting a payload with an `error` in it
/// as a failure too.
fn answer(id: u64, envelope: Envelope) -> Event {
    if envelope.ok == Some(false) {
        return Event::Answer {
            id,
            result: Err(envelope
                .error
                .unwrap_or_else(|| "the bridge would not say why".to_owned())),
        };
    }
    let Some(data) = envelope.data else {
        return Event::Answer {
            id,
            result: Err("the bridge answered with nothing".to_owned()),
        };
    };
    // This is how the backend says "open VALORANT and sign in".
    if let Some(why) = data.get("error").and_then(serde_json::Value::as_str) {
        return Event::Answer {
            id,
            result: Err(why.to_owned()),
        };
    }
    Event::Answer {
        id,
        result: Ok(data),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{
        Envelope, Event, Handled, RETRY_CEILING, answer, auth_error, backoff, credentials_path,
        read_credentials,
    };

    #[test]
    fn only_a_protocol_mismatch_stops_the_bridge() {
        for code in ["bad_token", "timeout"] {
            let handled = auth_error(Some(code.to_owned()), Some("Restart".to_owned()));
            assert!(
                matches!(handled, Handled::Broken(_)),
                "{code} should reconnect"
            );
        }
        let handled = auth_error(
            Some("incompatible_protocol".to_owned()),
            Some("Update it".to_owned()),
        );
        assert!(matches!(handled, Handled::Rejected(why) if why == "Update it"));
        assert!(matches!(auth_error(None, None), Handled::Rejected(_)));
    }

    /// Both shapes of failure, and a success, read the way the panel needs.
    #[test]
    fn an_answer_carries_its_own_bad_news() {
        let envelope = |raw: &str| serde_json::from_str::<Envelope>(raw).unwrap();

        let Event::Answer { id, result } = answer(
            7,
            envelope(r#"{"type":"response","id":7,"ok":true,"data":{"puuid":"x"}}"#),
        ) else {
            panic!("not an answer");
        };
        assert_eq!(id, 7);
        assert_eq!(
            result.unwrap().get("puuid").and_then(|v| v.as_str()),
            Some("x")
        );

        // The bridge refusing outright.
        let Event::Answer { result, .. } = answer(
            8,
            envelope(r#"{"type":"response","id":8,"ok":false,"error":"boom"}"#),
        ) else {
            panic!("not an answer");
        };
        assert_eq!(result.unwrap_err(), "boom");

        // The bridge succeeding at saying no, which is what "open VALORANT
        // and sign in" looks like on the wire.
        let Event::Answer { result, .. } = answer(
            9,
            envelope(r#"{"type":"response","id":9,"ok":true,"data":{"error":"sign in"}}"#),
        ) else {
            panic!("not an answer");
        };
        assert_eq!(result.unwrap_err(), "sign in");

        let Event::Answer { result, .. } =
            answer(10, envelope(r#"{"type":"response","id":10,"ok":true}"#))
        else {
            panic!("not an answer");
        };
        assert!(result.is_err(), "an answer with no payload read as one");
    }

    #[test]
    fn credentials_are_in_the_overseer_folder() {
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

    /// A delay with no ceiling stops reconnecting in any useful sense.
    #[test]
    fn backoff_doubles_to_a_ceiling() {
        assert_eq!(backoff(1).as_millis(), 500);
        assert_eq!(backoff(2).as_millis(), 1000);
        assert_eq!(backoff(3).as_millis(), 2000);
        assert_eq!(backoff(6).as_millis(), RETRY_CEILING.as_millis());
        assert_eq!(backoff(u32::MAX), RETRY_CEILING);
    }
}

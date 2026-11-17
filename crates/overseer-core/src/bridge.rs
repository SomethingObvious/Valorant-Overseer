//! The socket to `backend/ws_server.py`, on a blocking thread that hands the UI
//! an [`Event`] at a time. It sends no `Origin` header, since the bridge closes
//! any connection with one.

use std::io::ErrorKind;
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::time::{Duration, Instant};

use serde::Deserialize;
use tungstenite::{Message, WebSocket, client::IntoClientRequest};

use crate::board::Board;

const SPEAKS: u32 = 2;

const RETRY_BASE: Duration = Duration::from_millis(500);
const RETRY_CEILING: Duration = Duration::from_secs(10);
/// How long a read blocks, which is also the longest a question waits to go
/// out, since the thread cannot write while it reads.
const READ_TIMEOUT: Duration = Duration::from_millis(100);
/// How long the backend gets to take the socket, and then to say the token
/// is good. A `bridge.json` left by the last run can name a port some other
/// program has since taken, and one that never answers would otherwise hold
/// this thread for good, with the new backend's file never read.
const GREETING: Duration = Duration::from_secs(5);
/// How long a live bridge may say nothing at all before it counts as gone.
/// The backend pings every 30 seconds whether or not a board changed, so
/// this is two missed pings and a slow one. A backend that froze with its
/// socket open would otherwise leave the window showing it as connected.
#[cfg(not(test))]
const SILENCE: Duration = Duration::from_secs(75);
#[cfg(test)]
const SILENCE: Duration = Duration::from_secs(2);

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

#[derive(Debug)]
struct Ask {
    id: u64,
    request: String,
    params: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct Credentials {
    #[serde(rename = "wsPort")]
    ws_port: u16,
    token: String,
}

#[derive(Debug, Deserialize)]
struct Envelope {
    kind: Option<String>,
    board: Option<serde_json::Value>,
    why: Option<String>,
    say: Option<String>,
    n: Option<u64>,
    answer: Option<serde_json::Value>,
    failed: Option<String>,
}

/// A running connection. Dropping it asks the thread to stop.
#[derive(Debug)]
pub struct Bridge {
    events: Receiver<Event>,
    stop: Sender<()>,
    /// Questions on their way to the thread.
    outbox: Sender<Ask>,
}

static NEXT: AtomicU64 = AtomicU64::new(1);

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
            drop(failed.send(Event::Stopped(format!("no bridge thread: {e}"))));
        }
        Self {
            events,
            stop,
            outbox,
        }
    }

    /// Queues a question and returns the id its answer will carry. A question
    /// in flight when the socket drops is never answered, so ask it again.
    pub fn ask(&self, request: &str, params: serde_json::Value) -> u64 {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
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
    let mut failing: Option<String> = None;
    loop {
        if stop.try_recv().is_ok() {
            return;
        }
        let status = if attempt == 0 {
            Status::Connecting(String::new())
        } else {
            Status::Connecting(format!("retry {attempt}"))
        };
        let sent = if failing.is_some() {
            events.send(Event::Status(status)).is_ok()
        } else {
            post(events, wake, Event::Status(status))
        };
        if !sent {
            return;
        }

        match connect(path) {
            Ok(mut socket) => {
                attempt = 0;
                failing = None;
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
                let again = failing.as_ref() == Some(&why);
                failing = Some(why.clone());
                let lost = Event::Status(Status::Lost(why));
                if !(if again {
                    events.send(lost).is_ok()
                } else {
                    post(events, wake, lost)
                }) {
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

fn backoff(n: u32) -> Duration {
    let shift = n.saturating_sub(1).min(5);
    let scaled = RETRY_BASE.saturating_mul(1_u32 << shift);
    scaled.min(RETRY_CEILING)
}

fn wait(stop: &Receiver<()>, total: Duration) -> bool {
    matches!(stop.recv_timeout(total), Ok(()))
}

fn post<W: Fn()>(events: &Sender<Event>, wake: &W, event: Event) -> bool {
    if events.send(event).is_err() {
        return false;
    }
    wake();
    true
}

fn connect(path: &Path) -> Result<WebSocket<TcpStream>, String> {
    let creds = read_credentials(path)?;
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, creds.ws_port));
    let request = format!("ws://{address}")
        .into_client_request()
        .map_err(|e| format!("bad bridge address: {e}"))?;
    let unanswered = |e: std::io::Error| format!("bridge not answering: {e}");
    let stream = TcpStream::connect_timeout(&address, GREETING).map_err(unanswered)?;
    stream.set_nodelay(true).map_err(unanswered)?;
    stream
        .set_read_timeout(Some(GREETING))
        .map_err(unanswered)?;
    let (mut socket, _response) =
        tungstenite::client(request, stream).map_err(|e| format!("bridge not answering: {e}"))?;

    // Without a read timeout the pump never sees a stop, and closing the
    // window leaves the thread blocked until the backend next speaks.
    socket
        .get_ref()
        .set_read_timeout(Some(READ_TIMEOUT))
        .map_err(|e| format!("cannot set a read timeout: {e}"))?;

    let hello = serde_json::json!({ "kind": "hello", "key": creds.token, "speaks": SPEAKS });
    socket
        .send(Message::Text(hello.to_string().into()))
        .map_err(|e| format!("cannot greet the bridge: {e}"))?;
    Ok(socket)
}

enum Pumped {
    /// The UI asked to stop.
    Stopped,
    /// The bridge refused us. Retrying will not help.
    Rejected(String),
    /// The connection went away. Retrying will help.
    Dropped(String),
}

fn pump<W: Fn()>(
    socket: &mut WebSocket<TcpStream>,
    events: &Sender<Event>,
    stop: &Receiver<()>,
    asks: &Receiver<Ask>,
    wake: &W,
) -> Pumped {
    let mut live = false;
    let greeted = Instant::now();
    let mut heard = Instant::now();
    loop {
        if stop.try_recv().is_ok() {
            return Pumped::Stopped;
        }
        if !live && greeted.elapsed() > GREETING {
            return Pumped::Dropped("the backend didn't answer".to_owned());
        }
        if live && heard.elapsed() > SILENCE {
            return Pumped::Dropped("the backend stopped answering".to_owned());
        }
        if live && let Err(why) = send_asks(socket, asks) {
            return Pumped::Dropped(why);
        }
        let read = socket.read();
        if read.is_ok() {
            heard = Instant::now();
        }
        match read {
            Ok(Message::Text(text)) => match handle(&text) {
                Handled::Nothing => {}
                Handled::Event(event) => {
                    if matches!(event, Event::Status(Status::Live)) {
                        // The window counts everything it asked while the
                        // socket was down as lost, so those questions are
                        // dropped, not sent late. Sent, an Appear Offline
                        // pressed minutes ago would restart the game.
                        asks.try_iter().for_each(drop);
                        live = true;
                    }
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
                if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(e) => return Pumped::Dropped(format!("bridge read failed: {e}")),
        }
    }
}

fn send_asks(socket: &mut WebSocket<TcpStream>, asks: &Receiver<Ask>) -> Result<(), String> {
    for ask in asks.try_iter() {
        let frame = serde_json::json!({
            "kind": "ask",
            "n": ask.id,
            "what": ask.request,
            "with": ask.params,
        });
        socket
            .send(Message::Text(frame.to_string().into()))
            .map_err(|e| format!("cannot ask the bridge: {e}"))?;
    }
    Ok(())
}

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

fn refused(why: Option<String>, say: Option<String>) -> Handled {
    match why.as_deref() {
        Some("key" | "late") => Handled::Broken("Reconnecting to the backend".to_owned()),
        _ => Handled::Rejected(
            say.or(why)
                .unwrap_or_else(|| "the bridge turned this build away".to_owned()),
        ),
    }
}

fn handle(text: &str) -> Handled {
    let Ok(envelope) = serde_json::from_str::<Envelope>(text) else {
        return Handled::Nothing;
    };
    match envelope.kind.as_deref() {
        Some("welcome") => Handled::Event(Event::Status(Status::Live)),
        Some("refused") => refused(envelope.why, envelope.say),
        Some("board") => envelope.board.map_or(Handled::Nothing, |value| {
            match serde_json::from_value::<Board>(value) {
                Ok(board) => Handled::Event(Event::Board(Box::new(board))),
                Err(e) => Handled::Event(Event::Unreadable(e.to_string())),
            }
        }),
        Some("reply") => envelope
            .n
            .map_or(Handled::Nothing, |id| Handled::Event(answer(id, envelope))),
        _ => Handled::Nothing,
    }
}

fn answer(id: u64, envelope: Envelope) -> Event {
    if let Some(why) = envelope.failed {
        return Event::Answer {
            id,
            result: Err(why),
        };
    }
    let Some(data) = envelope.answer else {
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
    use super::{
        Envelope, Event, GREETING, Handled, SILENCE, Status, answer, connect, read_credentials,
        refused,
    };

    #[test]
    fn only_a_version_mismatch_stops_the_bridge() {
        for why in ["key", "late"] {
            let handled = refused(Some(why.to_owned()), Some("Restart".to_owned()));
            assert!(
                matches!(handled, Handled::Broken(_)),
                "{why} should reconnect"
            );
        }
        let handled = refused(Some("speaks".to_owned()), Some("Run setup".to_owned()));
        assert!(matches!(handled, Handled::Rejected(why) if why == "Run setup"));
        assert!(matches!(refused(None, None), Handled::Rejected(_)));
    }

    #[test]
    fn an_answer_carries_its_own_bad_news() {
        let envelope = |raw: &str| serde_json::from_str::<Envelope>(raw).unwrap();

        let Event::Answer { id, result } = answer(
            7,
            envelope(r#"{"kind":"reply","n":7,"answer":{"puuid":"x"}}"#),
        ) else {
            panic!("not an answer");
        };
        assert_eq!(id, 7);
        assert_eq!(
            result.unwrap().get("puuid").and_then(|v| v.as_str()),
            Some("x")
        );

        let Event::Answer { result, .. } =
            answer(8, envelope(r#"{"kind":"reply","n":8,"failed":"boom"}"#))
        else {
            panic!("not an answer");
        };
        assert_eq!(result.unwrap_err(), "boom");

        // The bridge succeeding at saying no, which is what "open VALORANT
        // and sign in" looks like on the wire.
        let Event::Answer { result, .. } = answer(
            9,
            envelope(r#"{"kind":"reply","n":9,"answer":{"error":"sign in"}}"#),
        ) else {
            panic!("not an answer");
        };
        assert_eq!(result.unwrap_err(), "sign in");

        let Event::Answer { result, .. } = answer(10, envelope(r#"{"kind":"reply","n":10}"#))
        else {
            panic!("not an answer");
        };
        assert!(result.is_err(), "an answer with no payload read as one");
    }

    /// A port that takes the socket and then says nothing is given up on, so
    /// the thread goes back to read the file the new backend writes.
    #[test]
    fn a_port_that_never_answers_is_given_up_on() {
        let silent = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = silent.local_addr().unwrap().port();
        let dir = std::env::temp_dir().join("overseer-bridge-silent");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bridge.json");
        std::fs::write(&path, format!(r#"{{"wsPort":{port},"token":"stale"}}"#)).unwrap();

        let began = std::time::Instant::now();
        let refused = connect(&path);
        drop(std::fs::remove_dir_all(&dir));
        assert!(refused.unwrap_err().starts_with("bridge not answering"));
        assert!(
            began.elapsed() < GREETING * 2,
            "it waited {:?}",
            began.elapsed()
        );
    }

    #[test]
    fn a_bridge_that_goes_quiet_is_dropped() {
        let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = server.local_addr().unwrap().port();
        let root = std::env::temp_dir().join(format!("overseer-quiet-{}", std::process::id()));
        std::fs::create_dir_all(root.join(".overseer")).unwrap();
        std::fs::write(
            root.join(".overseer").join("bridge.json"),
            format!(r#"{{"wsPort":{port},"token":"t"}}"#),
        )
        .unwrap();
        let (done, finished) = std::sync::mpsc::channel::<()>();
        let quiet = std::thread::spawn(move || {
            let (stream, _) = server.accept().unwrap();
            let mut socket = tungstenite::accept(stream).unwrap();
            let _hello = socket.read().unwrap();
            socket
                .send(tungstenite::Message::Text(r#"{"kind":"welcome"}"#.into()))
                .unwrap();
            let _done = finished.recv_timeout(SILENCE * 3);
        });

        let (woke, wakes) = std::sync::mpsc::channel::<()>();
        let bridge = super::Bridge::start(&root, move || {
            let _sent = woke.send(());
        });
        let until = std::time::Instant::now() + SILENCE * 3;
        let (mut live, mut why) = (false, None);
        while why.is_none() {
            let Some(left) = until.checked_duration_since(std::time::Instant::now()) else {
                break;
            };
            let _woken = wakes.recv_timeout(left);
            for event in bridge.drain() {
                match event {
                    Event::Status(Status::Live) => live = true,
                    Event::Status(Status::Lost(lost)) if live => why = Some(lost),
                    _ => {}
                }
            }
        }
        let _told = done.send(());
        drop(bridge);
        quiet.join().unwrap();
        drop(std::fs::remove_dir_all(&root));
        assert!(live, "it was welcomed");
        assert_eq!(why.as_deref(), Some("the backend stopped answering"));
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
}

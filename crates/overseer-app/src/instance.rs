//! One window at a time.
//!
//! The first launch listens on a fixed local port, and a second one knocks,
//! brings the first to the front and exits. The knock has to be answered
//! with the right word, so a program that happens to own the port is never
//! mistaken for us, and the window just runs unguarded.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Where the first window listens.
const PORT: u16 = 47_873;
/// What a second launch sends.
const KNOCK: &[u8; 4] = b"show";
/// What the first window answers, which tells the knock it reached us.
const ANSWER: &[u8; 8] = b"overseer";

/// How this launch starts.
pub(crate) enum Start {
    /// The only window: keep this and listen on it.
    First(TcpListener),
    /// Another window was already open and has been told to show itself.
    Second,
    /// The port belongs to something else, so run as if alone.
    Unguarded,
}

/// Claims the port, or knocks on whoever holds it.
pub(crate) fn claim() -> Start {
    let at = SocketAddr::from((Ipv4Addr::LOCALHOST, PORT));
    if let Ok(listener) = TcpListener::bind(at) {
        return Start::First(listener);
    }
    let answered = TcpStream::connect_timeout(&at, Duration::from_millis(500))
        .and_then(|mut stream| {
            stream.set_read_timeout(Some(Duration::from_millis(800)))?;
            stream.write_all(KNOCK)?;
            let mut reply = [0_u8; 8];
            stream.read_exact(&mut reply)?;
            Ok(&reply == ANSWER)
        })
        .unwrap_or(false);
    if answered {
        Start::Second
    } else {
        Start::Unguarded
    }
}

/// Answers knocks for as long as the window runs, raising `knocked` and
/// calling `wake` for each.
pub(crate) fn listen(
    listener: TcpListener,
    knocked: Arc<AtomicBool>,
    wake: impl Fn() + Send + 'static,
) {
    let _listening = std::thread::Builder::new()
        .name("overseer-instance".to_owned())
        .spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let _timed = stream.set_read_timeout(Some(Duration::from_millis(800)));
                let mut knock = [0_u8; 4];
                if stream.read_exact(&mut knock).is_ok() && &knock == KNOCK {
                    let _answered = stream.write_all(ANSWER);
                    knocked.store(true, Ordering::Relaxed);
                    wake();
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    use super::{Start, claim, listen};

    /// The first launch holds the port, a second is told to leave, and the
    /// first hears about it.
    #[test]
    fn a_second_launch_brings_the_first_forward() {
        let Start::First(listener) = claim() else {
            // A real window, or something else, already holds the port.
            return;
        };
        let knocked = Arc::new(AtomicBool::new(false));
        let (woke, woken) = std::sync::mpsc::channel();
        listen(listener, Arc::clone(&knocked), move || {
            let _sent = woke.send(());
        });
        assert!(matches!(claim(), Start::Second));
        assert!(woken.recv_timeout(Duration::from_secs(2)).is_ok());
        assert!(knocked.load(Ordering::Relaxed));
    }
}

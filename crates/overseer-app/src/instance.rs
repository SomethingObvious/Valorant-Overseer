use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const PORT: u16 = 47_873;
const KNOCK: &[u8; 4] = b"show";
const ANSWER: &[u8; 8] = b"overseer";

pub(crate) enum Start {
    First(TcpListener),
    Second,
    Unguarded,
}

pub(crate) fn claim() -> Start {
    let at = SocketAddr::from((Ipv4Addr::LOCALHOST, PORT));
    if let Ok(listener) = TcpListener::bind(at) {
        return Start::First(listener);
    }
    let answered = TcpStream::connect_timeout(&at, Duration::from_millis(500))
        .and_then(|mut stream| {
            // The first launch answers once its window is made, which on a
            // slow PC is seconds after it took the port, and a second click
            // in that time would otherwise open a second window.
            stream.set_read_timeout(Some(Duration::from_secs(10)))?;
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

    #[test]
    fn a_second_launch_brings_the_first_forward() {
        let Start::First(listener) = claim() else {
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

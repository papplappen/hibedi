//! Native transport: a background thread speaking JSON over a WebSocket
//! to the relay server (crates/server). The relay assigns our seat and
//! forwards state announcements between the two seats.

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use hibedi_core::{ServerMessage, StateSync};

use super::NetEvent;

const READ_POLL: Duration = Duration::from_millis(100);
const INITIAL_BACKOFF: Duration = Duration::from_millis(500);
const MAX_BACKOFF: Duration = Duration::from_secs(5);

pub struct NetHandle {
    syncs: Sender<StateSync>,
    events: Receiver<NetEvent>,
}

impl NetHandle {
    pub fn poll(&self) -> Option<NetEvent> {
        self.events.try_recv().ok()
    }

    /// Publishes a state announcement; the relay forwards it to the other
    /// seat. The `peer` targeting of the wasm transport does not exist
    /// here: the relay always knows both seats.
    pub fn announce(&self, sync: &StateSync, _peer: Option<&str>) {
        let _ = self.syncs.send(sync.clone());
    }
}

pub fn spawn(url: String) -> NetHandle {
    let (sync_tx, sync_rx) = mpsc::channel();
    let (event_tx, event_rx) = mpsc::channel();

    thread::spawn(move || run(url, sync_rx, event_tx));

    NetHandle {
        syncs: sync_tx,
        events: event_rx,
    }
}

fn run(url: String, sync_rx: Receiver<StateSync>, event_tx: mpsc::Sender<NetEvent>) {
    let mut backoff = INITIAL_BACKOFF;

    loop {
        match tungstenite::connect(&url) {
            Ok((mut socket, _)) => {
                if let tungstenite::stream::MaybeTlsStream::Plain(stream) = socket.get_mut() {
                    let _ = stream.set_read_timeout(Some(READ_POLL));
                    let _ = stream.set_nodelay(true);
                }

                backoff = INITIAL_BACKOFF;
                if matches!(pump(&mut socket, &sync_rx, &event_tx), Flow::Stop) {
                    // Rejected by the relay: there is no seat to retry for.
                    return;
                }
            }
            Err(_) => {
                let _ = event_tx.send(NetEvent::Disconnected);
            }
        }

        thread::sleep(backoff);
        backoff = next_backoff(backoff);
    }
}

enum Flow {
    Continue,
    Stop,
}

/// One connected session: relays our announcements out and the server's
/// messages in until the connection drops.
fn pump(
    socket: &mut tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>,
    sync_rx: &Receiver<StateSync>,
    event_tx: &mpsc::Sender<NetEvent>,
) -> Flow {
    loop {
        while let Ok(sync) = sync_rx.try_recv() {
            if send_json(socket, &ServerMessage::Sync(sync)).is_err() {
                return drop_connection(event_tx);
            }
        }

        match socket.read() {
            Ok(tungstenite::Message::Text(text)) => {
                match serde_json::from_str::<ServerMessage>(text.as_str()) {
                    Ok(message) => {
                        #[cfg(debug_assertions)]
                        eprintln!("relay in: {message:?}");
                        match message {
                            ServerMessage::Hello { side } => {
                                let _ = event_tx.send(NetEvent::Connected { side });
                            }
                            ServerMessage::PeerJoined => {
                                let _ = event_tx.send(NetEvent::PeerJoined { peer: None });
                            }
                            ServerMessage::PeerLeft => {
                                let _ = event_tx.send(NetEvent::PeerLeft { peer: None });
                            }
                            ServerMessage::Sync(sync) => {
                                let _ = event_tx.send(NetEvent::Sync(sync));
                            }
                            ServerMessage::Full => {
                                let _ = event_tx.send(NetEvent::Closed {
                                    reason: "the relay server is full".to_owned(),
                                });
                                return Flow::Stop;
                            }
                        }
                    }
                    Err(_) => {}
                }
            }
            Ok(tungstenite::Message::Close(_)) => return drop_connection(event_tx),
            Err(tungstenite::Error::Io(error))
                if error.kind() == std::io::ErrorKind::WouldBlock
                    || error.kind() == std::io::ErrorKind::TimedOut => {}
            Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                return drop_connection(event_tx)
            }
            Err(_) => return drop_connection(event_tx),
            Ok(_) => {}
        }
    }
}

fn drop_connection(event_tx: &mpsc::Sender<NetEvent>) -> Flow {
    let _ = event_tx.send(NetEvent::Disconnected);
    Flow::Continue
}

fn next_backoff(current: Duration) -> Duration {
    (current.saturating_mul(2)).min(MAX_BACKOFF)
}

fn send_json(
    socket: &mut tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>,
    message: &ServerMessage,
) -> Result<(), tungstenite::Error> {
    let json = serde_json::to_string(message).expect("relay messages always serialize");
    socket.send(tungstenite::Message::Text(json.into()))
}

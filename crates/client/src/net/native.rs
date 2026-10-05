//! Native transport: a background thread speaking JSON over a WebSocket
//! to the dedicated server (crates/server).

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use hibedi_core::ClientMessage;

use super::NetEvent;

const READ_POLL: Duration = Duration::from_millis(100);
const INITIAL_BACKOFF: Duration = Duration::from_millis(500);
const MAX_BACKOFF: Duration = Duration::from_secs(5);

pub struct NetHandle {
    commands: Sender<ClientMessage>,
    events: Receiver<NetEvent>,
}

impl NetHandle {
    pub fn poll(&self) -> Option<NetEvent> {
        self.events.try_recv().ok()
    }

    pub fn send(&self, message: ClientMessage) {
        let _ = self.commands.send(message);
    }
}

pub fn spawn(url: String) -> NetHandle {
    let (command_tx, command_rx) = mpsc::channel();
    let (event_tx, event_rx) = mpsc::channel();

    thread::spawn(move || run(url, command_rx, event_tx));

    NetHandle {
        commands: command_tx,
        events: event_rx,
    }
}

fn run(url: String, command_rx: Receiver<ClientMessage>, event_tx: mpsc::Sender<NetEvent>) {
    let mut backoff = INITIAL_BACKOFF;

    loop {
        match tungstenite::connect(&url) {
            Ok((mut socket, _)) => {
                if let tungstenite::stream::MaybeTlsStream::Plain(stream) = socket.get_mut() {
                    let _ = stream.set_read_timeout(Some(READ_POLL));
                    let _ = stream.set_nodelay(true);
                }

                // The server seats us the moment we connect; no hello is
                // exchanged.
                backoff = INITIAL_BACKOFF;
                let _ = event_tx.send(NetEvent::Connected);

                'connected: loop {
                    while let Ok(message) = command_rx.try_recv() {
                        if send_json(&mut socket, &message).is_err() {
                            break 'connected;
                        }
                    }

                    match socket.read() {
                        Ok(tungstenite::Message::Text(text)) => {
                            if let Ok(message) =
                                serde_json::from_str::<hibedi_core::ServerMessage>(text.as_str())
                            {
                                let _ = event_tx.send(NetEvent::Message(message));
                            }
                        }
                        Ok(tungstenite::Message::Close(_)) => break 'connected,
                        Err(tungstenite::Error::Io(error))
                            if error.kind() == std::io::ErrorKind::WouldBlock
                                || error.kind() == std::io::ErrorKind::TimedOut => {}
                        Err(
                            tungstenite::Error::ConnectionClosed
                            | tungstenite::Error::AlreadyClosed,
                        ) => break 'connected,
                        Err(_) => break 'connected,
                        Ok(_) => {}
                    }
                }

                let _ = event_tx.send(NetEvent::Disconnected);
            }
            Err(_) => {
                let _ = event_tx.send(NetEvent::Disconnected);
            }
        }

        thread::sleep(backoff);
        backoff = next_backoff(backoff);
    }
}

fn next_backoff(current: Duration) -> Duration {
    (current.saturating_mul(2)).min(MAX_BACKOFF)
}

fn send_json(
    socket: &mut tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>,
    message: &ClientMessage,
) -> Result<(), tungstenite::Error> {
    let json = serde_json::to_string(message).expect("client messages always serialize");
    socket.send(tungstenite::Message::Text(json.into()))
}

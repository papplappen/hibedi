use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use hibedi_core::{ClientMessage, ServerMessage};
use tungstenite::stream::MaybeTlsStream;

const READ_POLL: Duration = Duration::from_millis(100);
const INITIAL_BACKOFF: Duration = Duration::from_millis(500);
const MAX_BACKOFF: Duration = Duration::from_secs(5);

#[allow(clippy::large_enum_variant)]
pub enum NetEvent {
    Connected,
    Disconnected,
    Message(ServerMessage),
}

pub enum NetCommand {
    Send(ClientMessage),
    SetToken(Option<String>),
}

pub struct NetHandle {
    commands: Sender<NetCommand>,
    pub events: Receiver<NetEvent>,
}

impl NetHandle {
    pub fn send(&self, message: ClientMessage) {
        let _ = self.commands.send(NetCommand::Send(message));
    }

    pub fn set_token(&self, token: Option<String>) {
        let _ = self.commands.send(NetCommand::SetToken(token));
    }
}

pub fn spawn(url: String, initial_token: Option<String>) -> NetHandle {
    let (command_tx, command_rx) = mpsc::channel();
    let (event_tx, event_rx) = mpsc::channel();

    thread::spawn(move || run(url, initial_token, command_rx, event_tx));

    NetHandle {
        commands: command_tx,
        events: event_rx,
    }
}

fn run(
    url: String,
    mut token: Option<String>,
    command_rx: Receiver<NetCommand>,
    event_tx: Sender<NetEvent>,
) {
    let mut backoff = INITIAL_BACKOFF;

    loop {
        match tungstenite::connect(&url) {
            Ok((mut socket, _)) => {
                if let MaybeTlsStream::Plain(stream) = socket.get_mut() {
                    let _ = stream.set_read_timeout(Some(READ_POLL));
                    let _ = stream.set_nodelay(true);
                }

                if send_json(
                    &mut socket,
                    &ClientMessage::Hello {
                        token: token.clone(),
                    },
                )
                .is_err()
                {
                    let _ = event_tx.send(NetEvent::Disconnected);
                    thread::sleep(backoff);
                    backoff = next_backoff(backoff);
                    continue;
                }

                backoff = INITIAL_BACKOFF;
                let _ = event_tx.send(NetEvent::Connected);

                'connected: loop {
                    while let Ok(command) = command_rx.try_recv() {
                        match command {
                            NetCommand::SetToken(new_token) => token = new_token,
                            NetCommand::Send(message) => {
                                if send_json(&mut socket, &message).is_err() {
                                    break 'connected;
                                }
                            }
                        }
                    }

                    match socket.read() {
                        Ok(tungstenite::Message::Text(text)) => {
                            if let Ok(message) =
                                serde_json::from_str::<ServerMessage>(text.as_str())
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
    socket: &mut tungstenite::WebSocket<MaybeTlsStream<std::net::TcpStream>>,
    message: &ClientMessage,
) -> Result<(), tungstenite::Error> {
    let json = serde_json::to_string(message).expect("client messages always serialize");
    socket.send(tungstenite::Message::Text(json.into()))
}

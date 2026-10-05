//! Native multiplayer through the relay server (crates/server): the
//! native counterpart of the web page's rooms. [`join`] waits, before
//! the game starts, until every seat is taken; the [`RelaySession`] it
//! returns then syncs game states.
//!
//! A background thread owns the WebSocket and our copy of the shared,
//! versioned state (see the protocol crate). It versions what we
//! publish, adopts newer states from the relay, and after reconnecting
//! reclaims our seat and re-offers the state it holds, so a reconnecting
//! client and the relay bring each other up to date.

use std::io::ErrorKind;
use std::net::TcpStream;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::Duration;

use hibedi_relay_protocol::{ClientMessage, ServerMessage};
use macroquad::window::next_frame;
use serde_json::Value;
use tungstenite::stream::MaybeTlsStream;

use crate::session::Session;
use crate::ui;

/// How long the connection thread waits for an outgoing state before it
/// checks the socket again; bounds the latency of incoming messages.
const POLL: Duration = Duration::from_millis(5);
const INITIAL_BACKOFF: Duration = Duration::from_millis(500);
const MAX_BACKOFF: Duration = Duration::from_secs(5);

type Socket = tungstenite::WebSocket<MaybeTlsStream<TcpStream>>;

/// Connects to the relay at `url` and waits, showing a waiting screen,
/// until every seat is taken. Exits the process if the relay is full.
pub async fn join(url: String) -> RelaySession {
    let mut session = RelaySession::connect(url.clone());
    loop {
        session.process();
        if session.refused {
            eprintln!("every seat on {url} is taken");
            std::process::exit(1);
        }
        if session.everyone_online() {
            return session;
        }
        let (title, body) = match session.seat {
            Some(_) => (
                "Waiting for the other player",
                format!("Start a second client with --server {url}"),
            ),
            None => ("Connecting…", format!("Connecting to {url}")),
        };
        ui::message_screen(title, &body);
        next_frame().await;
    }
}

pub struct RelaySession {
    outbox: Sender<Value>,
    inbox: Receiver<Event>,
    /// Our seat; kept while reconnecting.
    seat: Option<usize>,
    /// Per seat, whether a player holds it; empty while disconnected.
    occupied: Vec<bool>,
    refused: bool,
    /// The latest state adopted from the relay, not yet received.
    state: Option<Value>,
}

/// Connection thread → session.
enum Event {
    Seats {
        seat: usize,
        occupied: Vec<bool>,
    },
    State(Value),
    /// The connection dropped; the thread is reconnecting.
    Lost,
    Refused,
}

impl RelaySession {
    fn connect(url: String) -> Self {
        let (outbox, outgoing) = mpsc::channel();
        let (incoming, inbox) = mpsc::channel();
        thread::spawn(move || Connection::new(incoming).run(&url, &outgoing));
        Self {
            outbox,
            inbox,
            seat: None,
            occupied: Vec::new(),
            refused: false,
            state: None,
        }
    }

    /// Applies everything the connection thread reported.
    fn process(&mut self) {
        while let Ok(event) = self.inbox.try_recv() {
            match event {
                Event::Seats { seat, occupied } => {
                    self.seat = Some(seat);
                    self.occupied = occupied;
                }
                Event::State(state) => self.state = Some(state),
                Event::Lost => self.occupied.clear(),
                Event::Refused => self.refused = true,
            }
        }
    }
}

impl Session for RelaySession {
    fn seat(&self) -> usize {
        self.seat.expect("a session is only handed out once seated")
    }

    fn receive(&mut self) -> Option<Value> {
        self.process();
        self.state.take()
    }

    fn publish(&mut self, state: Value) {
        let _ = self.outbox.send(state);
    }

    fn everyone_online(&self) -> bool {
        !self.occupied.is_empty() && self.occupied.iter().all(|&occupied| occupied)
    }
}

/// The connection thread's state, which outlives each connection.
struct Connection {
    events: Sender<Event>,
    seat: Option<usize>,
    /// Our copy of the shared state and its version.
    version: u64,
    state: Option<Value>,
}

enum Ended {
    /// The connection broke; reconnect.
    Dropped,
    /// Every seat is taken.
    Refused,
    /// The game dropped its end of the channels.
    GameClosed,
}

impl Connection {
    fn new(events: Sender<Event>) -> Self {
        Self {
            events,
            seat: None,
            version: 0,
            state: None,
        }
    }

    /// Connects, pumps one connection, reconnects.
    fn run(mut self, url: &str, outgoing: &Receiver<Value>) {
        let mut backoff = INITIAL_BACKOFF;
        loop {
            if let Ok((socket, _)) = tungstenite::connect(url) {
                backoff = INITIAL_BACKOFF;
                match self.pump(socket, outgoing) {
                    Ended::Dropped => {}
                    Ended::Refused | Ended::GameClosed => return,
                }
            }
            if self.events.send(Event::Lost).is_err() {
                return;
            }
            thread::sleep(backoff);
            backoff = (backoff * 2).min(MAX_BACKOFF);
        }
    }

    fn pump(&mut self, mut socket: Socket, outgoing: &Receiver<Value>) -> Ended {
        let mut greeting = vec![ClientMessage::Hello { seat: self.seat }];
        // Re-offer what we hold: if the relay restarted or we missed
        // nothing, it keeps ours; otherwise it answers with its newer one.
        greeting.extend(self.offer());
        if greeting
            .iter()
            .any(|message| send(&mut socket, message).is_err())
        {
            return Ended::Dropped;
        }
        if let MaybeTlsStream::Plain(stream) = socket.get_mut()
            && stream.set_nonblocking(true).is_err()
        {
            return Ended::Dropped;
        }

        loop {
            let sent = match outgoing.recv_timeout(POLL) {
                Ok(state) => {
                    self.version += 1;
                    self.state = Some(state);
                    let offer = self.offer().expect("a state was just stored");
                    send(&mut socket, &offer)
                }
                Err(RecvTimeoutError::Timeout) => socket.flush(),
                Err(RecvTimeoutError::Disconnected) => return Ended::GameClosed,
            };
            if sent.is_err_and(|error| !would_block(&error)) {
                return Ended::Dropped;
            }

            loop {
                let message = match socket.read() {
                    Ok(tungstenite::Message::Text(text)) => serde_json::from_str(text.as_str()),
                    Ok(tungstenite::Message::Close(_)) => return Ended::Dropped,
                    Ok(_) => continue,
                    Err(error) if would_block(&error) => break,
                    Err(_) => return Ended::Dropped,
                };
                let event = match message {
                    Ok(ServerMessage::Seats { seat, occupied }) => {
                        self.seat = Some(seat);
                        Event::Seats { seat, occupied }
                    }
                    // An older state is our own offer's answer arriving
                    // after we offered again: ours is still in flight.
                    Ok(ServerMessage::State { version, .. }) if version < self.version => continue,
                    Ok(ServerMessage::State { version, state }) => {
                        self.version = version;
                        self.state = Some(state.clone());
                        Event::State(state)
                    }
                    Ok(ServerMessage::Full) => {
                        let _ = self.events.send(Event::Refused);
                        return Ended::Refused;
                    }
                    Err(error) => {
                        eprintln!("ignoring malformed relay message: {error}");
                        continue;
                    }
                };
                if self.events.send(event).is_err() {
                    return Ended::GameClosed;
                }
            }
        }
    }

    /// Our state as an offer, if we hold one.
    fn offer(&self) -> Option<ClientMessage> {
        Some(ClientMessage::Offer {
            version: self.version,
            state: self.state.clone()?,
        })
    }
}

fn send(socket: &mut Socket, message: &ClientMessage) -> tungstenite::Result<()> {
    let json = serde_json::to_string(message).expect("relay messages always serialize");
    socket.send(tungstenite::Message::Text(json.into()))
}

/// A non-blocking socket only queued the write or has nothing to read
/// yet; later calls continue.
fn would_block(error: &tungstenite::Error) -> bool {
    matches!(error, tungstenite::Error::Io(io) if io.kind() == ErrorKind::WouldBlock)
}

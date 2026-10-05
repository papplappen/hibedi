//! Host-side glue for hosted rooms: runs the authoritative [`Session`]
//! inside the hosting browser and routes messages between the local
//! player and the remote guest over the JS bridge. Pure logic, so it is
//! unit-tested on native targets too.

use std::collections::{HashMap, HashSet};

use hibedi_core::session::{ClaimOutcome, ConnId, Session};
use hibedi_core::{ClientMessage, ServerMessage, Side};

use super::{BridgeCommand, NetEvent};

/// Connection id of the local (hosting) player; guests count up from here.
const LOCAL_CONN: ConnId = 0;

struct PeerSeat {
    conn: ConnId,
    side: Side,
}

pub struct HostGame {
    session: Session,
    local_side: Side,
    /// Trystero peer id → seat.
    peers: HashMap<String, PeerSeat>,
    next_conn: ConnId,
    /// Peers the session could not seat (room full); they receive an
    /// error and their further messages are dropped.
    rejected: HashSet<String>,
}

impl HostGame {
    /// Claims the local seat and delivers the initial waiting state.
    pub fn new(out: &mut Vec<BridgeCommand>) -> (Self, Vec<NetEvent>) {
        let mut session = Session::new();
        let (local_side, messages) = match session.claim(LOCAL_CONN) {
            ClaimOutcome::Seated { side, messages } => (side, messages),
            ClaimOutcome::Full => unreachable!("a fresh session always has a free seat"),
        };
        let host = Self {
            session,
            local_side,
            peers: HashMap::new(),
            next_conn: LOCAL_CONN + 1,
            rejected: HashSet::new(),
        };
        let events = host.deliver(messages, out);
        (host, events)
    }

    pub fn local_message(
        &mut self,
        message: ClientMessage,
        out: &mut Vec<BridgeCommand>,
    ) -> Vec<NetEvent> {
        let messages = self.session.handle_message(self.local_side, message);
        self.deliver(messages, out)
    }

    /// Seats a joining peer, or tells it the room is full.
    pub fn peer_joined(&mut self, peer: &str, out: &mut Vec<BridgeCommand>) -> Vec<NetEvent> {
        if self.rejected.contains(peer) || self.peers.contains_key(peer) {
            return Vec::new();
        }

        let conn = self.next_conn;
        self.next_conn += 1;
        match self.session.claim(conn) {
            ClaimOutcome::Seated { side, messages } => {
                self.peers.insert(peer.to_owned(), PeerSeat { conn, side });
                self.deliver(messages, out)
            }
            ClaimOutcome::Full => {
                self.rejected.insert(peer.to_owned());
                out.push(BridgeCommand::Server {
                    message: ServerMessage::Error {
                        reason: "the room is full".to_owned(),
                    },
                    peer: Some(peer.to_owned()),
                });
                Vec::new()
            }
        }
    }

    pub fn peer_left(&mut self, peer: &str, out: &mut Vec<BridgeCommand>) -> Vec<NetEvent> {
        let Some(seat) = self.peers.remove(peer) else {
            self.rejected.remove(peer);
            return Vec::new();
        };
        let messages = self.session.disconnect(seat.side, seat.conn);
        self.deliver(messages, out)
    }

    pub fn client_message(
        &mut self,
        peer: &str,
        message: ClientMessage,
        out: &mut Vec<BridgeCommand>,
    ) -> Vec<NetEvent> {
        let Some(seat) = self.peers.get(peer) else {
            return Vec::new();
        };
        let side = seat.side;
        let messages = self.session.handle_message(side, message);
        self.deliver(messages, out)
    }

    fn peer_of(&self, side: Side) -> Option<&str> {
        self.peers
            .iter()
            .find(|(_, seat)| seat.side == side)
            .map(|(peer, _)| peer.as_str())
    }

    /// Routes session messages: the local side's messages become events,
    /// the remote side's messages go to the bridge (directed at the
    /// seated peer when known).
    fn deliver(
        &self,
        messages: Vec<(Side, ServerMessage)>,
        out: &mut Vec<BridgeCommand>,
    ) -> Vec<NetEvent> {
        let mut events = Vec::new();
        for (side, message) in messages {
            if side == self.local_side {
                events.push(NetEvent::Message(message));
            } else {
                out.push(BridgeCommand::Server {
                    message,
                    peer: self.peer_of(side).map(str::to_owned),
                });
            }
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use hibedi_core::{Board, Status};

    use super::*;

    /// A live 1x3 board where red starts and has a legal first placement.
    fn live_game() -> hibedi_core::Game {
        hibedi_core::Game::from_board(Board::from_rows(&["R.G"]), Side::Red)
    }

    fn new_host(out: &mut Vec<BridgeCommand>) -> HostGame {
        let (host, _) = HostGame::new(out);
        host
    }

    fn server_messages(out: &[BridgeCommand]) -> Vec<(&ServerMessage, Option<&str>)> {
        out.iter()
            .filter_map(|command| match command {
                BridgeCommand::Server { message, peer } => Some((message, peer.as_deref())),
                _ => None,
            })
            .collect()
    }

    fn state_events(events: &[NetEvent]) -> Vec<&ServerMessage> {
        events
            .iter()
            .filter_map(|event| match event {
                NetEvent::Message(message) => Some(message),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn hosting_starts_in_the_waiting_state() {
        let mut out = Vec::new();
        let (host, events) = HostGame::new(&mut out);
        assert!(out.is_empty(), "nothing to send before a guest joins");
        let messages = state_events(&events);
        assert_eq!(messages.len(), 2, "welcome plus initial state");
        assert!(matches!(
            messages.first(),
            Some(ServerMessage::Welcome { side: Side::Red })
        ));
        match messages.last() {
            Some(ServerMessage::State {
                game, side, status, ..
            }) => {
                assert!(game.is_none());
                assert_eq!(*side, Side::Red);
                assert_eq!(*status, Status::Waiting);
            }
            other => panic!("unexpected message: {other:?}"),
        }
        assert_eq!(host.local_side, Side::Red);
    }

    #[test]
    fn a_joined_guest_gets_seated_and_the_game_starts() {
        let mut out = Vec::new();
        let (mut host, _) = HostGame::new(&mut out);

        let events = host.peer_joined("guest-1", &mut out);

        let sent = server_messages(&out);
        assert_eq!(sent.len(), 2, "welcome and state for the guest");
        assert!(matches!(
            sent[0],
            (ServerMessage::Welcome { side: Side::Green }, Some("guest-1"))
        ));
        assert!(matches!(
            sent[1],
            (ServerMessage::State { .. }, Some("guest-1"))
        ));

        // The host's own view: seated red, game running.
        let messages = state_events(&events);
        match messages.last() {
            Some(ServerMessage::State {
                game,
                side,
                status,
                ..
            }) => {
                assert!(game.is_some());
                assert_eq!(*side, Side::Red);
                assert_eq!(*status, Status::Playing);
            }
            other => panic!("unexpected message: {other:?}"),
        }
    }

    #[test]
    fn guest_moves_are_validated_and_broadcast() {
        let mut out = Vec::new();
        let (mut host, _) = HostGame::new(&mut out);
        host.peer_joined("guest-1", &mut out);
        out.clear();

        // Replace the random game with a deterministic one where red moves
        // first and (2, 1) is a legal red placement.
        host.session.set_game(live_game());

        // Green cannot move on red's turn; the error goes to the guest only.
        let events = host.client_message(
            "guest-1",
            ClientMessage::Place { x: 2, y: 1 },
            &mut out,
        );
        assert!(events.is_empty());
        let sent = server_messages(&out);
        assert_eq!(sent.len(), 1);
        assert!(matches!(
            sent[0],
            (ServerMessage::Error { .. }, Some("guest-1"))
        ));

        // Red's legal placement is broadcast to both seats.
        out.clear();
        let events = host.local_message(ClientMessage::Place { x: 2, y: 1 }, &mut out);
        let sent = server_messages(&out);
        assert_eq!(sent.len(), 1, "state for the guest");
        assert!(matches!(sent[0], (ServerMessage::State { .. }, Some("guest-1"))));
        let messages = state_events(&events);
        assert!(matches!(messages.last(), Some(ServerMessage::State { .. })));
    }

    #[test]
    fn a_leaving_guest_pauses_the_game() {
        let mut out = Vec::new();
        let (mut host, _) = HostGame::new(&mut out);
        host.peer_joined("guest-1", &mut out);
        out.clear();
        host.session.set_game(live_game());
        let held = host.session.game().cloned();

        let events = host.peer_left("guest-1", &mut out);

        let messages = state_events(&events);
        match messages.last() {
            Some(ServerMessage::State {
                game,
                opponent_present,
                status,
                ..
            }) => {
                assert_eq!(game.as_ref(), held.as_ref());
                assert!(!opponent_present);
                assert_eq!(*status, Status::Paused);
            }
            other => panic!("unexpected message: {other:?}"),
        }
        assert!(out.is_empty(), "the leaving guest gets nothing");
    }

    #[test]
    fn a_third_peer_is_rejected_and_ignored() {
        let mut out = Vec::new();
        let (mut host, _) = HostGame::new(&mut out);
        host.peer_joined("guest-1", &mut out);
        out.clear();

        let events = host.peer_joined("stranger", &mut out);
        assert!(events.is_empty());
        let sent = server_messages(&out);
        assert_eq!(sent.len(), 1);
        assert!(matches!(
            sent[0],
            (ServerMessage::Error { .. }, Some("stranger"))
        ));

        // The rejected peer's messages are dropped.
        out.clear();
        let events = host.client_message(
            "stranger",
            ClientMessage::Place { x: 0, y: 0 },
            &mut out,
        );
        assert!(events.is_empty());
        assert!(out.is_empty());

        // And its departure changes nothing.
        let events = host.peer_left("stranger", &mut out);
        assert!(events.is_empty());
        assert!(out.is_empty());
    }
}

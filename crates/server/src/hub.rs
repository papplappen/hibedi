//! The relay's room: a fixed number of seats and one versioned state
//! (see the protocol crate). A connecting client takes the seat it asks
//! for (its old one, after a reconnect) if free, else the first free
//! seat, and frees it when it disconnects; a client finding no free seat
//! is refused.

use hibedi_relay_protocol::ServerMessage;
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};

pub type Tx = mpsc::UnboundedSender<ServerMessage>;

/// A seat index; it identifies its holder until the holder leaves.
pub type Seat = usize;

pub enum Command {
    /// Seats a new connection, preferring the seat it asks for; replies
    /// `None` when every seat is taken.
    Join {
        tx: Tx,
        wanted: Option<Seat>,
        reply: oneshot::Sender<Option<Seat>>,
    },
    /// A state offered by the client in `seat`.
    Offer {
        seat: Seat,
        version: u64,
        state: Value,
    },
    /// Frees the seat of a closed connection.
    Leave { seat: Seat },
}

pub async fn run(capacity: usize, mut commands: mpsc::UnboundedReceiver<Command>) {
    let mut room = Room::new(capacity);
    while let Some(command) = commands.recv().await {
        match command {
            Command::Join { tx, wanted, reply } => {
                let _ = reply.send(room.join(tx, wanted));
            }
            Command::Offer {
                seat,
                version,
                state,
            } => room.offer(seat, version, state),
            Command::Leave { seat } => room.leave(seat),
        }
    }
}

struct Room {
    seats: Vec<Option<Tx>>,
    /// The kept state and its version.
    state: Option<(u64, Value)>,
}

impl Room {
    fn new(capacity: usize) -> Self {
        Self {
            seats: vec![None; capacity],
            state: None,
        }
    }

    fn join(&mut self, tx: Tx, wanted: Option<Seat>) -> Option<Seat> {
        let is_free = |seat: Seat| self.seats.get(seat).is_some_and(Option::is_none);
        let seat = wanted
            .filter(|&seat| is_free(seat))
            .or_else(|| self.seats.iter().position(Option::is_none))?;
        if let Some(message) = self.state_message() {
            let _ = tx.send(message);
        }
        self.seats[seat] = Some(tx);
        self.announce_seats();
        Some(seat)
    }

    fn leave(&mut self, seat: Seat) {
        self.seats[seat] = None;
        // An empty room starts over: its state leaves with its players.
        if self.seats.iter().all(Option::is_none) {
            self.state = None;
        }
        self.announce_seats();
    }

    fn offer(&mut self, from: Seat, version: u64, state: Value) {
        let newer = self.state.as_ref().is_none_or(|(kept, _)| version > *kept);
        if newer {
            self.state = Some((version, state));
        }
        let message = self
            .state_message()
            .expect("a state is kept after an offer");
        for (seat, tx) in self.occupied() {
            // A kept offer goes to everyone else; a stale one is answered
            // with the kept state.
            if (seat == from) != newer {
                let _ = tx.send(message.clone());
            }
        }
    }

    fn state_message(&self) -> Option<ServerMessage> {
        let (version, state) = self.state.clone()?;
        Some(ServerMessage::State { version, state })
    }

    fn announce_seats(&self) {
        let occupied: Vec<bool> = self.seats.iter().map(Option::is_some).collect();
        for (seat, tx) in self.occupied() {
            let _ = tx.send(ServerMessage::Seats {
                seat,
                occupied: occupied.clone(),
            });
        }
    }

    fn occupied(&self) -> impl Iterator<Item = (Seat, &Tx)> {
        self.seats
            .iter()
            .enumerate()
            .filter_map(|(seat, tx)| Some((seat, tx.as_ref()?)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client() -> (Tx, mpsc::UnboundedReceiver<ServerMessage>) {
        mpsc::unbounded_channel()
    }

    fn drain(rx: &mut mpsc::UnboundedReceiver<ServerMessage>) -> Vec<ServerMessage> {
        std::iter::from_fn(|| rx.try_recv().ok()).collect()
    }

    fn states(rx: &mut mpsc::UnboundedReceiver<ServerMessage>) -> Vec<u64> {
        drain(rx)
            .into_iter()
            .filter_map(|message| match message {
                ServerMessage::State { version, .. } => Some(version),
                _ => None,
            })
            .collect()
    }

    fn state(n: u64) -> Value {
        serde_json::json!({ "turn": n })
    }

    #[test]
    fn seats_fill_in_order_refuse_when_full_and_reopen() {
        let mut room = Room::new(2);
        let (a, _a_rx) = client();
        let (b, _b_rx) = client();
        let (c, _c_rx) = client();
        let (d, _d_rx) = client();

        assert_eq!(room.join(a, None), Some(0));
        assert_eq!(room.join(b, None), Some(1));
        assert_eq!(room.join(c, None), None);

        room.leave(0);
        assert_eq!(room.join(d, None), Some(0));
    }

    #[test]
    fn reconnecting_clients_get_their_seats_back() {
        // A relay restart drops both clients; whichever reconnects first
        // must not take the other's seat.
        let mut room = Room::new(2);
        let (green, _green_rx) = client();
        let (red, _red_rx) = client();
        assert_eq!(room.join(green, Some(1)), Some(1));
        assert_eq!(room.join(red, Some(0)), Some(0));
    }

    #[test]
    fn newer_offers_go_to_the_others_and_stale_ones_get_the_kept_state() {
        let mut room = Room::new(2);
        let (a, mut a_rx) = client();
        let (b, mut b_rx) = client();
        room.join(a, None);
        room.join(b, None);

        room.offer(0, 1, state(1));
        assert_eq!(states(&mut a_rx), Vec::<u64>::new());
        assert_eq!(states(&mut b_rx), vec![1]);

        // A concurrent offer of the same version loses, and its sender
        // learns the kept state.
        room.offer(1, 1, state(99));
        assert_eq!(states(&mut a_rx), Vec::<u64>::new());
        assert_eq!(states(&mut b_rx), vec![1]);
    }

    #[test]
    fn a_rejoining_client_gets_the_kept_state() {
        let mut room = Room::new(2);
        let (a, _a_rx) = client();
        let (b, _b_rx) = client();
        room.join(a, None);
        room.join(b, None);
        room.offer(0, 5, state(5));

        room.leave(1);
        let (b, mut b_rx) = client();
        room.join(b, Some(1));
        assert_eq!(states(&mut b_rx), vec![5]);
    }

    #[test]
    fn after_a_restart_the_newest_reoffered_state_wins() {
        // Both clients reconnect to a fresh relay and re-offer what they
        // hold; the one that missed the last move must catch up, in
        // either order.
        for stale_first in [true, false] {
            let mut room = Room::new(2);
            let (a, mut a_rx) = client();
            let (b, mut b_rx) = client();
            room.join(a, Some(0));
            room.join(b, Some(1));
            drain(&mut a_rx);
            drain(&mut b_rx);

            let (fresh, stale) = ((0, 10), (1, 9));
            for (seat, version) in if stale_first {
                [stale, fresh]
            } else {
                [fresh, stale]
            } {
                room.offer(seat, version, state(version));
            }

            assert_eq!(states(&mut b_rx).last(), Some(&10));
            assert_eq!(room.state.as_ref().map(|(version, _)| *version), Some(10));
        }
    }
}

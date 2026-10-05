//! Tokio relay hub: seats at most two connections (first red, second
//! green) and forwards state announcements from one seat to the other.
//! The relay holds no game state: each client plays its local game and
//! the announcements keep the seats in sync.

use hibedi_core::{ServerMessage, Side, StateSync};
use tokio::sync::{mpsc, oneshot};

pub type Tx = mpsc::UnboundedSender<ServerMessage>;

pub type ConnId = u64;

pub enum Command {
    /// Seats a freshly upgraded connection.
    Claim {
        tx: Tx,
        reply: oneshot::Sender<Claimed>,
    },
    /// A state announcement from one seat, to relay to the other.
    Sync { from: ConnId, sync: StateSync },
    /// Frees the seat of a dropped connection.
    Disconnect { conn: ConnId },
}

pub enum Claimed {
    Seated { conn: ConnId },
    Full,
}

pub async fn run(mut commands: mpsc::UnboundedReceiver<Command>) {
    let mut seats: [Option<(ConnId, Tx)>; 2] = [None, None];
    let mut next_conn: ConnId = 0;

    while let Some(command) = commands.recv().await {
        match command {
            Command::Claim { tx, reply } => match seats.iter().position(Option::is_none) {
                Some(index) => {
                    let conn = next_conn;
                    next_conn += 1;
                    let side = if index == 0 { Side::Red } else { Side::Green };
                    seats[index] = Some((conn, tx.clone()));
                    let _ = tx.send(ServerMessage::Hello { side });
                    if let Some((_, other)) = &seats[1 - index] {
                        let _ = other.send(ServerMessage::PeerJoined);
                    }
                    let _ = reply.send(Claimed::Seated { conn });
                }
                None => {
                    let _ = reply.send(Claimed::Full);
                }
            },
            Command::Sync { from, sync } => {
                for seat in &seats {
                    if let Some((conn, tx)) = seat
                        && *conn != from
                    {
                        let _ = tx.send(ServerMessage::Sync(sync.clone()));
                    }
                }
            }
            Command::Disconnect { conn } => {
                let index = seats
                    .iter()
                    .position(|seat| matches!(seat, Some((c, _)) if *c == conn));
                if let Some(index) = index {
                    seats[index] = None;
                    if let Some((_, other)) = &seats[1 - index] {
                        let _ = other.send(ServerMessage::PeerLeft);
                    }
                }
            }
        }
    }
}

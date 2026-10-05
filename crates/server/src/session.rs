//! Tokio hub around the shared [`Session`]: feeds commands from the
//! WebSocket handlers into the session and routes the resulting messages
//! back to the seated connections.

use std::collections::HashMap;

use hibedi_core::session::{ClaimOutcome, ConnId, Session};
use hibedi_core::{ClientMessage, ServerMessage, Side};
use tokio::sync::{mpsc, oneshot};

pub type Tx = mpsc::UnboundedSender<ServerMessage>;

pub enum Command {
    /// Seats a freshly upgraded connection.
    Claim {
        tx: Tx,
        reply: oneshot::Sender<Claimed>,
    },
    Message {
        side: Side,
        msg: ClientMessage,
    },
    Disconnect {
        side: Side,
        conn: ConnId,
    },
}

pub enum Claimed {
    Seated { side: Side, conn: ConnId },
    Full,
}

pub async fn run(mut commands: mpsc::UnboundedReceiver<Command>) {
    let mut session = Session::new();
    let mut connections: HashMap<ConnId, Tx> = HashMap::new();
    let mut next_conn: ConnId = 0;

    while let Some(command) = commands.recv().await {
        match command {
            Command::Claim { tx, reply } => {
                let conn = next_conn;
                next_conn += 1;
                match session.claim(conn) {
                    ClaimOutcome::Seated { side, messages } => {
                        connections.insert(conn, tx);
                        deliver(&session, &connections, messages);
                        let _ = reply.send(Claimed::Seated { side, conn });
                    }
                    ClaimOutcome::Full => {
                        let _ = reply.send(Claimed::Full);
                    }
                }
            }
            Command::Message { side, msg } => {
                let messages = session.handle_message(side, msg);
                deliver(&session, &connections, messages);
            }
            Command::Disconnect { side, conn } => {
                connections.remove(&conn);
                let messages = session.disconnect(side, conn);
                deliver(&session, &connections, messages);
            }
        }
    }
}

/// Routes session messages to the connection seated on each message's side.
fn deliver(
    session: &Session,
    connections: &HashMap<ConnId, Tx>,
    messages: Vec<(Side, ServerMessage)>,
) {
    for (side, message) in messages {
        if let Some(conn) = session.seat_conn(side)
            && let Some(tx) = connections.get(&conn)
        {
            let _ = tx.send(message);
        }
    }
}

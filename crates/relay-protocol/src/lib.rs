//! Wire protocol between native clients and the relay server
//! (crates/server). Game-agnostic: the relay runs one room of seats and
//! keeps one opaque JSON state the seated clients share.
//!
//! The state is a versioned register, the same model as the web's
//! peer-to-peer rooms (web/room.js): a client offers a state with a
//! version one above the version it holds; the relay keeps an offer only
//! if it is newer than its own state and passes it to the other seats,
//! and answers a stale offer with the state it keeps. A reconnecting
//! client re-offers the state it holds, so whichever side is newer, the
//! relay (still serving the other player) or the client (after a relay
//! restart), brings the other up to date.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Client → relay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// The first message on a connection: asks for a seat. A client
    /// that reconnects names the seat it held, and gets it back if it is
    /// still free; otherwise it gets the first free seat.
    Hello { seat: Option<usize> },
    /// Offers a state; kept if `version` is newer than the relay's.
    Offer { version: u64, state: Value },
}

/// Relay → client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// The seating changed: `seat` is the receiver's own seat,
    /// `occupied` tells for every seat whether someone holds it.
    Seats { seat: usize, occupied: Vec<bool> },
    /// The relay's state: on joining, when another seat's offer was
    /// kept, or in answer to a stale offer.
    State { version: u64, state: Value },
    /// Every seat is taken; the relay closes the connection.
    Full,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_round_trip() {
        let state = serde_json::json!({ "board": "R.G" });
        let client = [
            ClientMessage::Hello { seat: Some(1) },
            ClientMessage::Offer {
                version: 3,
                state: state.clone(),
            },
        ];
        for message in client {
            let json = serde_json::to_string(&message).unwrap();
            assert_eq!(
                serde_json::from_str::<ClientMessage>(&json).unwrap(),
                message
            );
        }

        let server = [
            ServerMessage::Seats {
                seat: 1,
                occupied: vec![true, true],
            },
            ServerMessage::State { version: 3, state },
            ServerMessage::Full,
        ];
        for message in server {
            let json = serde_json::to_string(&message).unwrap();
            assert_eq!(
                serde_json::from_str::<ServerMessage>(&json).unwrap(),
                message
            );
        }
    }
}

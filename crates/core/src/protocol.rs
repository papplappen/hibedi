//! Protocol types shared by the transports: the replicated-state
//! announcement (the only game traffic) and the native relay server's
//! connection messages. The wasm bridge needs no envelopes: it routes
//! `StateSync` payloads as opaque values (see web/game.js).

use serde::{Deserialize, Serialize};

use crate::Side;

/// A replicated state announcement: the writer changed its local game and
/// publishes the result. `state` is opaque JSON (`Null` = no game yet);
/// the sync layer never inspects it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSync {
    pub version: u64,
    pub state: serde_json::Value,
}

/// Tracks our replica's version and decides whether an incoming
/// announcement should replace the local state.
///
/// Replication here is symmetric and roleless: every seat plays its local
/// game exactly like a hotseat game and announces whatever it changes.
/// Because the game is deterministic and turn-serialized, announcements
/// can never conflict — except for simultaneous game resets, which equal
/// versions expose; both sides then adopt the lexicographically greater
/// state, converging without negotiation.
#[derive(Debug, Default, Clone)]
pub struct Replica {
    pub version: u64,
}

impl Replica {
    /// Should an incoming announcement replace our state? `current` is
    /// our state as an opaque JSON value (`Null` = no game).
    pub fn should_adopt(&self, sync: &StateSync, current: &serde_json::Value) -> bool {
        if sync.version > self.version {
            return true;
        }
        if sync.version < self.version || sync.state == *current {
            return false;
        }
        json(&sync.state) > json(current)
    }

    /// Records an adopted announcement's version.
    pub fn adopt(&mut self, sync: &StateSync) {
        self.version = sync.version;
    }
}

fn json(value: &serde_json::Value) -> String {
    serde_json::to_string(value).expect("values always serialize")
}

/// Native relay server → client messages. On wasm the lobby assigns the
/// seat and delivers [`StateSync`] payloads directly, so none of these
/// are used there.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Connection established; our seat.
    Hello { side: Side },
    /// The other seat filled (or refilled after a reconnect).
    PeerJoined,
    /// The other seat is empty; the game pauses.
    PeerLeft,
    /// A state announcement from the other player.
    Sync(StateSync),
    /// Rejected: the relay seats at most two connections.
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Waiting,
    Playing,
    Paused,
    Finished,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sync(version: u64, state: serde_json::Value) -> StateSync {
        StateSync { version, state }
    }

    #[test]
    fn state_sync_round_trips() {
        let message = sync(7, serde_json::json!({ "board": "R.G" }));
        let json = serde_json::to_string(&message).unwrap();
        let back: StateSync = serde_json::from_str(&json).unwrap();
        assert_eq!(json, serde_json::to_string(&back).unwrap());
        assert_eq!(back.version, 7);
        assert_eq!(back.state, message.state);
    }

    #[test]
    fn newer_versions_are_adopted_older_ignored() {
        let replica = Replica { version: 3 };
        let current = serde_json::json!({ "v": 1 });

        assert!(replica.should_adopt(&sync(4, current.clone()), &current));
        assert!(!replica.should_adopt(&sync(2, current.clone()), &current));
    }

    #[test]
    fn equal_versions_with_equal_states_are_ignored() {
        let replica = Replica { version: 3 };
        let current = serde_json::json!({ "v": 1 });

        assert!(!replica.should_adopt(&sync(3, current.clone()), &current));
    }

    #[test]
    fn simultaneous_writers_converge_on_the_greater_state() {
        // Both sides bumped to version 4 in the same round-trip window
        // (their own reset) with different states: each receives the
        // other's version-4 announcement and must adopt the same one, so
        // the greater serialized state wins on both sides.
        let a = serde_json::json!({ "reset": "a" });
        let b = serde_json::json!({ "reset": "b" });
        let replica_a = Replica { version: 4 };
        let replica_b = Replica { version: 4 };

        let adopted_by_a = replica_a.should_adopt(&sync(4, b.clone()), &a);
        let adopted_by_b = replica_b.should_adopt(&sync(4, a.clone()), &b);
        assert_ne!(adopted_by_a, adopted_by_b, "exactly one side gives way");
    }

    #[test]
    fn server_messages_round_trip() {
        let messages = [
            ServerMessage::Hello { side: Side::Red },
            ServerMessage::PeerJoined,
            ServerMessage::PeerLeft,
            ServerMessage::Sync(sync(1, serde_json::json!(null))),
            ServerMessage::Full,
        ];

        for message in messages {
            let json = serde_json::to_string(&message).unwrap();
            let back: ServerMessage = serde_json::from_str(&json).unwrap();
            assert_eq!(json, serde_json::to_string(&back).unwrap());
        }
    }
}

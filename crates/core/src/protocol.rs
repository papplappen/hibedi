use serde::{Deserialize, Serialize};

use crate::{Direction, Game, Side, TokenId};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    Place { x: i32, y: i32 },
    Jump { direction: Direction },
    SelectToken { id: TokenId },
    NewGame,
    Leave,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Waiting,
    Playing,
    Paused,
    Finished,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum ServerMessage {
    Welcome {
        side: Side,
    },
    State {
        game: Option<Game>,
        side: Side,
        opponent_present: bool,
        status: Status,
    },
    Error {
        reason: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_message_round_trips() {
        let messages = [
            ClientMessage::Place { x: -1, y: 4 },
            ClientMessage::Jump {
                direction: Direction::Left,
            },
            ClientMessage::SelectToken { id: TokenId(7) },
            ClientMessage::NewGame,
            ClientMessage::Leave,
        ];

        for message in messages {
            let json = serde_json::to_string(&message).unwrap();
            let back: ClientMessage = serde_json::from_str(&json).unwrap();
            assert_eq!(json, serde_json::to_string(&back).unwrap());
        }

        assert_eq!(
            serde_json::to_string(&ClientMessage::Jump {
                direction: Direction::Right
            })
            .unwrap(),
            r#"{"type":"jump","direction":"right"}"#
        );
    }

    #[test]
    fn server_welcome_round_trips() {
        let message = ServerMessage::Welcome { side: Side::Green };
        let json = serde_json::to_string(&message).unwrap();
        assert_eq!(json, r#"{"type":"welcome","side":"green"}"#);
        let back: ServerMessage = serde_json::from_str(&json).unwrap();
        match back {
            ServerMessage::Welcome { side } => assert_eq!(side, Side::Green),
            other => panic!("unexpected message: {other:?}"),
        }
    }

    #[test]
    fn error_reason_is_human_readable() {
        let json = r#"{"type":"error","reason":"not your turn"}"#;
        let message: ServerMessage = serde_json::from_str(json).unwrap();
        match message {
            ServerMessage::Error { reason } => assert_eq!(reason, "not your turn"),
            other => panic!("unexpected message: {other:?}"),
        }
    }
}
